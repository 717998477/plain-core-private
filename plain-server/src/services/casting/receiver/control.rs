use plain_rs::prefs::pref_keys::{DLNA_ALLOWED_SENDERS, DLNA_DENIED_SENDERS};
use super::renderer_state::DlnaRendererState;
use crate::assembly::server::ServerState;
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) const EVENT_UPDATED: &str = "DLNA_RENDERER_UPDATED";

pub(crate) fn snapshot(state: &DlnaRendererState) -> Value {
    json!({
        "version": state.version,
        "isRunning": state.is_running,
        "isRetrying": state.is_retrying,
        "mediaUri": state.media_uri,
        "mediaTitle": state.media_title,
        "mediaAlbumArtUri": state.media_album_art_uri,
        "mediaType": state.media_type,
        "playbackState": state.playback_state,
        "port": state.port,
        "currentPositionMs": state.current_position_ms,
        "durationMs": state.duration_ms,
        "seekTargetMs": state.seek_target_ms,
        "pendingCastRequest": state.pending_cast_request,
        "startError": state.start_error,
    })
}

pub(crate) fn emit(state: &ServerState, renderer: &DlnaRendererState) {
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        EVENT_UPDATED,
        json!({"renderer": snapshot(renderer)}).to_string(),
    ));
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Retry {},
    Snapshot {},
    Rules {},
    RemoveRule { ip: String, allowed: bool },
    Accept { remember: bool },
    Reject { remember: bool },
}

pub(crate) async fn execute(state: &ServerState, request: Request) -> Result<Value, String> {
    let mut stopped = state.stop.clone();
    if *stopped.borrow_and_update() {
        return Err("Core stopped".into());
    }
    let _guard = state.cast.receiver_operations.lock().await;
    let engine = &state.dlna;
    match request {
        Request::Rules {} => Ok(rules(state)),
        Request::RemoveRule { ip, allowed } => {
            plain_rs::prefs::dlna::remove_sender(
                &state.prefs,
                if allowed {
                    DLNA_ALLOWED_SENDERS
                } else {
                    DLNA_DENIED_SENDERS
                },
                &ip,
            );
            Ok(rules(state))
        }
        Request::Retry {} => {
            super::lifecycle::retry_locked(state).await?;
            Ok(snapshot(&engine.snapshot().await))
        }
        Request::Snapshot {} => Ok(snapshot(&engine.snapshot().await)),
        Request::Accept { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.accept_cast(remember, &state.prefs).await;
            let after = engine.snapshot().await;
            if before.is_some() {
                emit(state, &after);
            }
            Ok(snapshot(&after))
        }
        Request::Reject { remember } => {
            let before = engine.snapshot().await.pending_cast_request;
            engine.reject_cast(remember, &state.prefs).await;
            let after = engine.snapshot().await;
            if before.is_some() {
                emit(state, &after);
            }
            Ok(snapshot(&after))
        }
    }
}

pub(crate) fn follow_changes(state: &ServerState) {
    let mut changes = state.dlna.changes();
    let state = state.clone();
    tokio::spawn(async move {
        let mut stop = state.stop.clone();
        loop {
            if *stop.borrow() {
                break;
            }
            tokio::select! {
                _ = stop.changed() => break,
                result = changes.recv() => {
                    if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                    let renderer = state.dlna.snapshot().await;
                    emit(&state, &renderer);
                }
            }
        }
    });
}

fn rules(state: &ServerState) -> Value {
    let entries = |key| {
        plain_rs::prefs::dlna::senders(&state.prefs, key)
            .into_iter()
            .map(|entry| {
                let (ip, name) = plain_rs::prefs::dlna::decode_sender_entry(&entry);
                json!({"ip":ip,"name":name})
            })
            .collect::<Vec<_>>()
    };
    json!({"allowed":entries(DLNA_ALLOWED_SENDERS),"denied":entries(DLNA_DENIED_SENDERS)})
}
