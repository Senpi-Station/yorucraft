use std::path::{Path, PathBuf};
use std::env;

use reqwest::Client;
use sha1::{Digest, Sha1};
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

use crate::installer::manifest::{Library, Rule, VersionData};

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("HTTP request failed: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid Maven coordinate: {0}")]
    ParseMaven(String),
    #[error("Failed to download library from all mirrors")]
    DownloadFailed,
    #[error("Hash mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("Failed to extract native library: {0}")]
    ExtractionFailed(String),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, LibraryError>;

// ─── Resolved library ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ResolvedLibrary {
    pub path: PathBuf,
    pub is_native: bool,
    pub extract_to: Option<PathBuf>,
}

// ─── Library resolver ──────────────────────────────────────────────

pub struct LibraryResolver {
    client: Client,
    libraries_dir: PathBuf,
}

impl LibraryResolver {
    pub fn new(client: Client, libraries_dir: PathBuf) -> Self {
        Self {
            client,
            libraries_dir,
        }
    }

    pub fn parse_maven(coordinate: &str) -> Result<(String, String, String, Option<String>)> {
        let parts: Vec<&str> = coordinate.split(':').collect();
        if parts.len() < 3 {
            return Err(LibraryError::ParseMaven(format!(
                "Expected at least 3 colon-separated parts, got {}",
                parts.len()
            )));
        }
        let group = parts[0].to_string();
        let artifact = parts[1].to_string();
        let version = parts[2].to_string();
        let classifier = if parts.len() > 3 {
            Some(parts[3].to_string())
        } else {
            None
        };
        Ok((group, artifact, version, classifier))
    }

    pub fn artifact_path(&self, coordinate: &str, downloads: Option<&manifest::LibraryDownloads>) -> PathBuf {
        if let Some(dl) = downloads {
            if let Some(ref path) = dl.path {
                return self.libraries_dir.join(path);
            }
        }
        let (group, artifact, version, classifier) = Self::parse_maven(coordinate)
            .expect("invalid coordinate in artifact_path");
        let group_path = group.replace('.', '/');
        let filename = match classifier {
            Some(ref c) => format!("{}-{}-{}.jar", artifact, version, c),
            None => format!("{}-{}.jar", artifact, version),
        };
        self.libraries_dir
            .join(&group_path)
            .join(&artifact)
            .join(&version)
            .join(filename)
    }

