use super::*;
#[test]
fn upstream_json_keeps_the_control_envelope_registry() {
    let input=controls(br#"{"type":"screenMirrorControl","input":{"action":"SCROLL","x":0.5,"y":0.25,"deltaX":-10,"deltaY":120}}"#);
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["action"], "SCROLL");
    assert_eq!(input[0]["deltaX"], -10);
    for bytes in [
        br#"{"type":"deviceCommand","action":"BACK"}"#.as_slice(),
        br#"{"action":"BACK"}"#,
        b"not json",
        b"{}",
        b"[1,2,3]",
        br#"{"type":"screenMirrorControl","input":{"action":"INVALID"}}"#,
    ] {
        assert!(controls(bytes).is_empty());
    }
}
#[test]
fn touch_frames_keep_endianness_order_cancel_and_the_reserved_stream_id() {
    let bytes = [
        0x54, 4, 0xff, 0xff, 0, 7, 0x34, 0x12, 0x78, 0x56, 0, 0, 1, 3, 0xff, 0xff, 0xff, 0xff, 0,
        0, 2, 7, 0, 0, 0, 0, 0, 0, 3, 7, 0, 0, 0, 0, 0, 0,
    ];
    let items = controls(&bytes);
    assert_eq!(items.len(), 4);
    assert_eq!(
        items
            .iter()
            .map(|row| row["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["TOUCH_DOWN", "TOUCH_MOVE", "TOUCH_UP", "TOUCH_UP"]
    );
    assert_eq!(items[0]["pointerId"], 7);
    assert_eq!(
        items[0]["x"].as_f64().unwrap(),
        (f32::from(0x1234u16) / 65535.0) as f64
    );
    assert_eq!(
        items[0]["y"].as_f64().unwrap(),
        (f32::from(0x5678u16) / 65535.0) as f64
    );
    assert_eq!(items[1]["x"], 1.0);
    for bytes in [
        b"".as_slice(),
        &[0x54],
        &[0x54, 0, 0, 0],
        &[0x53, 1, 0, 0],
        &[0x54, 1, 0, 0, 0],
    ] {
        assert!(controls(bytes).is_empty());
    }
}
#[test]
fn rejecting_confirmation_retains_the_legacy_retry_close_code() {
    let runtime = Runtime::default();
    let (outgoing, _) = mpsc::channel(1);
    let (closing, receiver) = tokio::sync::watch::channel(None);
    runtime.connections.lock().unwrap().insert(
        "connection".into(),
        Connection {
            client_id: "client".into(),
            registered: false,
            pending: Some("request".into()),
            outgoing,
            closing,
        },
    );
    runtime.reject("request");
    assert_eq!(*receiver.borrow(), Some((1013, "rejected")));
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn auth_confirmation_uses_native_events_and_returns_the_hosts_decision_to_the_browser() {
    use futures_util::SinkExt;
    use sha2::Digest;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    prefs.set_user("desktop_access", true).unwrap();
    prefs.set("auth_two_factor", true).unwrap();
    prefs.set("password", "secret").unwrap();
    prefs.set("client_id", "phone").unwrap();
    let (private, _) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&private),
        )
        .unwrap();
    let server = crate::assembly::server::ContentServer::start(
        dir.path(),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let bridge = std::sync::Arc::new(server.host_events());
    let host = bridge.clone();
    let consumer = tokio::spawn(async move {
        while let Some(request) = host.next_request().await {
            assert_ne!(request["method"], "systemWebLoginRequest");
            host.reply(json!({"id":request["id"],"result":null}))
                .unwrap();
        }
    });
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (port, _) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let digest = plain_rs::utils::hex::bytes_to_hex(&sha2::Sha512::digest(b"secret"));
    let client = plain_rs::crypto::EcdhSession::generate();
    let request = json!({"password":digest,"browserName":"Test","browserVersion":"1","osName":"Fixture","osVersion":"1","isMobile":false,"ecdhPublicKey":plain_rs::utils::base64::base64_encode(&client.public_key_bytes),"peer":null});
    let frame = plain_rs::crypto::xchacha_encrypt_raw(
        &digest.as_bytes()[..32],
        &serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    for (cid, allowed) in [("rejected-browser", false), ("accepted-browser", true)] {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/?cid={cid}&auth=1"))
                .await
                .unwrap();
        socket
            .send(ClientMessage::Binary(frame.clone().into()))
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(response, ClientMessage::Binary(_)));
        let event = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let event = bridge.next_event().await.unwrap();
                if event["type"] == "WEB_LOGIN_REQUESTED" {
                    break event;
                }
            }
        })
        .await
        .unwrap();
        let payload: Value = serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["clientId"], cid);
        assert_eq!(payload["request"]["password"], "");
        assert_eq!(payload["request"]["browserName"], "Test");
        assert!(
            plain_rs::events::WsEvent::broadcast("WEB_LOGIN_REQUESTED", String::new())
                .is_internal()
        );
        let id = payload["requestId"].as_str().unwrap();
        confirm_login(&server, id, allowed).await.unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if allowed {
            let ClientMessage::Binary(frame) = response else {
                panic!("expected completed auth frame")
            };
            let decrypted =
                plain_rs::crypto::xchacha_decrypt_raw(&digest.as_bytes()[..32], &frame).unwrap();
            let result: Value = serde_json::from_slice(&decrypted).unwrap();
            assert_eq!(result["status"], "COMPLETED");
            assert!(
                server
                    .runtime_state()
                    .db
                    .session_get(cid)
                    .unwrap()
                    .is_some()
            );
        } else {
            let ClientMessage::Close(Some(frame)) = response else {
                panic!("expected rejected close frame")
            };
            assert_eq!(u16::from(frame.code), 1013);
            assert_eq!(frame.reason, "rejected");
            assert!(
                server
                    .runtime_state()
                    .db
                    .session_get(cid)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(confirm_login(&server, id, true).await.is_err());
    }
    server.shutdown().await;
    consumer.await.unwrap();
}

