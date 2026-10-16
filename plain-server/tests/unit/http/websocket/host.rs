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
    let frame = plain_rs::crypto::xchacha_encrypt_raw(&key, b"1700000000000").unwrap();
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
    let (kind, payload) = plain_rs::protocols::websocket::decode(&bytes, &key).unwrap();
    assert_eq!(kind, "CONTENT_CHANGED");
    assert_eq!(
        serde_json::from_slice::<Value>(&payload).unwrap(),
        json!({"probe":true})
    );
    assert!(!state.host.connected());
    socket.close(None).await.unwrap();
    core.shutdown().await;
}

async fn confirm_login(
    server: &crate::assembly::server::ContentServer,
    id: &str,
    allowed: bool,
) -> Result<(), String> {
    let response = server.native_graphql()?.execute(
        async_graphql::Request::new("mutation($id:String!,$allowed:Boolean!){confirmWebLogin(requestId:$id,allowed:$allowed)}")
            .variables(async_graphql::Variables::from_json(serde_json::json!({"id":id,"allowed":allowed}))),
    ).await;
    if let Some(error) = response.errors.first() {
        return Err(error.message.clone());
    }
    Ok(())
}
