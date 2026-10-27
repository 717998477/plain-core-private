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
async fn authenticated_reply_accepts_discovery_refresh_and_rejects_revoked_pairing() {
    for change in ["address", "key", "public_key", "status", "removed"] {
        let (_dir, server, state) = state();
        let host = state.host.clone();
        let (generation, mut requests) = crate::test_support::HostRequests::connect(&host);
        let sender = state.clone();
        let call =
            tokio::spawn(
                async move { send(&sender, &peer(), "", &[7; 32], "signed request").await },
            );
        let caps = requests.recv().await.unwrap();
        host.reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
            .unwrap();
        let exchange = requests.recv().await.unwrap();
        assert_eq!(exchange["method"], "peerTransportBleExchange");
        let plain_rs::protocols::ble::Request::PeerGraphql { body, .. } =
            state.ble_transport.request_for_test(&exchange["params"])
        else {
            panic!("Expected peer request")
        };
        let mut changed = peer();
        match change {
            "address" => {
                changed.ip = "192.0.2.9".into();
                changed.port = Some(2443);
                changed.name = "Refreshed name".into();
            }
            "key" => changed.key = plain_rs::utils::base64::base64_encode(&[9; 32]),
            "public_key" => changed.public_key = "replacement".into(),
            "status" => changed.status = plain_rs::chat::enums::PeerStatus::Paired,
            "removed" => {}
            _ => unreachable!(),
        }
        if change == "removed" {
            peers::delete(&state.db, &[changed.id.clone()]).unwrap();
        } else {
            peers::save(
                &state.db,
                &[changed],
                plain_rs::db::chat_store::SaveMode::Update,
            )
            .unwrap();
        }
        let bytes = plain_rs::transport::http_response::Context::new("/peer_graphql", &body)
            .encrypt(&[7; 32], br#"{"data":{"message":true}}"#)
            .unwrap();
        let reply = state.ble_transport.reply_for_test(
            &exchange["params"],
            plain_rs::protocols::ble::response(200, &bytes).unwrap(),
        );
        host.reply(generation, json!({"id":exchange["id"],"result":reply}))
            .unwrap();
        let result = call.await.unwrap();
        if change == "address" {
            assert_eq!(result.unwrap()["data"]["message"], true);
        } else {
            assert_eq!(
                result.unwrap_err(),
                if change == "removed" {
                    "Peer removed"
                } else {
                    "Peer changed"
                }
            );
        }
        assert!(state.transport.active().is_empty());
        host.disconnect(generation);
        server.shutdown().await;
    }
}
#[tokio::test]
async fn rust_falls_back_from_unavailable_socket_and_authenticates_raw_ble_business_errors() {
    let (_dir, server, state) = state();
    let host = state.host.clone();
    let (generation, mut receiver) = crate::test_support::HostRequests::connect(&host);
    let ble = state.ble_transport.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = receiver.recv().await {
            let response = match request["method"].as_str().unwrap() {
                "systemCapabilities" => json!({"id":request["id"],"result":["WIFI_AWARE"]}),
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
                    let bytes =
                        plain_rs::transport::http_response::Context::new("/peer_graphql", &body)
                            .encrypt(
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
    state.prefs.set("https_port", 2443).unwrap();
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
        snapshot(&state)["active"],
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
async fn host_transport_snapshots_follow_current_routes_and_reconnect() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let mut first = peer();
    first.id = "b-peer".into();
    let ticket = state.transport.begin(&first, &[TransportType::Ble]).unwrap().ticket.unwrap();
    let mut second = first.clone();
    second.id = "a-peer".into();
    state.transport.begin(&second, &[TransportType::Aware]).unwrap();
    let bridge = core.host_events().unwrap();
    assert_eq!(transport_callback(&bridge).await, json!([
        {"peerId":"a-peer","transport":"AWARE"}, {"peerId":"b-peer","transport":"BLE"},
    ]));
    state.transport.abort(&ticket);
    notify(&state);
    assert_eq!(transport_callback(&bridge).await, json!([{"peerId":"a-peer","transport":"AWARE"}]));
    state.transport.forget("a-peer");
    let replacement = core.host_events().unwrap();
    assert_eq!(transport_callback(&replacement).await, json!([]));
    core.stop_core().await;
}

async fn transport_callback(bridge: &crate::host::events::HostEvents) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let request = bridge.next_request().await.unwrap();
            bridge.reply(json!({"id":request["id"],"result":null})).unwrap();
            if request["method"] == "peerTransportsChanged" { break request["params"]["active"].clone(); }
        }
    }).await.unwrap()
}

#[tokio::test]
async fn shared_capabilities_use_current_host_facts_and_reject_invalid_receipts() {
    let host = Arc::new(crate::host::bridge::Host::default());
    assert!(capabilities(&host).await.unwrap().is_empty());
    let (generation, mut requests) = host.connect();
    for (facts, expected) in [
        (
            json!(["BLE", "AWARE"]),
            Some(vec![TransportType::Ble, TransportType::Aware]),
        ),
        (json!([]), Some(vec![])),
        (json!({"bleReady":true}), None),
    ] {
        let reader = host.clone();
        let task = tokio::spawn(async move { capabilities(&reader).await });
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "peerTransportCapabilities");
        host.reply(generation, json!({"id":request["id"],"result":facts}))
            .unwrap();
        match expected {
            Some(expected) => assert_eq!(task.await.unwrap().unwrap(), expected),
            None => assert!(task.await.unwrap().is_err()),
        }
        assert!(requests.try_recv().is_err());
    }
    host.disconnect(generation);
    assert!(capabilities(&host).await.unwrap().is_empty());
}
