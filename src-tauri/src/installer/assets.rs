use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Error, Debug)]
pub enum AssetError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to deserialize asset index: {0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("HTTP error: status {status}")]
    HttpError { status: u16 },
    #[error("Hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AssetError>;

// ─── Asset index structures ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexData {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ─── Download progress ─────────────────────────────────────────────

pub struct DownloadProgress {
    pub done: AtomicUsize,
    pub total: usize,
    pub errors: AtomicUsize,
}

impl DownloadProgress {
    pub fn new(total: usize) -> Self {
        Self {
            done: AtomicUsize::new(0),
            total,
            errors: AtomicUsize::new(0),
        }
    }

    pub fn inc_done(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn done_count(&self) -> usize {
        self.done.load(Ordering::Relaxed)
    }

    pub fn error_count(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }
}

// ─── Asset downloader ──────────────────────────────────────────────

pub struct AssetDownloader {
    client: Client,
    assets_dir: PathBuf,
}

impl AssetDownloader {
    pub fn new(client: Client, assets_dir: PathBuf) -> Self {
        Self { client, assets_dir }
    }

    pub fn asset_path(&self, hash: &str) -> PathBuf {
        self.assets_dir
            .join("objects")
            .join(&hash[0..2])
            .join(hash)
    }

    pub fn index_path(&self, index_id: &str) -> PathBuf {
        self.assets_dir
            .join("indexes")
            .join(format!("{}.json", index_id))
    }

    pub async fn verify_file(&self, path: &Path, expected_hash: &str, expected_size: u64) -> bool {
        let meta = match fs::metadata(path).await {
            Ok(m) => m,
            Err(_) => return false,
        };
        if meta.len() != expected_size {
            return false;
        }
        let data = match fs::read(path).await {
            Ok(d) => d,
            Err(_) => return false,
        };
        let mut hasher = Sha1::new();
        hasher.update(&data);
        let result = format!("{:x}", hasher.finalize());
        result.eq_ignore_ascii_case(expected_hash)
    }

    pub async fn download_index(&self, url: &str, index_id: &str) -> Result<AssetIndexData> {
        let path = self.index_path(index_id);
        if let Ok(meta) = fs::metadata(&path).await {
            if meta.len() > 100 {
                let data = fs::read(&path).await?;
                return Ok(serde_json::from_slice(&data)?);
            }
        }

        let resp = self.client.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(AssetError::HttpError {
                status: resp.status().as_u16(),
            });
        }
        let bytes = resp.bytes().await?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let tmp_path = path.with_extension("json.tmp");
        let mut file = fs::File::create(&tmp_path).await?;
        file.write_all(&bytes).await?;
        file.flush().await?;
        fs::rename(&tmp_path, &path).await?;

        Ok(serde_json::from_slice(&bytes)?)
    }

    pub async fn download_all(
        &self,
        index: &AssetIndexData,
        concurrency: usize,
    ) -> Result<Arc<DownloadProgress>> {
        self.download_all_with_progress(index, concurrency, |_, _, _| {}).await
    }

