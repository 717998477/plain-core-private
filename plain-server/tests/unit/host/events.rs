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
async fn cancelled_audio_snapshot_read_is_retried_before_consuming_queued_events() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let bridge = core.host_events().unwrap();
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
        "DLNA_SENDER_UPDATED",
        "APP_UPDATE_DOWNLOAD_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    let (locked, ready) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let audio = state.audio.clone();
    let holder = tokio::spawn(async move {
        audio
            .run_playback(move |_, _| {
                locked.send(()).unwrap();
                let _ = released.blocking_recv();
                Ok(())
            })
            .await
            .unwrap();
    });
    ready.await.unwrap();
    {
        let mut pending = Box::pin(bridge.next_event());
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(pending.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
    }
    release.send(()).unwrap();
    holder.await.unwrap();
    state
        .events
        .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
        .unwrap();
    let recovered = tokio::time::timeout(Duration::from_secs(3), bridge.next_event())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered["type"], "AUDIO_PLAYBACK_UPDATED");
    let playback: Value = serde_json::from_str(recovered["payload"].as_str().unwrap()).unwrap();
    assert!(playback.get("error").is_none(), "{playback}");
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    let queued = tokio::time::timeout(Duration::from_secs(3), bridge.next_event())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(queued["type"], "CONTENT_CHANGED");
    core.stop_core().await;
}

#[tokio::test]
async fn model_snapshots_recover_current_versions_on_attach_overflow_and_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let server = server(&directory);
    let state = server.runtime_state();
    let bridge = server.host_events().unwrap();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    let initial = bridge.next_event().await.unwrap();
    assert_eq!(initial["type"], "IMAGE_MODELS_UPDATED");
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_TRANSPORT_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "DLNA_SENDER_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    let before: Value = serde_json::from_str(initial["payload"].as_str().unwrap()).unwrap();
    assert_eq!(
        before,
        serde_json::to_value(state.image_models.snapshot()).unwrap()
    );
    state.image_index.update_cache(|db| {
        db.with_conn(|connection| connection.execute(
            "INSERT INTO image_embeddings(id,path,embedding,created_at,updated_at) VALUES('owned-image','/owned/image.jpg',X'0000803F','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')", [],
        ))?;
        Ok(())
    }).unwrap();
    let expected = serde_json::to_value(state.image_models.snapshot()).unwrap();
    assert!(expected["version"].as_u64().unwrap() > before["version"].as_u64().unwrap());
    assert_eq!(expected["indexedImages"], 1);
    for _ in 0..300 {
        state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    let recovery = bridge.next_event().await.unwrap();
    assert_eq!(recovery["reconcile"], true);
    assert_eq!(recovery["type"], "CONTENT_CHANGED");
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    let snapshot = bridge.next_event().await.unwrap();
    assert_eq!(snapshot["type"], "IMAGE_MODELS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(snapshot["payload"].as_str().unwrap()).unwrap(),
        expected
    );
    let replacement = server.host_events().unwrap();
    assert!(bridge.next_event().await.is_none());
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    let snapshot = replacement.next_event().await.unwrap();
    assert_eq!(snapshot["type"], "IMAGE_MODELS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(snapshot["payload"].as_str().unwrap()).unwrap(),
        expected
    );
    server.shutdown().await;
    assert!(replacement.next_event().await.is_none());
}

