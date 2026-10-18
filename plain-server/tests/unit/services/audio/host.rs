use super::*;
use plain_rs::media::audio::{playback, queue};

#[tokio::test]
async fn native_host_lifetime_restores_position_and_invalidates_reports_once_per_generation() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let track = queue::AudioTrack::from_path_stem("/owned/track.mp3");
    state.db.with_conn(|conn| conn.execute(
        "INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES('owned-track',1,?1,'audio/mpeg','track.mp3','Track','','',1,0,'',0,0,0,'now','now')",
        [&track.path],
    )).unwrap();
    queue::enqueue(&state.db, std::slice::from_ref(&track), false).unwrap();
    playback::prepare_track(&state.db, &track).unwrap();
    let previous = playback::seek(&state.db, 1234).unwrap();
    let (first, requests) = state.host.connect();
    let event = state.audio.playback_event().await;
    assert_eq!(event.event_type, "AUDIO_PLAYBACK_UPDATED");
    let restored: playback::Playback = serde_json::from_str(&event.payload).unwrap();
    assert_eq!(restored.path, track.path);
    assert_eq!(restored.position_ms, 1234);
    assert!(restored.revision > previous.revision);
    assert!(!playback::report(&state.db, &track.path, previous.revision, 0).unwrap());
    assert!(requests.is_empty());
    let again: playback::Playback =
        serde_json::from_str(&state.audio.playback_event().await.payload).unwrap();
    assert_eq!(again, restored);
    let (second, requests) = state.host.connect();
    assert!(second > first);
    let replacement: playback::Playback =
        serde_json::from_str(&state.audio.playback_event().await.payload).unwrap();
    assert_eq!(replacement.position_ms, 1234);
    assert!(replacement.revision > restored.revision);
    assert!(!playback::report(&state.db, &track.path, restored.revision, 0).unwrap());
    assert!(requests.is_empty());
    let retired = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "mutation { invalidateAudioEngine { revision } }",
        ))
        .await;
    assert!(!retired.errors.is_empty());
    core.stop_core().await;
}
