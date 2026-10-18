use crate::assembly::server::ServerState;
use anyhow::{Result, ensure};
use plain_rs::{db::DChat, files::shares::Service};
use std::sync::Arc;

struct PendingShare {
    db: Arc<plain_rs::db::Db>,
    id: Option<String>,
}
impl Drop for PendingShare {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            if let Err(error) = self.db.share_delete(&id) {
                log::error!("Unable to roll back unbound folder share: {error}");
            }
        }
    }
}

pub(crate) async fn existing(
    state: &ServerState,
    service: Arc<Service>,
    targets: Vec<String>,
    id: String,
) -> Result<Vec<DChat>> {
    super::chat_actions::validate_targets(&targets)?;
    if targets.is_empty() {
        return Ok(vec![]);
    }
    let content = super::share_card::content(state, service, id).await?;
    super::chat_actions::send_many(state, targets, content).await
}

pub(crate) async fn folder(
    state: &ServerState,
    service: Arc<Service>,
    targets: Vec<String>,
    path: String,
    name: String,
    expires_at: Option<String>,
) -> Result<Vec<DChat>> {
    super::chat_actions::validate_targets(&targets)?;
    if targets.is_empty() {
        return Ok(vec![]);
    }
    if let Some(expires) = expires_at.as_deref() {
        ensure!(
            chrono::DateTime::parse_from_rfc3339(expires)? > chrono::Utc::now(),
            "Share has expired"
        );
    }
    ensure!(!path.trim().is_empty(), "Share folder path required");
    let path = state.file_platform.resolve_reference(&path)?;
    state
        .file_platform
        .authorize_metadata_paths(std::slice::from_ref(&path))
        .await?;
    let create = service.clone();
    let db = state.db.clone();
    let (share, mut pending) = tokio::task::spawn_blocking(move || -> Result<_> {
        ensure!(std::fs::metadata(&path)?.is_dir(), "Share folder required");
        let share = create.create(name, vec![path], true, expires_at)?;
        let pending = PendingShare {
            db,
            id: Some(share.id.clone()),
        };
        Ok((share, pending))
    })
    .await??;
    let content = super::share_card::content(state, service, share.id).await?;
    let rows = super::chat_actions::prepare_send_many(state, &targets, &content)?;
    pending.id = None;
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        "CONTENT_CHANGED",
        "{}".into(),
    ));
    super::chat_actions::deliver_many(state, rows).await
}

#[cfg(test)]
#[path = "../../../tests/unit/services/chat/share_send.rs"]
mod tests;