fn public_fixture() -> (tempfile::TempDir, crate::assembly::server::ContentServer) {
    let directory = tempfile::tempdir().unwrap();
    let prefs = std::sync::Arc::new(
        plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap(),
    );
    prefs.set_user("desktop_access", true).unwrap();
    let core = crate::assembly::server::ContentServer::start(
        directory.path(),
        &plain_rs::utils::base64::base64_encode(&[4; 32]),
        prefs,
    )
    .unwrap();
    let bundle = directory.path().join("web");
    std::fs::create_dir(&bundle).unwrap();
    std::fs::write(
        bundle.join("index.html"),
        "<!doctype html><html><head></head><body>fixture</body></html>",
    )
    .unwrap();
    core.set_web_root(bundle.to_str().unwrap());
    (directory, core)
}

#[tokio::test]
async fn root_serves_the_spa_and_old_ws_path_never_upgrades() {
    let (_directory, core) = public_fixture();
    let client = reqwest::Client::new();
    for path in ["/", "/ws?cid=browser"] {
        let response = client
            .get(format!("http://127.0.0.1:{}{path}", core.port))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert!(response.text().await.unwrap().contains("fixture"));
    }
    let denied =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws?cid=browser", core.port))
            .await;
    match denied {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), 200)
        }
        _ => panic!("the retired /ws endpoint must never upgrade"),
    }
    core.shutdown().await;
}

