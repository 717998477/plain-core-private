use crate::host::bridge::Host;
use crate::services::audio::host::Audio;
use plain_rs::media::audio::{playback, queue::AudioTrack};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Progress {
    path: String,
    revision: i64,
    position_ms: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Started {
    path: String,
    revision: i64,
}
pub(crate) async fn accept(
    audio: &Arc<Audio>,
    host: Arc<Host>,
    generation: u64,
    kind: &str,
    payload: &str,
) -> Result<bool, String> {
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
            let report: Started =
                serde_json::from_str(payload).map_err(|error| error.to_string())?;
            audio
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
                    playback::started(db, &track, report.revision)
                })
                .await
                .map_err(|error| error.message)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
