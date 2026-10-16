use crate::services::files::archives::{ArchiveReader, Archives};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;

#[derive(serde::Deserialize)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone)]
pub struct NativeArchives {
    archives: Arc<Archives>,
    platform: Arc<crate::services::files::platform::FilePlatform>,
    stop: watch::Receiver<bool>,
}

impl NativeArchives {
    pub(crate) fn new(
        archives: Arc<Archives>,
        platform: Arc<crate::services::files::platform::FilePlatform>,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            archives,
            platform,
            stop,
        }
    }

    pub async fn files(&self, entries: Vec<ArchiveEntry>) -> Result<ArchiveReader, String> {
        if *self.stop.borrow() || self.stop.has_changed().is_err() {
            return Err("Rust core stopped".into());
        }
        let permit = self.archives.acquire().ok_or("archive busy")?;
        let mut items = Vec::with_capacity(entries.len());
        for entry in entries {
            if !entry.path.is_absolute()
                || entry.name.is_empty()
                || entry.name.contains(['\\', '\0'])
                || Path::new(&entry.name)
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err("invalid archive entry".into());
            }
            items.push((entry.path, entry.name));
        }
        let mut paths = items
            .iter()
            .map(|(path, _)| {
                path.to_str()
                    .map(str::to_owned)
                    .ok_or("invalid file path encoding")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = std::collections::HashSet::new();
        paths.retain(|path| seen.insert(path.clone()));
        for paths in paths.chunks(128) {
            self.platform
                .authorize_paths(paths, false)
                .await
                .map_err(|error| error.to_string())?;
        }
        self.archives.build(items, permit, self.stop.clone()).await
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/archives.rs"]
mod tests;
