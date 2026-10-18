use anyhow::{Context, Result, ensure};
use plain_rs::cache_paths::CachePaths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

const MAX_ENTRIES: usize = 32768;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
static PREPARATION: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    #[serde(default)]
    pub directory: String,
    #[serde(default)]
    pub archives: Vec<String>,
    #[serde(default)]
    pub prefix: String,
}

pub fn prepare(cache: &CachePaths, source: &Source) -> Result<Option<PathBuf>> {
    prepare_with(cache, source, |_| {})
}

fn prepare_with(
    cache: &CachePaths,
    source: &Source,
    mut opened: impl FnMut(&Path),
) -> Result<Option<PathBuf>> {
    let _guard = PREPARATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Web asset preparation lock poisoned"))?;
    if !source.directory.is_empty() {
        ensure!(
            source.archives.is_empty() && source.prefix.is_empty(),
            "Conflicting web asset sources"
        );
        let directory = Path::new(&source.directory);
        ensure!(
            directory.is_absolute(),
            "Absolute web asset directory required"
        );
        return if directory.join("index.html").is_file() {
            Ok(Some(directory.canonicalize()?))
        } else {
            Ok(None)
        };
    }
    if source.archives.is_empty() {
        ensure!(source.prefix.is_empty(), "Web asset archive required");
        return Ok(None);
    }
    ensure!(
        source.prefix.ends_with('/'),
        "Web asset prefix must end in a slash"
    );
    relative(source.prefix.trim_end_matches('/'))?;
    ensure!(
        cache.root().is_absolute(),
        "Absolute cache directory required"
    );
    let mut hash = Sha256::new();
    hash.update(b"web-assets-v1");
    hash.update((source.prefix.len() as u64).to_le_bytes());
    hash.update(source.prefix.as_bytes());
    let mut archives = Vec::new();
    for source in &source.archives {
        let path = Path::new(source);
        ensure!(path.is_absolute(), "Absolute web asset archive required");
        let path = path.canonicalize()?;
        let metadata = path.metadata()?;
        ensure!(metadata.is_file(), "Web asset archive must be a file");
        let name = path.to_string_lossy();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(metadata.len().to_le_bytes());
        hash.update(
            metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
                .to_le_bytes(),
        );
        archives.push(path);
    }
    let key = format!("{:x}", hash.finalize());
    let parent = cache.web_assets();
    directory(&parent)?;
    let target = parent.join(&key);
    if fs::symlink_metadata(&target).is_ok() {
        ensure!(
            fs::symlink_metadata(&target)?.file_type().is_dir(),
            "Invalid web asset cache directory"
        );
        if regular(&target.join("index.html"))
            && regular(&target.join(".complete"))
            && fs::metadata(target.join(".complete"))?.len() == key.len() as u64
            && fs::read(target.join(".complete"))? == key.as_bytes()
        {
            return Ok(Some(target));
        }
        fs::remove_dir_all(&target)?;
    }
    let staging = tempfile::Builder::new()
        .prefix(".extract-")
        .tempdir_in(&parent)?;
    let mut entries = 0usize;
    let mut bytes = 0u64;
    for path in archives {
        opened(&path);
        let mut zip = zip::ZipArchive::new(fs::File::open(&path)?)
            .with_context(|| format!("Invalid web asset archive: {}", path.display()))?;
        let mut names = HashSet::new();
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let Some(name) = entry.name().strip_prefix(&source.prefix) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let destination = relative(name.trim_end_matches('/'))?;
            ensure!(
                destination != Path::new(".complete"),
                "Reserved web asset name"
            );
            ensure!(
                entry
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 != 0o120000),
                "Web asset symbolic links are unsupported"
            );
            entries += 1;
            ensure!(entries <= MAX_ENTRIES, "Too many web asset entries");
            if entry.is_dir() {
                fs::create_dir_all(staging.path().join(destination))?;
                continue;
            }
            ensure!(
                names.insert(destination.clone()),
                "Duplicate web asset entry"
            );
            bytes = bytes
                .checked_add(entry.size())
                .filter(|size| *size <= MAX_BYTES)
                .ok_or_else(|| anyhow::anyhow!("Web assets exceed size limit"))?;
            let output = staging.path().join(destination);
            fs::create_dir_all(output.parent().unwrap())?;
            let size = entry.size();
            let written = std::io::copy(
                &mut (&mut entry).take(size + 1),
                &mut fs::File::create(output)?,
            )?;
            ensure!(written == size, "Invalid web asset entry size");
        }
    }
    if !regular(&staging.path().join("index.html")) {
        return Ok(None);
    }
    fs::write(staging.path().join(".complete"), key.as_bytes())?;
    fs::rename(staging.path(), &target)?;
    Ok(Some(target))
}

fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn directory(path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        ensure!(
            metadata.file_type().is_dir(),
            "Invalid web asset cache directory"
        );
    } else {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn relative(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    ensure!(
        !name.is_empty()
            && !name.contains(['\\', '\0'])
            && !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "Invalid web asset path"
    );
    Ok(path.to_owned())
}

pub(crate) async fn configure(
    state: &crate::assembly::server::ServerState,
    source: Source,
) -> Result<(), String> {
    let state = state.clone();
    tokio::spawn(async move {
        let _changes = state.web_asset_changes.lock().await;
        configure_locked(&state, source).await
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn configure_locked(
    state: &crate::assembly::server::ServerState,
    source: Source,
) -> Result<(), String> {
    let cache = state.cache.clone();
    let preparation_source = source.clone();
    let root = tokio::task::spawn_blocking(move || prepare(&cache, &preparation_source))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    *state.web_root.write().map_err(|error| error.to_string())? = root;
    *state
        .web_source
        .write()
        .map_err(|error| error.to_string())? = Some(source);
    Ok(())
}
pub(crate) async fn clear_cache(
    state: &crate::assembly::server::ServerState,
) -> Result<(), String> {
    let _changes = state.web_asset_changes.lock().await;
    let root = state
        .web_root
        .read()
        .map_err(|error| error.to_string())?
        .clone();
    crate::services::cache::clear(&state.cache, &state.host, root.clone()).await?;
    let active = tokio::task::spawn_blocking(move || {
        root.is_some_and(|root| regular(&root.join("index.html")))
    })
    .await
    .map_err(|error| error.to_string())?;
    if !active {
        let source = state
            .web_source
            .read()
            .map_err(|error| error.to_string())?
            .clone();
        if let Some(source) = source {
            configure_locked(state, source).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/services/web_assets.rs"]
mod tests;
