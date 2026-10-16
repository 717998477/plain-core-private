use crate::host::resources::NativeResources;
use plain_rs::{db::DAppFile, files::app_files::FileStore};
use std::sync::Arc;
use tokio::sync::watch;

pub(crate) struct Import {
    pub source: String,
    pub name: String,
    pub mime: String,
    pub delete_source: bool,
}

pub(crate) async fn import(
    store: &Arc<FileStore>,
    resources: &NativeResources,
    cache: &std::path::Path,
    request: Import,
    stop: watch::Receiver<bool>,
) -> Result<DAppFile, String> {
    if *stop.borrow() {
        return Err("File import cancelled".into());
    }
    if !request.source.starts_with("content://") && !request.source.starts_with("ph://") {
        let source =
            super::references::resolve(store.db.clone(), store.directory.clone(), &request.source)
                .map_err(|error| error.to_string())?;
        return store
            .import(
                source.into(),
                request.name,
                request.mime,
                request.delete_source,
            )
            .await;
    }
    if request.delete_source {
        return Err("Deleting an imported platform resource requires its platform adapter".into());
    }
    let temporary = super::source_files::stage(
        &request.source,
        store.db.clone(),
        &store.directory,
        resources,
        cache,
        stop,
    )
    .await?;
    let store = store.clone();
    tokio::task::spawn_blocking(move || {
        store.import_file(temporary.path(), &request.name, &request.mime, false)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/imports.rs"]
mod tests;
