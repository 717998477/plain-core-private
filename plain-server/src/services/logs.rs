use crate::host::bridge::Host;
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

pub(crate) struct Logs {
    host: Arc<Host>,
    path: RwLock<Option<PathBuf>>,
}
impl Logs {
    pub(crate) fn new(host: Arc<Host>) -> Self {
        Self {
            host,
            path: RwLock::new(None),
        }
    }
    pub(crate) fn set_path(&self, path: PathBuf) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("Absolute log file path required".into());
        }
        *self.path.write().map_err(|error| error.to_string())? = Some(path);
        Ok(())
    }
    pub(crate) async fn path(&self) -> Result<PathBuf, String> {
        if let Some(path) = self.path.read().map_err(|error| error.to_string())?.clone() {
            return Ok(path);
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let value = self
                .host
                .call("systemLogPath", serde_json::json!({}))
                .await?;
            let path = PathBuf::from(value.as_str().ok_or("Invalid log path")?);
            if !path.is_absolute() {
                return Err("Absolute log file path required".into());
            }
            return Ok(path);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = &self.host;
            Err("Log file path is not configured".into())
        }
    }
    pub(crate) async fn read(
        &self,
        query: String,
        offset: i32,
        limit: i32,
    ) -> Result<Vec<String>, String> {
        if limit <= 0 {
            return Ok(Vec::new());
        }
        let path = self.path().await?;
        tokio::task::spawn_blocking(move || {
            read_lines_newest_first(
                &path,
                Some(query.trim()),
                offset.max(0) as usize,
                limit as usize,
            )
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    }
    pub(crate) async fn size(&self) -> Result<u64, String> {
        let path = self.path().await?;
        tokio::task::spawn_blocking(move || {
            log_files(&path)?.into_iter().try_fold(0u64, |size, file| {
                size.checked_add(std::fs::metadata(file)?.len())
                    .ok_or_else(|| io::Error::other("Log size overflow"))
            })
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    }
    pub(crate) async fn clear_files(&self) -> Result<(), String> {
        let path = self.path().await?;
        tokio::task::spawn_blocking(move || {
            for file in log_files(&path)? {
                let result = if file == path {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(&file)
                        .and_then(|file| file.set_len(0))
                } else {
                    std::fs::remove_file(&file)
                };
                if let Err(error) = result {
                    if error.kind() != io::ErrorKind::NotFound {
                        return Err(error);
                    }
                }
            }
            Ok::<_, io::Error>(())
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    }
    pub(crate) async fn export(
        &self,
        directory: &Path,
        cache: &plain_rs::cache_paths::CachePaths,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<Option<String>, String> {
        let path = self.path().await?;
        let paths = plain_rs::data_paths::DataPaths::new(directory);
        let reports = vec![paths.crash_report(), paths.crash_log()];
        let target = cache.root().join("logs.zip");
        let files = tokio::task::spawn_blocking(move || {
            let mut files = log_files(&path)?
                .into_iter()
                .map(|path| {
                    let name = format!(
                        "logs/{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    (path, name)
                })
                .collect::<Vec<_>>();
            for path in reports {
                match std::fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_file() => {
                        let name = path
                            .file_name()
                            .ok_or_else(|| io::Error::other("Missing crash log filename"))?
                            .to_string_lossy()
                            .into_owned();
                        files.push((path, name));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
            Ok::<_, io::Error>(files)
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
        if files.is_empty() {
            return Ok(None);
        }
        super::files::archive_operations::pack_named_files(files, target, stop)
            .await
            .map(Some)
    }
    pub(crate) async fn clear(&self) -> Result<(), String> {
        let path = self.path().await?;
        tokio::task::spawn_blocking(move || {
            match std::fs::OpenOptions::new().write(true).open(path) {
                Ok(file) => file.set_len(0),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        })
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
    }
}

fn log_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing log directory"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("Missing log filename"))?
        .to_string_lossy();
    let prefix = format!("{name}.");
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let entry_name = entry.file_name();
        let entry_name = entry_name.to_string_lossy();
        if (entry_name == name || entry_name.starts_with(&prefix)) && entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

pub fn read_lines_newest_first(
    path: &Path,
    needle: Option<&str>,
    offset: usize,
    limit: usize,
) -> io::Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let len = file.metadata()?.len();
    read(&mut file, len, needle, offset, limit)
}

fn read(
    file: &mut (impl Read + Seek),
    mut remaining: u64,
    needle: Option<&str>,
    offset: usize,
    limit: usize,
) -> io::Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let needle = needle
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);
    let mut result = Vec::new();
    let mut skipped = 0;
    let mut tail = Vec::new();
    let mut accept = |bytes: &[u8]| {
        let line = String::from_utf8_lossy(bytes)
            .trim_end_matches('\r')
            .to_owned();
        if line.is_empty()
            || needle
                .as_ref()
                .is_some_and(|value| !line.to_lowercase().contains(value))
        {
            return false;
        }
        if skipped < offset {
            skipped += 1;
        } else {
            result.push(line);
        }
        result.len() >= limit
    };
    while remaining > 0 {
        let size = remaining.min(64 * 1024) as usize;
        remaining -= size as u64;
        file.seek(SeekFrom::Start(remaining))?;
        let mut bytes = vec![0; size + tail.len()];
        file.read_exact(&mut bytes[..size])?;
        bytes[size..].copy_from_slice(&tail);
        let mut end = bytes.len();
        for index in (0..bytes.len()).rev() {
            if bytes[index] == b'\n' {
                if accept(&bytes[index + 1..end]) {
                    return Ok(result);
                }
                end = index;
            }
        }
        tail = bytes[..end].to_vec();
    }
    if !tail.is_empty() {
        accept(&tail);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/unit/services/logs.rs"]
mod tests;