    pub fn download_urls(&self, coordinate: &str, path: &str, custom_url: Option<&str>) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(url) = custom_url {
            let base = if url.ends_with('/') { url } else { &format!("{}/", url) };
            urls.push(format!("{}{}", base, path));
        }
        urls.push(format!("https://libraries.minecraft.net/{}", path));
        urls.push(format!("https://repo.maven.apache.org/maven2/{}", path));
        urls
    }

    pub async fn verify_jar(&self, path: &Path, expected_sha1: Option<&str>) -> bool {
        let meta = match fs::metadata(path).await {
            Ok(m) => m,
            Err(_) => return false,
        };
        if meta.len() == 0 {
            return false;
        }
        if let Some(expected) = expected_sha1 {
            let data = match fs::read(path).await {
                Ok(d) => d,
                Err(_) => return false,
            };
            let mut hasher = Sha1::new();
            hasher.update(&data);
            let actual = format!("{:x}", hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                return false;
            }
        }
        true
    }

    pub fn should_include(rules: &[Rule]) -> bool {
        if rules.is_empty() {
            return true;
        }
        let mut result = false;
        let current_os = match env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        let current_arch = env::consts::ARCH;

        for rule in rules {
            let os_match = rule.os.as_ref().map_or(true, |os_rule| {
                let name_ok = os_rule.name.as_ref().map_or(true, |n| n == current_os);
                let arch_ok = os_rule.arch.as_ref().map_or(true, |a| a == current_arch);
                name_ok && arch_ok
            });

            if rule.action == "allow" {
                if os_match {
                    result = true;
                }
            } else if rule.action == "disallow" {
                if os_match {
                    result = false;
                }
            }
        }
        result
    }

    pub fn resolve_natives(libraries: &[Library]) -> Vec<(&Library, String)> {
        let current_os = match env::consts::OS {
            "macos" => "osx",
            other => other,
        };
        let arch_str = if env::consts::ARCH == "x86_64" || env::consts::ARCH == "aarch64" {
            "64"
        } else {
            "32"
        };

        let mut result = Vec::new();
        for lib in libraries {
            if let Some(ref natives) = lib.natives {
                if let Some(classifier_template) = natives.get(current_os) {
                    let classifier = classifier_template.replace("${arch}", arch_str);
                    result.push((lib, classifier));
                }
            }
        }
        result
    }

    pub async fn resolve_all(&self, version_data: &VersionData) -> Result<Vec<ResolvedLibrary>> {
        let mut resolved = Vec::new();
        let native_entries = Self::resolve_natives(&version_data.libraries);

        for lib in &version_data.libraries {
            if let Some(ref rules) = lib.rules {
                if !Self::should_include(rules) {
                    continue;
                }
            }

            let coordinate = &lib.name;
            let path_buf = self.artifact_path(coordinate, lib.downloads.as_ref());
            let relative_path = path_buf
                .strip_prefix(&self.libraries_dir)
                .unwrap_or(&path_buf)
                .to_string_lossy()
                .to_string();

            let expected_sha1 = lib
                .downloads
                .as_ref()
                .and_then(|d| d.sha1.as_deref());

            if !self.verify_jar(&path_buf, expected_sha1).await {
                let custom_url = lib.url.as_deref();
                let urls = self.download_urls(coordinate, &relative_path, custom_url);
                self.download_with_retry(&urls, &path_buf, expected_sha1).await?;
            }

            let is_native = native_entries.iter().any(|(n, _)| n.name == lib.name);
            let extract_to = None;

            resolved.push(ResolvedLibrary {
                path: path_buf,
                is_native,
                extract_to,
            });
        }

        Ok(resolved)
    }

    pub fn build_classpath(resolved: &[ResolvedLibrary], client_jar: &Path) -> String {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let mut paths: Vec<String> = resolved
            .iter()
            .filter(|r| !r.is_native)
            .map(|r| r.path.display().to_string())
            .collect();
        paths.push(client_jar.display().to_string());
        paths.join(separator)
    }

    async fn download_with_retry(
        &self,
        urls: &[String],
        target: &Path,
        expected_sha1: Option<&str>,
    ) -> Result<()> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }

        for url in urls {
            let mut last_err = None;
            for attempt in 0..3u32 {
                match self.client.get(url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        let bytes = resp.bytes().await?;
                        let tmp_path = target.with_extension("tmp");
                        let mut file = fs::File::create(&tmp_path).await?;
                        file.write_all(&bytes).await?;
                        file.flush().await?;
                        fs::rename(&tmp_path, target).await?;

                        if let Some(expected) = expected_sha1 {
                            let mut hasher = Sha1::new();
                            hasher.update(&bytes);
                            let actual = format!("{:x}", hasher.finalize());
                            if !actual.eq_ignore_ascii_case(expected) {
                                let _ = fs::remove_file(target).await;
                                return Err(LibraryError::HashMismatch {
                                    path: target.display().to_string(),
                                    expected: expected.to_string(),
                                    actual,
                                });
                            }
                        }
                        return Ok(());
                    }
                    Ok(resp) => {
                        let wait = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                        tokio::time::sleep(wait).await;
                        last_err = Some(LibraryError::Reqwest(resp.error_for_status().unwrap_err()));
                    }
                    Err(e) => {
                        let wait = std::time::Duration::from_millis(100 * 2u64.pow(attempt));
                        tokio::time::sleep(wait).await;
                        last_err = Some(LibraryError::Reqwest(e));
                    }
                }
            }
            if last_err.is_some() {
                continue;
            }
        }

        Err(LibraryError::DownloadFailed)
    }

    pub async fn extract_natives(
        &self,
        jar_path: &Path,
        natives_dir: &Path,
        exclude: &[String],
    ) -> Result<Vec<PathBuf>> {
        let file = std::fs::File::open(jar_path)?;
        let mut archive = ZipArchive::new(file)?;
        let mut extracted = Vec::new();

        fs::create_dir_all(natives_dir).await?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let entry_path = entry.name().to_string();

            if entry_path.contains("META-INF/") {
                continue;
            }
            if exclude.iter().any(|e| entry_path.starts_with(e.as_str())) {
                continue;
            }

            let ext = Path::new(&entry_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if !matches!(ext, "dll" | "so" | "dylib" | "jnilib") {
                continue;
            }

            let filename = Path::new(&entry_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&entry_path);

            let target = natives_dir.join(filename);

            #[cfg(target_os = "windows")]
            {
                if target.exists() {
                    let tmp = target.with_extension("dll.tmp");
                    if fs::rename(&target, &tmp).await.is_ok() {
                        let _ = fs::remove_file(&tmp).await;
                    }
                }
            }

            let mut contents = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut contents)?;
            let mut file = fs::File::create(&target).await?;
            file.write_all(&contents).await?;
            file.flush().await?;

            extracted.push(target);
        }

        Ok(extracted)
    }
}