#[tokio::test]
async fn native_events_enforce_direction_and_reconcile_lag() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let bridge = server.host_events().unwrap();
    let mut public = server.runtime_state().events.subscribe();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_TRANSPORT_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "DLNA_SENDER_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    assert!(public.try_recv().is_err());
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
                | "DLNA_POSITION_REPORTED"
                | "DLNA_PLAYBACK_REPORTED"
                | "DLNA_SEEK_CONSUMED_REPORTED"
                | "DLNA_STOPPED_REPORTED"
                | "IMAGE_CATALOG_CHANGED"
                | "MEDIA_CATALOG_CHANGED"
                | "NETWORK_INTERFACES_CHANGED"
                | "BLE_ADVERTISEMENT_REPORTED"
                | "BLE_SCAN_STATE_REPORTED"
                | "BLE_ADVERTISING_FACTS_REPORTED"
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
    let bridge = server.host_events().unwrap();
    let host = server.runtime_state().host.clone();
    let call_host = host.clone();
    let task = tokio::spawn(async move { call_host.call("fixture", json!({})).await });
    let request = loop {
        let request = bridge.next_request().await.unwrap();
        if request["method"] == "fixture" {
            break request;
        }
        assert_eq!(request["method"], "systemCapabilities");
        bridge
            .reply(json!({"id":request["id"],"result":[]}))
            .unwrap();
    };
    bridge
        .reply(json!({"id":request["id"],"result":42}))
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), json!(42));
    let task = tokio::spawn(async move { host.call("fixture", json!({})).await });
    loop {
        let request = bridge.next_request().await.unwrap();
        if request["method"] == "fixture" {
            break;
        }
        assert_eq!(request["method"], "systemCapabilities");
        bridge
            .reply(json!({"id":request["id"],"result":[]}))
            .unwrap();
    }
    let replacement = server.host_events().unwrap();
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
    let bridge = server.host_events().unwrap();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_TRANSPORT_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "DLNA_SENDER_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    let state = server.runtime_state();
    state.http_server.failed("failed".into());
    for _ in 0..300 {
        let _ = state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
    let recovery = bridge.next_event().await.unwrap();
    assert_eq!(recovery["type"], "CONTENT_CHANGED");
    assert_eq!(recovery["reconcile"], true);
    let event = bridge.next_event().await.unwrap();
    assert_eq!(event["type"], "HTTP_SERVER_STATE_CHANGED");
    let payload: Value = serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["state"], "error");
    assert_eq!(payload["message"], "failed");
    assert!(payload.get("generation").is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn replacing_a_host_wakes_a_waiting_event_receiver() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let original = server.host_events().unwrap();
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "HTTP_SERVER_STATE_CHANGED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "PEER_TRANSPORT_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "DLNA_SENDER_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        original.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    let mut waiting = Box::pin(original.next_event());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(waiting.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let replacement = server.host_events().unwrap();
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
    let original = server.host_events().unwrap();
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
        .unwrap();
    let replacement = server.host_events().unwrap();
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
    let bridge = server.host_events().unwrap();
    let state = server.runtime_state();
    let mut public = state.events.subscribe();
    let scan = uuid::Uuid::new_v4().to_string();
    bridge
        .publish_text(
            "BLE_SCAN_STATE_REPORTED",
            serde_json::json!({"scanId":scan,"active":true}).to_string(),
        )
        .await
        .unwrap();
    bridge
        .publish_text(
            "BLE_ADVERTISEMENT_REPORTED",
            serde_json::json!({"scanId":scan,"shortId":"invalid"}).to_string(),
        )
        .await
        .unwrap();
    assert!(public.try_recv().is_err());
    bridge
        .publish_text(
            "BLE_SCAN_STATE_REPORTED",
            serde_json::json!({"scanId":scan,"active":false}).to_string(),
        )
        .await
        .unwrap();
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
    let bridge = server.host_events().unwrap();
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

#[tokio::test]
async fn casting_snapshots_recover_the_current_version_without_a_query() {
    let directory = tempfile::tempdir().unwrap();
    let server = server(&directory);
    let state = server.runtime_state();
    state.cast.snapshot.lock().unwrap().items.push(
        crate::services::casting::dlna_sender_runtime::Item {
            path: "/owned/track.mp3".into(),
            title: "Owned track".into(),
            audio: true,
            ..Default::default()
        },
    );
    let bridge = server.host_events().unwrap();
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    let initial = bridge.next_event().await.unwrap();
    assert_eq!(initial["type"], "DLNA_SENDER_UPDATED");
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    let initial: Value = serde_json::from_str(initial["payload"].as_str().unwrap()).unwrap();
    assert_eq!(initial["items"][0]["path"], "/owned/track.mp3");
    state.cast.snapshot.lock().unwrap().items.clear();
    state.cast.publish(&state);
    let expected = serde_json::to_value(state.cast.snapshot()).unwrap();
    assert!(expected["version"].as_u64().unwrap() > initial["version"].as_u64().unwrap());
    for _ in 0..300 {
        state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    let recovered = bridge.next_event().await.unwrap();
    assert_eq!(recovered["type"], "DLNA_SENDER_UPDATED");
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    assert_eq!(
        serde_json::from_str::<Value>(recovered["payload"].as_str().unwrap()).unwrap(),
        expected
    );
    let replacement = server.host_events().unwrap();
    assert!(bridge.next_event().await.is_none());
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
    ] {
        assert_eq!(replacement.next_event().await.unwrap()["type"], kind);
    }
    let restored = replacement.next_event().await.unwrap();
    assert_eq!(restored["type"], "DLNA_SENDER_UPDATED");
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "APP_UPDATE_DOWNLOAD_UPDATED"
    );
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "AUDIO_PLAYBACK_UPDATED"
    );
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "PEER_NOTIFICATIONS_UPDATED"
    );
    assert_eq!(
        serde_json::from_str::<Value>(restored["payload"].as_str().unwrap()).unwrap(),
        expected
    );
    server.shutdown().await;
}

