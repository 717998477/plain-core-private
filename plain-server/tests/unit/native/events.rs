use super::*;
use crate::assembly::server::ContentServer;
use crate::native::graphql::NativeRequestScope;
use std::future::Future;

const KEY: [u8; 32] = [7; 32];

fn fixture() -> (
    tempfile::TempDir,
    ContentServer,
    NativeEventStream,
    NativeRequestScope,
) {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    core.runtime_state()
        .prefs
        .set(
            plain_rs::prefs::pref_keys::URL_TOKEN,
            plain_rs::utils::base64::base64_encode(&KEY),
        )
        .unwrap();
    let scope = NativeRequestScope::default();
    let stream = core
        .native_graphql()
        .unwrap()
        .subscribe_events(scope.clone())
        .unwrap();
    (directory, core, stream, scope)
}

async fn snapshots(stream: &mut NativeEventStream) {
    for kind in [
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
        "DLNA_SENDER_UPDATED",
        "APP_UPDATE_DOWNLOAD_UPDATED",
        "AUDIO_PLAYBACK_UPDATED",
        "PEER_NOTIFICATIONS_UPDATED",
    ] {
        assert_eq!(stream.recv().await.unwrap().event_type, kind);
    }
}

#[tokio::test]
async fn trusted_native_management_events_are_delivered_and_stay_private_on_network_sockets() {
    let (_directory, core, mut stream, _scope) = fixture();
    snapshots(&mut stream).await;
    let event = WsEvent::broadcast(
        plain_rs::events::PEER_CLIPBOARD_CHANGED,
        "{\"peerId\":\"fixture\"}".into(),
    );
    assert!(event.clone().into_public().is_none());
    core.runtime_state().events.send(event.clone()).unwrap();
    let frame = stream.next_frame().await.unwrap().unwrap();
    assert_eq!(
        plain_rs::protocols::websocket::decode(&frame, &KEY),
        Some((event.event_type.into(), event.payload.into_bytes()))
    );
    core.stop_core().await;
}

#[tokio::test]
async fn platform_reports_and_other_clients_targets_do_not_reach_the_native_view() {
    let (_directory, core, mut stream, _scope) = fixture();
    snapshots(&mut stream).await;
    let state = core.runtime_state();
    state
        .events
        .send(WsEvent::host_text("CLIPBOARD_REPORTED", "{}".into()).unwrap())
        .unwrap();
    state
        .events
        .send(WsEvent::targeted(
            "CHAT_MESSAGE",
            "other".into(),
            "other-client",
        ))
        .unwrap();
    state
        .events
        .send(WsEvent::targeted(
            "CHAT_MESSAGE",
            "own".into(),
            &state.native_client_id().unwrap(),
        ))
        .unwrap();
    assert_eq!(stream.recv().await.unwrap().payload, "own");
    core.stop_core().await;
}

#[tokio::test]
async fn text_frames_keep_existing_encryption_and_binary_frames_keep_exact_bytes() {
    let (_directory, core, mut stream, _scope) = fixture();
    snapshots(&mut stream).await;
    let state = core.runtime_state();
    state
        .events
        .send(WsEvent::broadcast("CHAT_MESSAGE", "fixture text".into()))
        .unwrap();
    let text = stream.next_frame().await.unwrap().unwrap();
    assert_eq!(
        plain_rs::protocols::websocket::decode(&text, &KEY),
        Some(("CHAT_MESSAGE".into(), b"fixture text".to_vec()))
    );
    assert_ne!(
        text,
        plain_rs::protocols::websocket::encode_raw("CHAT_MESSAGE", b"fixture text").unwrap()
    );
    let bytes = vec![0, 159, 146, 150];
    state
        .events
        .send(WsEvent::broadcast_binary(
            "SCREEN_MIRROR_VIDEO",
            bytes.clone(),
        ))
        .unwrap();
    assert_eq!(
        stream.next_frame().await.unwrap(),
        plain_rs::protocols::websocket::encode_raw("SCREEN_MIRROR_VIDEO", &bytes)
    );
    core.stop_core().await;
}

#[tokio::test]
async fn closing_a_view_ends_its_wait_without_waiting_for_another_event() {
    let (_directory, core, mut stream, scope) = fixture();
    snapshots(&mut stream).await;
    let mut waiting = Box::pin(stream.next_frame());
    std::future::poll_fn(|context| {
        assert!(waiting.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    scope.close();
    assert!(waiting.await.unwrap().is_none());
    core.stop_core().await;
}

#[tokio::test]
async fn closed_views_and_stopped_cores_take_precedence_over_initial_and_queued_events() {
    let (_directory, core, mut stream, scope) = fixture();
    scope.close();
    assert!(stream.next_frame().await.unwrap().is_none());
    let mut stream = core
        .native_graphql()
        .unwrap()
        .subscribe_events(Default::default())
        .unwrap();
    snapshots(&mut stream).await;
    core.runtime_state()
        .events
        .send(WsEvent::broadcast("CHAT_MESSAGE", "queued".into()))
        .unwrap();
    core.stop_core().await;
    assert!(stream.next_frame().await.unwrap().is_none());
}

#[tokio::test]
async fn lagged_native_views_recover_current_snapshots_before_retained_events() {
    let (_directory, core, mut stream, _scope) = fixture();
    snapshots(&mut stream).await;
    let state = core.runtime_state();
    for _ in 0..300 {
        state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    let recovery = stream.next_delivery().await.unwrap();
    assert!(recovery.reconcile);
    assert_eq!(recovery.event.event_type, "CONTENT_CHANGED");
    let current = stream.recv().await.unwrap();
    assert_eq!(current.event_type, "IMAGE_MODELS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(&current.payload).unwrap(),
        serde_json::to_value(state.image_models.snapshot()).unwrap()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn invalid_stored_event_keys_refuse_subscription() {
    let (_directory, core, _stream, _scope) = fixture();
    let state = core.runtime_state();
    state
        .prefs
        .set(
            plain_rs::prefs::pref_keys::URL_TOKEN,
            plain_rs::utils::base64::base64_encode(&KEY[..31]),
        )
        .unwrap();
    assert!(
        core.native_graphql()
            .unwrap()
            .subscribe_events(Default::default())
            .is_err()
    );
    state
        .prefs
        .set(
            plain_rs::prefs::pref_keys::URL_TOKEN,
            plain_rs::utils::base64::base64_encode(&KEY),
        )
        .unwrap();
    let mut stream = core
        .native_graphql()
        .unwrap()
        .subscribe_events(Default::default())
        .unwrap();
    let first = stream.next_frame().await.unwrap().unwrap();
    assert_eq!(
        plain_rs::protocols::websocket::decode(&first, &KEY)
            .unwrap()
            .0,
        "IMAGE_MODELS_UPDATED"
    );
    core.stop_core().await;
}
