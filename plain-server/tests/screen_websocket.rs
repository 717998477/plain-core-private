#![cfg(all(feature = "core", feature = "http_transport"))]
use futures_util::StreamExt;
use plain_server::assembly::server::ContentServer;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

fn public_fixture() -> (
    tempfile::TempDir,
    ContentServer,
    Arc<plain_rs::prefs::Prefs>,
) {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("desktop_access", true).unwrap();
    prefs.set("client_id", "screen-fixture").unwrap();
    let core = ContentServer::start(
        directory.path(),
        &plain_rs::utils::base64::base64_encode(&[4; 32]),
        prefs.clone(),
    )
    .unwrap();
    (directory, core, prefs)
}
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

#[tokio::test]
async fn screen_binary_media_is_separate_while_codec_and_controls_keep_the_global_socket() {
    let (_directory, core, prefs) = public_fixture();
    let key = plain_rs::utils::base64::base64_decode(&plain_rs::prefs::identity::ensure_url_token(
        &prefs,
    ));
    let client = prefs.get::<String>("client_id").unwrap().unwrap();
    let bridge = Arc::new(core.host_events().unwrap());
    let host = bridge.clone();
    let (sent, mut received) = mpsc::channel(8);
    let responder = tokio::spawn(async move {
        while let Some(request) = host.next_request().await {
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "systemCapabilities" => json!(["SCREEN_MIRROR"]),
                "systemScreenMirrorControls" | "systemScreenMirrorResetTouch" => {
                    sent.send((method.to_owned(), request["params"].clone()))
                        .await
                        .unwrap();
                    json!(true)
                }
                method => panic!("Unexpected platform primitive: {method}"),
            };
            let _ = host.reply(json!({"id":request["id"],"result":result}));
        }
    });
    let (mut app, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/?cid={client}", core.port))
            .await
            .unwrap();
    let registration = plain_rs::crypto::xchacha_encrypt_raw(
        &key,
        chrono::Utc::now().timestamp_millis().to_string().as_bytes(),
    )
    .unwrap();
    let mut app_stream = plain_rs::protocols::websocket::Stream::new(&registration);
    app.send(ClientMessage::Binary(registration.into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while bridge.next_event().await.unwrap()["type"] != "WEB_SOCKET_REGISTERED" {}
    })
    .await
    .unwrap();
    for payload in [
        br#"{"type":"screenMirrorControl","input":{"action":"BACK"}}"#.as_slice(),
        &[0x54, 1, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0],
    ] {
        let mut plain = chrono::Utc::now().timestamp_millis().to_be_bytes().to_vec();
        plain.extend_from_slice(payload);
        app.send(ClientMessage::Binary(
            plain_rs::crypto::xchacha_encrypt_raw(&key, &plain)
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), received.recv())
                .await
                .unwrap()
                .unwrap()
                .0,
            "systemScreenMirrorControls"
        );
    }
    let (mut screen, _) = tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{}/screen?cid={client}",
        core.port
    ))
    .await
    .unwrap();
    let registration = plain_rs::crypto::xchacha_encrypt_raw(
        &key,
        chrono::Utc::now().timestamp_millis().to_string().as_bytes(),
    )
    .unwrap();
    let mut screen_stream = plain_rs::protocols::websocket::Stream::new(&registration);
    screen
        .send(ClientMessage::Binary(registration.into()))
        .await
        .unwrap();
    screen
        .send(ClientMessage::Ping(vec![1].into()))
        .await
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), screen.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        ClientMessage::Pong(_)
    ));
    for (kind, payload) in [
        ("SCREEN_MIRROR_VIDEO", vec![0, 255, 1]),
        ("SCREEN_MIRROR_AUDIO", vec![2, 0, 3]),
    ] {
        bridge.publish_binary(kind, payload.clone()).unwrap();
        let message = tokio::time::timeout(Duration::from_secs(3), screen.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let ClientMessage::Binary(bytes) = message else {
            panic!("Expected encrypted media")
        };
        assert_eq!(
            screen_stream.decode(&bytes, &key).unwrap(),
            (kind.to_owned(), payload)
        );
    }
    bridge
        .publish_text(
            "SCREEN_MIRROR_VIDEO_CODEC",
            r#"{"annexB":"fixture"}"#.into(),
        )
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), app.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ClientMessage::Binary(bytes) = message else {
        panic!("Expected codec on global socket")
    };
    assert_eq!(
        app_stream.decode(&bytes, &key).unwrap(),
        (
            "SCREEN_MIRROR_VIDEO_CODEC".into(),
            br#"{"annexB":"fixture"}"#.to_vec()
        )
    );
    let mut control = chrono::Utc::now().timestamp_millis().to_be_bytes().to_vec();
    control.extend_from_slice(br#"{"type":"screenMirrorControl","input":{"action":"HOME"}}"#);
    let control = plain_rs::crypto::xchacha_encrypt_raw(&key, &control).unwrap();
    screen
        .send(ClientMessage::Binary(control.clone().into()))
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), screen.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(message, ClientMessage::Close(Some(frame)) if frame.reason == "invalid_request")
    );
    assert!(received.try_recv().is_err());
    for _ in 0..300 {
        bridge
            .publish_binary("SCREEN_MIRROR_VIDEO", vec![1, 2, 3])
            .unwrap();
    }
    bridge
        .publish_text("SCREEN_MIRRORING", "{}".into())
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), app.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let ClientMessage::Binary(bytes) = message else {
        panic!("Media must not fill global event queue")
    };
    assert_eq!(
        app_stream.decode(&bytes, &key).unwrap(),
        ("SCREEN_MIRRORING".into(), b"{}".to_vec())
    );
    app.send(ClientMessage::Binary(control.into()))
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), app.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(message, ClientMessage::Close(Some(frame)) if frame.reason == "invalid_request")
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), received.recv())
            .await
            .unwrap()
            .unwrap()
            .0,
        "systemScreenMirrorResetTouch"
    );
    assert!(received.try_recv().is_err());
    core.shutdown().await;
    responder.await.unwrap();
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn screen_rejects_login_modes_missing_clients_invalid_keys_and_disabled_access() {
    let (_directory, core, prefs) = public_fixture();
    let client = prefs.get::<String>("client_id").unwrap().unwrap();
    for query in [
        String::new(),
        format!("cid={client}&auth=1"),
        format!("cid={client}&auth=2"),
        format!("cid={client}"),
    ] {
        let (mut screen, _) = tokio_tungstenite::connect_async(format!(
            "ws://127.0.0.1:{}/screen?{query}",
            core.port
        ))
        .await
        .unwrap();
        if query == format!("cid={client}") {
            screen
                .send(ClientMessage::Binary(vec![0; 40].into()))
                .await
                .unwrap();
        }
        let message = tokio::time::timeout(Duration::from_secs(3), screen.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(message, ClientMessage::Close(_)));
    }
    prefs.set("desktop_access", false).unwrap();
    let (mut screen, _) = tokio_tungstenite::connect_async(format!(
        "ws://127.0.0.1:{}/screen?cid={client}",
        core.port
    ))
    .await
    .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(3), screen.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(message, ClientMessage::Close(Some(frame)) if frame.reason == "desktop_access_disabled")
    );
    core.shutdown().await;
}