#[tokio::test]
async fn scan_facts_are_validated_and_scoped_to_the_connected_host() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(&dir);
    let old = server.host_events().unwrap();
    let id = uuid::Uuid::from_u128(1).to_string();
    let active = serde_json::json!({"scanId":id,"active":true}).to_string();
    old.publish_text("BLE_SCAN_STATE_REPORTED", active.clone())
        .await
        .unwrap();
    let bridge = server.host_events().unwrap();
    assert!(
        old.publish_text("BLE_SCAN_STATE_REPORTED", active.clone())
            .await
            .is_err()
    );
    bridge
        .publish_text("BLE_SCAN_STATE_REPORTED", active)
        .await
        .unwrap();
    drop(old);
    let state = server.runtime_state();
    assert!(matches!(
        state.ble_scans.seen(&id, "0001020304050607").unwrap(),
        plain_rs::discovery::nearby_scan::Step::Read { .. }
    ));
    for invalid in [
        serde_json::json!({"scanId":"invalid","active":true}),
        serde_json::json!({"scanId":id,"active":"true"}),
        serde_json::json!({"scanId":id,"active":true,"extra":true}),
    ] {
        assert!(
            bridge
                .publish_text("BLE_SCAN_STATE_REPORTED", invalid.to_string())
                .await
                .is_err()
        );
    }
    drop(ServingSession(&bridge));
    assert!(state.ble_scans.seen(&id, "0001020304050607").is_err());
    server.shutdown().await;
}

#[tokio::test]
async fn update_download_snapshots_recover_without_querying_or_replaying_stale_hints() {
    let dir = tempfile::tempdir().unwrap();
    let core = server(&dir);
    let state = core.runtime_state();
    let bridge = core.host_events().unwrap();
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
        "DLNA_SENDER_UPDATED",
        "APP_UPDATE_DOWNLOAD_UPDATED",
        "AUDIO_PLAYBACK_UPDATED",
        "PEER_NOTIFICATIONS_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    let expected = state.update_checks.download.snapshot_event().payload;
    state
        .events
        .send(WsEvent::broadcast(
            "APP_UPDATE_DOWNLOAD_UPDATED",
            "{\"status\":\"COMPLETED\",\"filePath\":\"stale\"}".into(),
        ))
        .unwrap();
    let current = bridge.next_event().await.unwrap();
    assert_eq!(current["type"], "APP_UPDATE_DOWNLOAD_UPDATED");
    assert_eq!(current["payload"], expected);
    for _ in 0..300 {
        state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    for kind in [
        "HTTP_SERVER_STATE_CHANGED",
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
        "DLNA_SENDER_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    let restored = bridge.next_event().await.unwrap();
    assert_eq!(restored["type"], "APP_UPDATE_DOWNLOAD_UPDATED");
    assert_eq!(restored["payload"], expected);
    core.stop_core().await;
}

#[tokio::test]
async fn http_status_snapshots_replace_stale_listener_events_without_exposing_generations() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    state.http_server.started((8080, 8443));
    let bridge = core.host_events().unwrap();
    let initial = bridge.next_event().await.unwrap();
    assert_eq!(initial["type"], "HTTP_SERVER_STATE_CHANGED");
    assert_eq!(
        serde_json::from_str::<Value>(initial["payload"].as_str().unwrap()).unwrap(),
        json!({"state":"started","message":""})
    );
    for kind in [
        "IMAGE_MODELS_UPDATED",
        "ONLINE_CLIENTS_UPDATED",
        "PEER_TRANSPORT_UPDATED",
        "DLNA_SENDER_UPDATED",
        "APP_UPDATE_DOWNLOAD_UPDATED",
        "AUDIO_PLAYBACK_UPDATED",
        "PEER_NOTIFICATIONS_UPDATED",
    ] {
        assert_eq!(bridge.next_event().await.unwrap()["type"], kind);
    }
    state
        .events
        .send(WsEvent::broadcast(
            plain_rs::events::HTTP_SERVER_FAILED,
            json!({"generation":1,"message":"old failure"}).to_string(),
        ))
        .unwrap();
    state.http_server.started((8180, 8543));
    let current = bridge.next_event().await.unwrap();
    assert_eq!(current["type"], "HTTP_SERVER_STATE_CHANGED");
    assert_eq!(
        serde_json::from_str::<Value>(current["payload"].as_str().unwrap()).unwrap(),
        json!({"state":"started","message":""})
    );
    state.http_server.failed("current failure".into());
    state.http_server.stopped();
    state
        .events
        .send(state.http_server.status().event())
        .unwrap();
    let stopped = bridge.next_event().await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(stopped["payload"].as_str().unwrap()).unwrap(),
        json!({"state":"stopped","message":""})
    );
    core.stop_core().await;
}