use crate::installer::manifest;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_maven_simple() {
        let (group, artifact, version, classifier) =
            LibraryResolver::parse_maven("com.mojang:authlib:6.0.52").unwrap();
        assert_eq!(group, "com.mojang");
        assert_eq!(artifact, "authlib");
        assert_eq!(version, "6.0.52");
        assert_eq!(classifier, None);
    }

    #[test]
    fn test_parse_maven_with_classifier() {
        let (group, artifact, version, classifier) =
            LibraryResolver::parse_maven("org.lwjgl:lwjgl:3.3.1:natives-linux").unwrap();
        assert_eq!(group, "org.lwjgl");
        assert_eq!(artifact, "lwjgl");
        assert_eq!(version, "3.3.1");
        assert_eq!(classifier, Some("natives-linux".to_string()));
    }

    #[test]
    fn test_parse_maven_invalid() {
        assert!(LibraryResolver::parse_maven("invalid").is_err());
        assert!(LibraryResolver::parse_maven("only:two").is_err());
    }

    #[test]
    fn test_should_include_no_rules() {
        assert!(LibraryResolver::should_include(&[]));
    }

    #[test]
    fn test_should_include_allow_unconditional() {
        let rules = vec![Rule {
            action: "allow".to_string(),
            os: None,
            features: None,
        }];
        assert!(LibraryResolver::should_include(&rules));
    }

    #[test]
    fn test_should_include_disallow_unconditional() {
        let rules = vec![Rule {
            action: "disallow".to_string(),
            os: None,
            features: None,
        }];
        assert!(!LibraryResolver::should_include(&rules));
    }

    #[test]
    fn test_should_include_allow_then_disallow() {
        let rules = vec![
            Rule {
                action: "allow".to_string(),
                os: None,
                features: None,
            },
            Rule {
                action: "disallow".to_string(),
                os: None,
                features: None,
            },
        ];
        assert!(!LibraryResolver::should_include(&rules));
    }

    #[test]
    fn test_build_classpath() {
        let resolved = vec![
            ResolvedLibrary {
                path: PathBuf::from("/libs/a.jar"),
                is_native: false,
                extract_to: None,
            },
            ResolvedLibrary {
                path: PathBuf::from("/libs/b-native.so"),
                is_native: true,
                extract_to: None,
            },
            ResolvedLibrary {
                path: PathBuf::from("/libs/c.jar"),
                is_native: false,
                extract_to: None,
            },
        ];
        let client_jar = Path::new("/versions/1.21.4/client.jar");
        let cp = LibraryResolver::build_classpath(&resolved, client_jar);
        let expected = if cfg!(windows) {
            "/libs/a.jar;/libs/c.jar;/versions/1.21.4/client.jar"
        } else {
            "/libs/a.jar:/libs/c.jar:/versions/1.21.4/client.jar"
        };
        assert_eq!(cp, expected);
    }

    #[test]
    fn test_download_urls() {
        let resolver = LibraryResolver::new(Client::new(), PathBuf::from("/tmp/libs"));
        let urls = resolver.download_urls(
            "com.mojang:authlib:6.0.52",
            "com/mojang/authlib/6.0.52/authlib-6.0.52.jar",
            None,
        );
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("libraries.minecraft.net"));
        assert!(urls[1].contains("maven.apache.org"));
    }

    #[test]
    fn test_download_urls_custom() {
        let resolver = LibraryResolver::new(Client::new(), PathBuf::from("/tmp/libs"));
        let urls = resolver.download_urls(
            "com.example:lib:1.0",
            "com/example/lib/1.0/lib-1.0.jar",
            Some("https://maven.example.com/releases/"),
        );
        assert_eq!(urls.len(), 3);
        assert!(urls[0].contains("maven.example.com"));
    }
}
