use crate::host::bridge::Host;
use crate::services::audio::host::Audio;
use plain_rs::media::audio::{playback, queue::AudioTrack};
use plain_rs::media::duration_cache::{Duration, DurationCache};
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DurationReport {
    media_type: String,
    media_id: String,
    duration_ms: i64,
    path: String,
    size_bytes: i64,
    modified_at: String,
}

pub(crate) async fn accept(
    audio: &Arc<Audio>,
    durations: &DurationCache,
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
        "MEDIA_DURATION_REPORTED" => {
            let report: DurationReport =
                serde_json::from_str(payload).map_err(|error| error.to_string())?;
            durations
                .save(Duration {
                    media_type: report.media_type,
                    media_id: report.media_id,
                    duration_ms: report.duration_ms,
                    path: report.path,
                    size_bytes: report.size_bytes,
                    modified_at: report.modified_at,
                    updated_at: String::new(),
                })
                .map_err(|error| error.to_string())?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