    pub async fn download_all_with_progress(
        &self,
        index: &AssetIndexData,
        concurrency: usize,
        progress_fn: impl Fn(usize, usize, usize) + Send + Sync + 'static,
    ) -> Result<Arc<DownloadProgress>> {
        fs::create_dir_all(self.assets_dir.join("objects")).await?;

        let mut to_download: Vec<(String, AssetObject)> = Vec::new();
        let progress = Arc::new(DownloadProgress::new(index.objects.len()));

        for (name, obj) in &index.objects {
            let path = self.asset_path(&obj.hash);
            if self.verify_file(&path, &obj.hash, obj.size).await {
                progress.inc_done();
            } else {
                to_download.push((name.clone(), obj.clone()));
            }
        }

        if to_download.is_empty() {
            return Ok(progress);
        }

        let client = self.client.clone();
        let assets_dir = self.assets_dir.clone();
        let progress = Arc::clone(&progress);
        let progress_fn = Arc::new(progress_fn);

        stream::iter(to_download)
            .map(|(_name, obj)| {
                let client = client.clone();
                let assets_dir = assets_dir.clone();
                let progress = Arc::clone(&progress);
                let progress_fn = Arc::clone(&progress_fn);
                async move {
                    let result =
                        download_asset_with_retry(&client, &assets_dir, &obj, 5).await;
                    if result.is_err() {
                        progress.inc_errors();
                    }
                    progress.inc_done();
                    progress_fn(
                        progress.done_count(),
                        progress.total,
                        progress.error_count(),
                    );
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;

        Ok(progress)
    }
}

async fn download_asset_with_retry(
    client: &Client,
    assets_dir: &PathBuf,
    obj: &AssetObject,
    max_retries: u32,
) -> Result<()> {
    let hash = &obj.hash;
    let target_path = assets_dir
        .join("objects")
        .join(&hash[0..2])
        .join(hash);

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let url = format!(
        "https://resources.download.minecraft.net/{}/{}",
        &hash[0..2],
        hash
    );

    let mut last_err = None;
    for attempt in 0..max_retries {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp.bytes().await?;
                let tmp_path = target_path.with_extension("tmp");
                let mut file = fs::File::create(&tmp_path).await?;
                file.write_all(&bytes).await?;
                file.flush().await?;
                fs::rename(&tmp_path, &target_path).await?;

                let mut hasher = Sha1::new();
                hasher.update(&bytes);
                let actual = format!("{:x}", hasher.finalize());
                if !actual.eq_ignore_ascii_case(hash) {
                    let _ = fs::remove_file(&target_path).await;
                    return Err(AssetError::HashMismatch {
                        path: target_path.display().to_string(),
                        expected: hash.clone(),
                        actual,
                    });
                }
                return Ok(());
            }
            Ok(resp) if resp.status().as_u16() == 429 => {
                let wait = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(wait).await;
                last_err = Some(AssetError::HttpError {
                    status: resp.status().as_u16(),
                });
            }
            Ok(resp) if resp.status().as_u16() == 403 => {
                return Err(AssetError::HttpError {
                    status: resp.status().as_u16(),
                });
            }
            Ok(resp) => {
                last_err = Some(AssetError::HttpError {
                    status: resp.status().as_u16(),
                });
            }
            Err(e) => {
                let wait = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                tokio::time::sleep(wait).await;
                last_err = Some(AssetError::Reqwest(e));
            }
        }
    }

    let _ = fs::remove_file(target_path.with_extension("tmp")).await;
    Err(last_err.unwrap_or(AssetError::HttpError { status: 0 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_asset_path() {
        let dir = PathBuf::from("/tmp/assets");
        let dl = AssetDownloader::new(Client::new(), dir);
        let path = dl.asset_path("abcdef1234567890");
        assert_eq!(
            path,
            PathBuf::from("/tmp/assets/objects/ab/cdef1234567890")
        );
    }

    #[test]
    fn test_index_path() {
        let dir = PathBuf::from("/tmp/assets");
        let dl = AssetDownloader::new(Client::new(), dir);
        let path = dl.index_path("1234");
        assert_eq!(path, PathBuf::from("/tmp/assets/indexes/1234.json"));
    }

    #[tokio::test]
    async fn test_verify_file_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        let data = b"hello world";
        let mut f = File::create(&file_path).unwrap();
        f.write_all(data).unwrap();
        drop(f);

        let dl = AssetDownloader::new(Client::new(), dir.path().to_path_buf());
        let mut hasher = Sha1::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());

        assert!(dl.verify_file(&file_path, &hash, data.len() as u64).await);
    }

    #[tokio::test]
    async fn test_verify_file_wrong_hash() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        let data = b"hello world";
        let mut f = File::create(&file_path).unwrap();
        f.write_all(data).unwrap();
        drop(f);

        let dl = AssetDownloader::new(Client::new(), dir.path().to_path_buf());
        assert!(!dl.verify_file(&file_path, "0000000000000000000000000000000000000000", data.len() as u64).await);
    }

    #[tokio::test]
    async fn test_verify_file_wrong_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.bin");
        let data = b"hello world";
        let mut f = File::create(&file_path).unwrap();
        f.write_all(data).unwrap();
        drop(f);

        let dl = AssetDownloader::new(Client::new(), dir.path().to_path_buf());
        let mut hasher = Sha1::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());

        assert!(!dl.verify_file(&file_path, &hash, 999).await);
    }

    #[test]
    fn test_download_progress() {
        let progress = DownloadProgress::new(10);
        assert_eq!(progress.done_count(), 0);
        assert_eq!(progress.error_count(), 0);
        assert_eq!(progress.total, 10);

        progress.inc_done();
        progress.inc_done();
        progress.inc_errors();

        assert_eq!(progress.done_count(), 2);
        assert_eq!(progress.error_count(), 1);
    }
}
