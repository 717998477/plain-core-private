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