#[tokio::test]
async fn preference_updates_send_only_changed_values_and_skip_unchanged_values() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let bridge = Arc::new(core.host_events().unwrap());
    state.prefs.set("http_port", 8181).unwrap();
    state.prefs.set("feed_auto_refresh_interval", 900).unwrap();
    state.prefs.set("pkg_sort_by", "NAME_DESC").unwrap();
    let mut update = Box::pin(bridge.prefs_changed());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(update.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    loop {
        let request = bridge.next_request().await.unwrap();
        let is_preferences = request["method"] == "prefsChanged";
        if is_preferences {
            assert_eq!(
                request["params"],
                json!({"values":{"http_port":8181,"feed_auto_refresh_interval":900,"pkg_sort_by":"NAME_DESC"}})
            );
        }
        bridge
            .reply(json!({"id":request["id"],"result":null}))
            .unwrap();
        if is_preferences {
            break;
        }
    }
    update.await.unwrap();
    let mut unchanged = Box::pin(bridge.prefs_changed());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(unchanged.as_mut(), context).is_ready());
        std::task::Poll::Ready(())
    })
    .await;
    drop(unchanged);
    core.stop_core().await;
}

#[tokio::test]
async fn failed_preference_delivery_returns_once_and_does_not_replay_the_change() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let bridge = Arc::new(core.host_events().unwrap());
    state.prefs.set("http_port", 8181).unwrap();
    let mut update = Box::pin(bridge.prefs_changed());
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(update.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    loop {
        let request = bridge.next_request().await.unwrap();
        let is_preferences = request["method"] == "prefsChanged";
        let response = if is_preferences {
            json!({"id":request["id"],"error":"platform failure"})
        } else {
            json!({"id":request["id"],"result":null})
        };
        bridge.reply(response).unwrap();
        if is_preferences {
            break;
        }
    }
    std::future::poll_fn(
        |context| match std::future::Future::poll(update.as_mut(), context) {
            std::task::Poll::Ready(result) => {
                assert_eq!(result.unwrap_err(), "platform failure");
                std::task::Poll::Ready(())
            }
            std::task::Poll::Pending => panic!("failed preference delivery did not finish"),
        },
    )
    .await;
    drop(update);
    bridge.prefs_changed().await.unwrap();
    core.stop_core().await;
}

#[tokio::test]
async fn preference_read_failure_does_not_replace_the_running_host() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let bridge = core.host_events().unwrap();
    state.prefs.set("http_port", "invalid").unwrap();
    assert!(core.host_events().is_err());
    assert_eq!(state.host.generation(), Some(bridge.generation));
    assert!(bridge.prefs_changed().await.is_err());
    core.stop_core().await;
    assert!(core.host_events().is_err());
}
