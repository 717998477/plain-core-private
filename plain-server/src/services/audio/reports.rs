use crate::assembly::server::ServerState;
use plain_rs::media::audio::{playback, queue::AudioTrack};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Progress {
    path: String,
    revision: i64,
    position_ms: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Transition {
    path: String,
    revision: i64,
}
pub(crate) async fn accept(
    state: &ServerState,
    generation: u64,
    kind: &str,
    payload: &str,
) -> Result<bool, String> {
    let host = state.host.clone();
    let audio = &state.audio;
    match kind {
        "AUDIO_PROGRESS_REPORTED" => {
            let report: Progress =
                serde_json::from_str(payload).map_err(|error| error.to_string())?;
            audio
                .run_playback(move |db, _| {
                    if !host.is_current(generation) {
                        return Err(plain_rs::error::DomainError::Other(
                            "host disconnected".into(),
                        ));
                    }
                    playback::report(db, &report.path, report.revision, report.position_ms)
                })
                .await
                .map_err(|error| error.message)?;
        }
        "AUDIO_STARTED_REPORTED" => {
            let report: Transition =
                serde_json::from_str(payload).map_err(|error| error.to_string())?;
            let accepted = audio
                .run_playback(move |db, _| {
                    if !host.is_current(generation) {
                        return Err(plain_rs::error::DomainError::Other(
                            "host disconnected".into(),
                        ));
                    }
                    let track = db
                        .with_conn(|connection| {
                            plain_rs::media::audio::queue::track_by_path_conn(
                                connection,
                                &report.path,
                            )
                        })?
                        .unwrap_or_else(|| AudioTrack::from_path_stem(&report.path));
                    let indexed = track.media_id.is_some();
                    Ok(playback::started(db, &track, report.revision)? && indexed)
                })
                .await
                .map_err(|error| error.message)?;
            if accepted {
                publish_library_updated(&state.events);
            }
        }
        "AUDIO_COMPLETED_REPORTED" => {
            let report: Transition =
                serde_json::from_str(payload).map_err(|error| error.to_string())?;
            if report.path.is_empty() || report.revision < 0 {
                return Err("invalid audio completion".into());
            }
            let prefs = state.prefs.clone();
            let accepted = audio
                .run_playback(move |db, engine| {
                    if !host.is_current(generation) {
                        return Err(plain_rs::error::DomainError::Other(
                            "host disconnected".into(),
                        ));
                    }
                    let current = playback::snapshot(db)?;
                    if current.path != report.path || current.revision != report.revision {
                        return Ok(false);
                    }
                    engine.refresh()?;
                    plain_rs::media::audio::commands::completed(
                        db,
                        &prefs,
                        engine,
                        &report.path,
                        report.revision,
                    )
                })
                .await
                .map_err(|error| error.message)?;
            if accepted {
                let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
                    "CONTENT_CHANGED",
                    "{}".into(),
                ));
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(crate) fn publish_library_updated(
    events: &tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
) {
    let _ = events.send(plain_rs::events::WsEvent::broadcast(
        plain_rs::events::AUDIO_LIBRARY_UPDATED,
        "{}".into(),
    ));
}

#[cfg(test)]
#[path = "../../../tests/unit/services/audio/reports.rs"]
mod tests;
