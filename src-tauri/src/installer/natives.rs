use std::path::{Path, PathBuf};

use thiserror::Error;
use zip::ZipArchive;

#[derive(Error, Debug)]
pub enum NativeError {
    #[error("Failed to open ZIP archive: {0}")]
    ZipOpen(String),
    #[error("Failed to read ZIP entry: {0}")]
    EntryRead(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No native libraries found in archive")]
    NoNativesFound,
}

pub type Result<T> = std::result::Result<T, NativeError>;

const NATIVE_EXTENSIONS: &[&str] = &["dll", "so", "dylib", "jnilib"];

pub struct NativeExtractor;

impl NativeExtractor {
    pub fn extract_all(
        jar_path: &Path,
        natives_dir: &Path,
        exclude: &[String],
    ) -> Result<Vec<PathBuf>> {
        let file = std::fs::File::open(jar_path)
            .map_err(|e| NativeError::ZipOpen(e.to_string()))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| NativeError::ZipOpen(e.to_string()))?;
        let mut extracted = Vec::new();

        std::fs::create_dir_all(natives_dir)?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| NativeError::EntryRead(e.to_string()))?;
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
            if !NATIVE_EXTENSIONS.contains(&ext) {
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
                    let tmp = target.with_extension(&format!("{}.tmp", ext));
                    if std::fs::rename(&target, &tmp).is_ok() {
                        let _ = std::fs::remove_file(&tmp);
                    }
                }
            }

            let mut contents = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut contents)
                .map_err(|e| NativeError::EntryRead(e.to_string()))?;
            std::fs::write(&target, contents)?;

            extracted.push(target);
        }

        Ok(extracted)
    }

    pub fn verify_natives(natives_dir: &Path, expected_count: usize) -> bool {
        let count = std::fs::read_dir(natives_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map_or(false, |ext| NATIVE_EXTENSIONS.contains(&ext))
                    })
                    .count()
            })
            .unwrap_or(0);
        count >= expected_count
    }

    pub fn clean_natives(natives_dir: &Path) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(natives_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if NATIVE_EXTENSIONS.contains(&ext) {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    fn create_test_jar(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let jar_path = dir.join("test.jar");
        let file = File::create(&jar_path).unwrap();
        let mut zip = ZipWriter::new(file);
        for (name, content) in files {
            let options = FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(name, options).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
        jar_path
    }

    #[test]
    fn test_extract_all_finds_natives() {
        let dir = tempdir().unwrap();
        let natives_dir = dir.path().join("natives");
        let jar = create_test_jar(
            dir.path(),
            &[
                ("lib/test.so", b"fake so"),
                ("test.dll", b"fake dll"),
                ("META-INF/MANIFEST.MF", b"manifest"),
                ("readme.txt", b"not native"),
            ],
        );

        let extracted = NativeExtractor::extract_all(&jar, &natives_dir, &[]).unwrap();
        assert_eq!(extracted.len(), 2);
        assert!(natives_dir.join("test.so").exists());
        assert!(natives_dir.join("test.dll").exists());
        assert!(!natives_dir.join("MANIFEST.MF").exists());
    }

    #[test]
    fn test_extract_all_respects_exclude() {
        let dir = tempdir().unwrap();
        let natives_dir = dir.path().join("natives");
        let jar = create_test_jar(
            dir.path(),
            &[
                ("lib/test.so", b"fake so"),
                ("excluded/test.dll", b"fake dll"),
            ],
        );

        let extracted =
            NativeExtractor::extract_all(&jar, &natives_dir, &["excluded/".to_string()]).unwrap();
        assert_eq!(extracted.len(), 1);
        assert!(natives_dir.join("test.so").exists());
        assert!(!natives_dir.join("test.dll").exists());
    }

    #[test]
    fn test_verify_natives() {
        let dir = tempdir().unwrap();
        let natives_dir = dir.path().join("natives");
        std::fs::create_dir_all(&natives_dir).unwrap();
        std::fs::write(natives_dir.join("test.so"), b"fake").unwrap();
        std::fs::write(natives_dir.join("test.dll"), b"fake").unwrap();

        assert!(NativeExtractor::verify_natives(&natives_dir, 2));
        assert!(NativeExtractor::verify_natives(&natives_dir, 1));
        assert!(!NativeExtractor::verify_natives(&natives_dir, 3));
    }

    #[test]
    fn test_clean_natives() {
        let dir = tempdir().unwrap();
        let natives_dir = dir.path().join("natives");
        std::fs::create_dir_all(&natives_dir).unwrap();
        std::fs::write(natives_dir.join("test.so"), b"fake").unwrap();
        std::fs::write(natives_dir.join("test.dll"), b"fake").unwrap();
        std::fs::write(natives_dir.join("keep.txt"), b"keep").unwrap();

        NativeExtractor::clean_natives(&natives_dir).unwrap();
        assert!(!natives_dir.join("test.so").exists());
        assert!(!natives_dir.join("test.dll").exists());
        assert!(natives_dir.join("keep.txt").exists());
    }
}