#[tokio::test]
async fn root_upgrades_then_closes_missing_clients_as_the_pre_migration_app_did() {
    use futures_util::SinkExt;
    let (_directory, core) = public_fixture();
    let (mut socket, response) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/", core.port))
            .await
            .unwrap();
    assert_eq!(response.status(), 101);
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match frame {
        tokio_tungstenite::tungstenite::Message::Close(Some(frame)) => {
            assert_eq!(u16::from(frame.code), 1008);
            assert_eq!(frame.reason, "`cid` is missing");
        }
        _ => panic!("missing clients require a policy close"),
    }
    let (mut socket, response) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/?cid=browser", core.port))
            .await
            .unwrap();
    assert_eq!(response.status(), 101);
    socket.close(None).await.unwrap();
    core.shutdown().await;
}

#[tokio::test]
async fn native_event_socket_registers_without_a_session_and_encrypts_events_with_the_owned_url_key()
 {
    use futures_util::SinkExt;
    let (_directory, core) = public_fixture();
    let state = core.runtime_state();
    let key = plain_rs::utils::base64::base64_decode(&plain_rs::prefs::identity::ensure_url_token(
        &state.prefs,
    ));
    let id = state.native_client_id().unwrap();
    let mut events = state.events.subscribe();
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/?cid={id}", core.port))
            .await
            .unwrap();
    let frame = plain_rs::crypto::xchacha_encrypt_raw(
        &key,
        chrono::Utc::now().timestamp_millis().to_string().as_bytes(),
    )
    .unwrap();
    let mut stream = plain_rs::protocols::websocket::Stream::new(&frame);
    socket
        .send(tokio_tungstenite::tungstenite::Message::Binary(
            frame.into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if events.recv().await.unwrap().event_type == "WEB_SOCKET_REGISTERED" {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(state.main_ws.snapshot().contains(&id));
    assert!(state.db.session_get(&id).unwrap().is_none());
    state
        .events
        .send(plain_rs::events::WsEvent::broadcast(
            "CONTENT_CHANGED",
            json!({"probe":true}).to_string(),
        ))
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let tokio_tungstenite::tungstenite::Message::Binary(bytes) = frame else {
        panic!("Expected encrypted event bytes")
    };
    let (kind, payload) = stream.decode(&bytes, &key).unwrap();
    assert_eq!(kind, "CONTENT_CHANGED");
    assert_eq!(
        serde_json::from_slice::<Value>(&payload).unwrap(),
        json!({"probe":true})
    );
    for kind in [
        "SCREEN_MIRROR_VIDEO",
        "SCREEN_MIRROR_AUDIO",
        "IMAGE_EDITOR_UPDATE",
    ] {
        let payload = vec![0, 1, 255, 0, 4];
        state
            .events
            .send(WsEvent::broadcast_binary(kind, payload.clone()))
            .unwrap();
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let tokio_tungstenite::tungstenite::Message::Binary(frame) = message else {
            panic!("Expected encrypted binary event");
        };
        let (_, encrypted) = plain_rs::protocols::websocket::decode_raw(&frame).unwrap();
        assert_ne!(encrypted, payload);
        assert_eq!(stream.decode(&frame, &key).unwrap(), (kind.into(), payload));
        assert!(stream.decode(&frame, &key).is_none());
        assert!(
            plain_rs::protocols::websocket::Stream::new(b"another registration")
                .decode(&frame, &key)
                .is_none()
        );
    }
    assert!(!state.host.connected());
    socket.close(None).await.unwrap();
    core.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn pairing_success_keeps_native_credentials_out_of_every_network_event_socket() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;

    let (_directory, core) = public_fixture();
    let state = core.runtime_state();
    let native = core.native_graphql().unwrap();
    let mut native_events = native.subscribe_events(Default::default()).unwrap();
    let session =
        crate::services::sessions::create(&state, "Pairing event fixture".into()).unwrap();
    let clients = [
        (session.client_id, plain_rs::base64_decode(&session.token)),
        (
            state.native_client_id().unwrap(),
            plain_rs::base64_decode(&core.native_url_token().unwrap()),
        ),
    ];
    let mut sockets = Vec::new();
    for (id, key) in clients {
        let mut events = state.events.subscribe();
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/?cid={id}", core.port))
                .await
                .unwrap();
        let frame = plain_rs::crypto::xchacha_encrypt_raw(
            &key,
            chrono::Utc::now().timestamp_millis().to_string().as_bytes(),
        )
        .unwrap();
        let stream = plain_rs::protocols::websocket::Stream::new(&frame);
        socket
            .send(ClientMessage::Binary(frame.into()))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if events.recv().await.unwrap().event_type == "WEB_SOCKET_REGISTERED" {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(state.main_ws.snapshot().contains(&id));
        sockets.push((socket, key, stream));
    }
    let key = plain_rs::base64_encode(&[19; 32]);
    crate::services::discovery::pairing_runtime::success(
        &state,
        &json!({"id":"paired-peer","name":"Device","key":key}),
        "192.0.2.1",
    );
    let event = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = native_events.recv().await.unwrap();
            if event.event_type == plain_rs::events::WS_PAIRING_SUCCESS {
                break event;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&event.payload).unwrap()["key"],
        key
    );
    for (socket, session_key, stream) in &mut sockets {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ClientMessage::Binary(frame) = message else {
            panic!("Expected encrypted pairing event")
        };
        let (kind, payload) = stream.decode(&frame, session_key).unwrap();
        assert_eq!(kind, plain_rs::events::WS_PAIRING_SUCCESS);
        assert_eq!(
            serde_json::from_slice::<Value>(&payload).unwrap(),
            json!({"deviceId":"paired-peer","deviceName":"Device","ip":"192.0.2.1"})
        );
    }
    for event in [
        WsEvent::broadcast(plain_rs::events::WS_PAIRING_SUCCESS, "not json".into()),
        WsEvent::broadcast_binary(plain_rs::events::WS_PAIRING_SUCCESS, key.into_bytes()),
        WsEvent::broadcast("WEB_LOGIN_REQUESTED", "private".into()),
        WsEvent::broadcast("CONTENT_CHANGED", json!({"probe":true}).to_string()),
    ] {
        state.events.send(event).unwrap();
    }
    for (mut socket, session_key, mut stream) in sockets {
        let message = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ClientMessage::Binary(frame) = message else {
            panic!("Expected encrypted content event")
        };
        let (kind, payload) = stream.decode(&frame, &session_key).unwrap();
        assert_eq!(kind, "CONTENT_CHANGED");
        assert_eq!(
            serde_json::from_slice::<Value>(&payload).unwrap(),
            json!({"probe":true})
        );
        socket.close(None).await.unwrap();
    }
    core.shutdown().await;
}

async fn confirm_login(
    server: &crate::assembly::server::ContentServer,
    id: &str,
    allowed: bool,
) -> Result<(), String> {
    server
        .native_management()?
        .confirm_web_login(id.to_owned(), allowed)
        .await
        .map(|_| ())
}

#[tokio::test]
async fn encrypted_timed_controls_forward_exact_json_and_touch_inputs_once_and_refuse_replay() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    let (_directory, core) = public_fixture();
    let state = core.runtime_state();
    let key = plain_rs::utils::base64::base64_decode(&plain_rs::prefs::identity::ensure_url_token(
        &state.prefs,
    ));
    let client = state.native_client_id().unwrap();
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let (sent, mut received) = tokio::sync::mpsc::channel(8);
    let responder = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemCapabilities" => json!([]),
                "systemScreenMirrorResetTouch" => json!(true),
                "systemScreenMirrorControls" => {
                    sent.send(request["params"]["inputs"].clone())
                        .await
                        .unwrap();
                    json!(true)
                }
                method => panic!("Unexpected platform primitive: {method}"),
            };
            let _ = host.reply(generation, json!({"id":request["id"],"result":result}));
        }
    });
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/?cid={client}", core.port))
            .await
            .unwrap();
    socket
        .send(ClientMessage::Binary(
            plain_rs::crypto::xchacha_encrypt_raw(
                &key,
                chrono::Utc::now().timestamp_millis().to_string().as_bytes(),
            )
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let json_control = br#"{"type":"screenMirrorControl","input":{"action":"BACK"}}"#;
    let touch = [0x54, 1, 0xff, 0xff, 0, 7, 0x34, 0x12, 0x78, 0x56, 0, 0];
    let mut last = Vec::new();
    for payload in [json_control.as_slice(), touch.as_slice()] {
        let mut plain = chrono::Utc::now().timestamp_millis().to_be_bytes().to_vec();
        plain.extend_from_slice(payload);
        last = plain_rs::crypto::xchacha_encrypt_raw(&key, &plain).unwrap();
        socket
            .send(ClientMessage::Binary(last.clone().into()))
            .await
            .unwrap();
        let inputs = tokio::time::timeout(Duration::from_secs(3), received.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inputs, Value::Array(controls(payload)));
    }
    socket
        .send(ClientMessage::Binary(last.into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(message) = socket.next().await {
            if let ClientMessage::Close(Some(close)) = message.unwrap() {
                assert_eq!(u16::from(close.code), 1013);
                assert_eq!(close.reason, "invalid_request");
                return;
            }
        }
        panic!("Replayed control must receive an explicit rejection");
    })
    .await
    .unwrap();
    assert!(received.try_recv().is_err());
    core.shutdown().await;
    state.host.disconnect(generation);
    tokio::time::timeout(Duration::from_secs(3), responder)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn native_online_snapshots_recover_on_attach_lag_and_host_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let insert = |id: &str, registered: bool| {
        let (outgoing, _) = mpsc::channel(1);
        let (closing, _) = tokio::sync::watch::channel(None);
        state.main_ws.connections.lock().unwrap().insert(
            id.into(),
            Connection {
                client_id: id.into(),
                registered,
                outgoing,
                pending: None,
                closing,
            },
        );
    };
    insert("owned-client", true);
    insert("unregistered-client", false);
    let bridge = core.host_events();
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    let initial = bridge.next_event().await.unwrap();
    assert_eq!(initial["type"], "ONLINE_CLIENTS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(initial["payload"].as_str().unwrap()).unwrap(),
        json!(["owned-client"])
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

    state
        .events
        .send(WsEvent::broadcast(
            "ONLINE_CLIENTS_UPDATED",
            json!(["stale-client"]).to_string(),
        ))
        .unwrap();
    state
        .main_ws
        .connections
        .lock()
        .unwrap()
        .remove("owned-client");
    insert("current-client", true);
    let current = bridge.next_event().await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(current["payload"].as_str().unwrap()).unwrap(),
        json!(["current-client"])
    );

    for _ in 0..300 {
        state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()))
            .unwrap();
    }
    assert_eq!(bridge.next_event().await.unwrap()["reconcile"], true);
    assert_eq!(
        bridge.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    let recovered = bridge.next_event().await.unwrap();
    assert_eq!(recovered["type"], "ONLINE_CLIENTS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(recovered["payload"].as_str().unwrap()).unwrap(),
        json!(["current-client"])
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

    let replacement = core.host_events();
    assert!(bridge.next_event().await.is_none());
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "IMAGE_MODELS_UPDATED"
    );
    let restored = replacement.next_event().await.unwrap();
    assert_eq!(restored["type"], "ONLINE_CLIENTS_UPDATED");
    assert_eq!(
        serde_json::from_str::<Value>(restored["payload"].as_str().unwrap()).unwrap(),
        json!(["current-client"])
    );
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "PEER_TRANSPORT_UPDATED"
    );
    assert_eq!(
        replacement.next_event().await.unwrap()["type"],
        "DLNA_SENDER_UPDATED"
    );
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
    core.stop_core().await;
    assert!(replacement.next_event().await.is_none());
}
