use super::*;
use crate::assembly::server::ContentServer;
use std::{sync::Arc, time::Duration};

fn server(dir: &tempfile::TempDir) -> ContentServer {
    ContentServer::start(
        dir.path(),
        &plain_rs::utils::base64::base64_encode(&[9; 32]),
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap()),
    )
    .unwrap()
}
#[tokio::test]
async fn native_events_enforce_direction_and_reconcile_lag() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let mut public = server.runtime_state().events.subscribe();
    for kind in [
        "MESSAGE_CREATED",
        "SMS_SEND_RESULT",
        "PAIRING_SUCCESS",
        "CONTENT_CHANGED",
        "CLIPBOARD_CHANGED",
        "UNKNOWN_EVENT",
        "IMAGE_INDEX_ERROR_REPORTED",
        plain_rs::events::NETWORK_CHANGED,
        plain_rs::events::AUDIO_LIBRARY_UPDATED,
        plain_rs::events::MEDIA_LIBRARY_UPDATED,
        plain_rs::events::HTTP_SERVER_FAILED,
        plain_rs::events::MEDIA_ACTION_FINISHED,
    ] {
        assert!(bridge.publish_text(kind, "{}".into()).await.is_err());
        assert!(bridge.publish_binary(kind, vec![]).is_err());
    }
    for kind in HOST_TEXT_TYPES {
        if matches!(
            *kind,
            "PAIRING_REQUEST_REPORTED"
                | "PAIRING_RESPONSE_REPORTED"
                | "PAIRING_CANCEL_REPORTED"
                | "NOTIFICATION_REPORTED"
                | "CLIPBOARD_REPORTED"
                | "AUDIO_PROGRESS_REPORTED"
                | "AUDIO_STARTED_REPORTED"
                | "AUDIO_COMPLETED_REPORTED"
                | "IMAGE_CATALOG_CHANGED"
                | "MEDIA_CATALOG_CHANGED"
                | "NETWORK_INTERFACES_CHANGED"
                | "BLE_ADVERTISEMENT_REPORTED"
                | "SMS_PART_SENT_REPORTED"
                | "BACKGROUND_EXECUTION_GRANTED"
                | "BACKGROUND_EXECUTION_REVOKED"
        ) {
            assert!(
                bridge.publish_text(kind, "{}".into()).await.is_err(),
                "{kind}"
            );
            assert!(public.try_recv().is_err(), "{kind}");
        } else {
            bridge.publish_text(kind, "{}".into()).await.unwrap();
            assert!(public.recv().await.unwrap().is_host_event());
        }
    }
    for kind in HOST_BINARY_TYPES {
        bridge.publish_binary(kind, vec![1, 2, 3]).unwrap();
        assert_eq!(
            public.recv().await.unwrap().binary_payload,
            Some(vec![1, 2, 3])
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(20), bridge.next_event())
            .await
            .is_err()
    );
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast("PAIRING_SUCCESS", "{}".into()))
        .unwrap();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PAIRING_SUCCESS"
    );
    for _ in 0..300 {
        server
            .runtime_state()
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    server.shutdown().await;
    assert!(bridge.next_event().await.is_none());
}
#[tokio::test]
async fn native_requests_reply_and_replacement_cancel_pending_calls() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let host = server.runtime_state().host.clone();
    let call_host = host.clone();
    let task = tokio::spawn(async move { call_host.call("fixture", json!({})).await });
    let request = bridge.next_request().await.unwrap();
    bridge
        .reply(json!({"id":request["id"],"result":42}))
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), json!(42));
    let task = tokio::spawn(async move { host.call("fixture", json!({})).await });
    bridge.next_request().await.unwrap();
    let replacement = server.host_events();
    assert!(task.await.unwrap().unwrap_err().contains("replaced"));
    assert!(
        bridge
            .publish_text(HOST_TEXT_TYPES[0], "{}".into())
            .await
            .is_err()
    );
    drop(bridge);
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn listener_failure_survives_event_queue_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let state = server.runtime_state();
    *state.http_failure.lock().unwrap() = Some(crate::assembly::lifecycle::HttpServerFailure {
        generation: 7,
        message: "failed".into(),
    });
    for _ in 0..300 {
        let _ = state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
    let event = bridge.next_event().await.unwrap();
    assert_eq!(event["type"], "HTTP_SERVER_FAILED");
    assert_eq!(event["reconcile"], true);
    let payload: Value = serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["generation"], 7);
    server.shutdown().await;
}

#[tokio::test]
async fn replacing_a_host_wakes_a_waiting_event_receiver() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let original = server.host_events();
    let mut waiting = Box::pin(original.next_event());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let replacement = server.host_events();
    std::future::poll_fn(|context| {
        assert!(matches!(
            std::future::Future::poll(waiting.as_mut(), context),
            std::task::Poll::Ready(None)
        ));
        std::task::Poll::Ready(())
    })
    .await;
    drop(waiting);
    drop(original);
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn replaced_hosts_discard_requests_and_events_already_in_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let original = server.host_events();
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
        .unwrap();
    let replacement = server.host_events();
    assert!(original.next_request().await.is_none());
    assert!(original.next_event().await.is_none());
    original.wait_disconnected().await;
    assert!(
        server
            .runtime_state()
            .host
            .is_current(replacement.generation)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn raw_ble_advertisements_are_consumed_without_public_rebroadcast() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events();
    let state = server.runtime_state();
    let mut public = state.events.subscribe();
    let scan = state.ble_scans.begin().unwrap();
    bridge
        .publish_text(
            "BLE_ADVERTISEMENT_REPORTED",
            serde_json::json!({"scanId":scan,"shortId":"invalid"}).to_string(),
        )
        .await
        .unwrap();
    assert!(public.try_recv().is_err());
    state.ble_scans.end(&scan);
    assert!(
        bridge
            .publish_text(
                "BLE_ADVERTISEMENT_REPORTED",
                serde_json::json!({"scanId":scan,"shortId":"0001020304050607"}).to_string()
            )
            .await
            .is_err()
    );
    bridge
        .publish_text("IMAGE_CATALOG_CHANGED", "null".into())
        .await
        .unwrap();
    assert!(public.try_recv().is_err());
    bridge
        .publish_text("NETWORK_INTERFACES_CHANGED", "null".into())
        .await
        .unwrap();
    let changed = public.try_recv().unwrap();
    assert_eq!(changed.event_type, plain_rs::events::NETWORK_CHANGED);
    assert!(!changed.is_host_event());
    assert!(public.try_recv().is_err());
    assert!(
        bridge
            .publish_text("NETWORK_INTERFACES_CHANGED", "{}".into())
            .await
            .is_err()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn client_request_errors_cannot_overwrite_shared_image_model_state() {
    let directory = tempfile::tempdir().unwrap();
    let server = server(&directory);
    let bridge = server.host_events();
    let state = server.runtime_state();
    let before = state.image_models.snapshot();
    assert!(
        bridge
            .publish_text(
                "IMAGE_INDEX_ERROR_REPORTED",
                json!("Client transport failed").to_string()
            )
            .await
            .is_err()
    );
    let after = state.image_models.snapshot();
    assert_eq!(before.status.error_message, after.status.error_message);
    assert_eq!(before.status.status, after.status.status);
    assert_eq!(before.version, after.version);
    server.shutdown().await;
}
