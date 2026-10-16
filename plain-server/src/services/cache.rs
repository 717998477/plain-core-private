use plain_rs::cache_paths::CachePaths;
use std::path::PathBuf;

fn cache(paths: &CachePaths) -> std::io::Result<Option<PathBuf>> {
    let root = paths.root().to_owned();
    match std::fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => Ok(Some(root)),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid application cache directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn size(paths: &CachePaths) -> std::io::Result<u64> {
    let mut pending: Vec<PathBuf> = cache(paths)?.into_iter().collect();
    let mut size = 0u64;
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                size = size.checked_add(metadata.len()).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Cache size overflow")
                })?;
            }
        }
    }
    Ok(size)
}

pub(crate) fn clear_files(paths: &CachePaths) -> std::io::Result<()> {
    let Some(root) = cache(paths)? else {
        return Ok(());
    };
    let media_caches = [paths.image_cache(), paths.video_cache()];
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if media_caches.contains(&path) && metadata.is_dir() {
            continue;
        }
        let result = if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match result {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            result => result?,
        }
    }
    Ok(())
}

pub(crate) async fn clear(
    paths: &CachePaths,
    host: &crate::host::bridge::Host,
) -> Result<(), String> {
    let root = paths.clone();
    tokio::task::spawn_blocking(move || cache(&root))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    crate::platform::media_cache::clear(paths, host).await?;
    let paths = paths.clone();
    tokio::task::spawn_blocking(move || clear_files(&paths))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/services/cache.rs"]
mod tests;
