#![cfg(feature = "http_transport")]
use super::*;
use plain_rs::db::chat_store::peers;
use std::sync::Arc;
fn peer() -> DPeer {
    let mut p = DPeer::new(
        "fixture",
        "fixture",
        "",
        Some(443),
        plain_rs::chat::enums::DeviceType::Phone,
    );
    p.key = plain_rs::utils::base64::base64_encode(&[7; 32]);
    p
}
fn state() -> (
    tempfile::TempDir,
    crate::assembly::server::ContentServer,
    ServerState,
) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    let server = crate::assembly::server::ContentServer::start(
        dir.path(),
        &plain_rs::utils::base64::base64_encode(&[1; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    peers::save(
        &state.db,
        &[peer()],
        plain_rs::db::chat_store::SaveMode::Insert,
    )
    .unwrap();
    (dir, server, state)
}
#[tokio::test]
async fn rust_falls_back_from_unavailable_socket_and_authenticates_raw_ble_business_errors() {
    let (_dir, server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let ble = state.ble_transport.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = receiver.recv().await {
            let response = match request["method"].as_str().unwrap() {
                "peerTransportCapabilities" => json!({"id":request["id"],"result":["BLE","AWARE"]}),
                "peerStartAware" | "peerStopAware" => {
                    json!({"id":request["id"],"result":true})
                }
                "peerTransportSocketOpen" => {
                    let params = request["params"].as_object().unwrap();
                    assert_eq!(params.len(), 2);
                    assert!(
                        params
                            .get("peerId")
                            .and_then(serde_json::Value::as_str)
                            .is_some()
                    );
                    assert!(params.contains_key("token"));

                    json!({"id":request["id"],"error":"SDK network unavailable"})
                }
                "peerTransportSocketClose" | "peerTransportSocketCloseAll" => {
                    json!({"id":request["id"],"result":true})
                }
                "peerTransportBleExchange" => {
                    let plain_rs::protocols::ble::Request::PeerGraphql {
                        client_id,
                        channel_id,
                        body,
                    } = ble.request_for_test(&request["params"])
                    else {
                        panic!("Expected peer request")
                    };
                    assert_eq!(client_id, "actor");
                    assert_eq!(channel_id, "channel");
                    assert_eq!(
                        plain_rs::crypto::xchacha_decrypt_raw(&[7; 32], &body).unwrap(),
                        b"signed request"
                    );
                    let bytes = plain_rs::crypto::xchacha_encrypt_raw(
                        &[7; 32],
                        br#"{"errors":[{"message":"business rejection"}]}"#,
                    )
                    .unwrap();
                    json!({"id":request["id"],"result":ble.reply_for_test(&request["params"],plain_rs::protocols::ble::response(200,&bytes).unwrap())})
                }
                _ => panic!("{request}"),
            };
            host.reply(generation, response).unwrap();
        }
    });
    let response = send(&state, &peer(), "channel", &[7; 32], "signed request")
        .await
        .unwrap();
    assert_eq!(response["errors"][0]["message"], "business rejection");
    assert!(state.transport.active().is_empty());
    server.shutdown().await;
    state.host.disconnect(generation);
    responder.abort();
}
#[tokio::test]
async fn cancel_releases_route_and_malformed_raw_ble_response_is_terminal() {
    let (_dir, server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = host.connect();
    let request_state = state.clone();
    let call =
        tokio::spawn(async move { send(&request_state, &peer(), "", &[7; 32], "signed").await });
    let caps = receiver.recv().await.unwrap();
    host.reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let _exchange = receiver.recv().await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert!(state.transport.active().is_empty());
    let request_state = state.clone();
    let call =
        tokio::spawn(async move { send(&request_state, &peer(), "", &[7; 32], "signed").await });
    let caps = receiver.recv().await.unwrap();
    host.reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let exchange = receiver.recv().await.unwrap();
    host.reply(
        generation,
        json!({"id":exchange["id"],"result":state.ble_transport.reply_for_test(&exchange["params"], b"malformed response".to_vec())}),
    )
    .unwrap();
    assert!(call.await.unwrap().is_err());
    assert!(state.transport.active().is_empty());
    host.disconnect(generation);
    server.shutdown().await;
}

#[tokio::test]
async fn aware_config_uses_current_pairing_identity_and_rejects_corrupt_keys() {
    let (_dir, server, state) = state();
    state.prefs.set_user("https_port", 2443).unwrap();
    let config =
        aware_config_for_peer(&state, &peers::get(&state.db, "fixture").unwrap().unwrap()).unwrap();
    assert!(config.is_client);
    assert_eq!(config.local_port, 2443);
    assert_eq!(
        config.pmk.unwrap(),
        plain_rs::utils::base64::base64_encode(&[7; 32])
    );
    let native = server.native_graphql().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&snapshot_event(&state).payload).unwrap(),
        json!([])
    );
    let removed = native
        .execute(async_graphql::Request::new(
            "{ __type(name: \"PeerAwareConfig\") { name } }",
        ))
        .await;
    assert!(removed.errors.is_empty(), "{:?}", removed.errors);
    assert!(removed.data.into_json().unwrap()["__type"].is_null());
    let mut corrupt = peers::get(&state.db, "fixture").unwrap().unwrap();
    corrupt.key = "%%%".into();
    peers::save(
        &state.db,
        &[corrupt],
        plain_rs::db::chat_store::SaveMode::Update,
    )
    .unwrap();
    assert!(
        aware_config_for_peer(&state, &peers::get(&state.db, "fixture").unwrap().unwrap()).is_err()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn native_transport_snapshots_follow_current_routes_without_graphql_reads() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let mut first = peer();
    first.id = "b-peer".into();
    let ticket = state
        .transport
        .begin(&first, &[TransportType::Ble])
        .unwrap()
        .ticket
        .unwrap();
    let mut second = first.clone();
    second.id = "a-peer".into();
    state
        .transport
        .begin(&second, &[TransportType::Aware])
        .unwrap();
    let bridge = core.host_events();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    let initial = bridge.next_event().await.unwrap();
    assert_eq!(initial["type"], "PEER_TRANSPORT_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(initial["payload"].as_str().unwrap()).unwrap(),
        json!([
            {"peerId":"a-peer","transport":"AWARE"}, {"peerId":"b-peer","transport":"BLE"},
        ])
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
    notify(&state);
    state.transport.abort(&ticket);
    let current = bridge.next_event().await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(current["payload"].as_str().unwrap()).unwrap(),
        json!([
            {"peerId":"a-peer","transport":"AWARE"},
        ])
    );
    state.transport.forget("a-peer");
    for _ in 0..300 {
        state
            .events
            .send(plain_rs::events::WsEvent::broadcast(
                "CONTENT_CHANGED",
                "{}".into(),
            ))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "ONLINE_CLIENTS_UPDATED"
    );
    let restored = bridge.next_event().await.unwrap();
    assert_eq!(restored["type"], "PEER_TRANSPORT_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(restored["payload"].as_str().unwrap()).unwrap(),
        json!([])
    );
    core.stop_core().await;
}
