use super::*;
use plain_rs::prefs::Prefs;

#[test]
fn local_request_pages_are_bounded_and_never_expose_request_secrets() {
    let mut attempts = LoginAttempts::default();
    for index in (0..66).rev() {
        attempts.pending.insert(format!("{index:032x}"), Pending {
            client_id: "界".repeat(200), client_ip: "fixture".into(),
            request: json!({"browserName":"界".repeat(200), "password":"fixture-password-secret", "token":"fixture-token-secret", "ecdhPublicKey":"fixture-key-secret", "verificationCode":"forged-code"}),
            at: Instant::now(), protection: Protection::Password("fixture-digest-secret".into()),
        });
    }
    attempts
        .pending
        .get_mut(&format!("{:032x}", 65))
        .unwrap()
        .at = Instant::now() - Duration::from_secs(301);
    let first = attempts.pending_requests(None);
    assert_eq!(first.requests.len(), 64);
    assert_eq!(
        first.next_after.as_deref(),
        Some(format!("{:032x}", 63).as_str())
    );
    assert_eq!(first.requests[0].client_id.chars().count(), 128);
    assert_eq!(first.requests[0].browser_name.chars().count(), 128);
    assert!(
        first
            .requests
            .iter()
            .all(|entry| entry.verification_code.is_none())
    );
    let json = serde_json::to_string(&first).unwrap();
    for secret in [
        "fixture-password-secret",
        "fixture-token-secret",
        "fixture-key-secret",
        "fixture-digest-secret",
        "forged-code",
    ] {
        assert!(!json.contains(secret));
    }
    let last = attempts.pending_requests(first.next_after.as_deref());
    assert_eq!(last.requests.len(), 1);
    assert_eq!(last.requests[0].request_id, format!("{:032x}", 64));
    assert!(last.next_after.is_none());
}

#[cfg(all(
    feature = "http_transport",
    any(target_os = "linux", target_os = "macos")
))]
#[tokio::test]
async fn local_approval_socket_controls_real_passwordless_browser_sessions() {
    use crate::assembly::headless::login;
    use tokio_tungstenite::tungstenite::Message;
    let (directory, server, port) = passwordless_server().await;
    let service = login::Service::start(&server).await.unwrap().unwrap();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    for allowed in [false, true] {
        let client_id = if allowed {
            "local-approved"
        } else {
            "local-denied"
        };
        let (mut socket, channel, session, _) = passwordless_browser(&state, port, client_id).await;
        let event = pending_event(&mut events).await;
        let page = login::requests(directory.path(), None).await.unwrap();
        assert_eq!(page.requests.len(), 1);
        let request = &page.requests[0];
        assert_eq!(request.request_id, event["requestId"].as_str().unwrap());
        assert_eq!(
            request.verification_code.as_deref(),
            Some(channel.verification_code.as_str())
        );
        assert!(state.db.session_get(client_id).unwrap().is_none());
        login::decide(directory.path(), request.request_id.clone(), allowed)
            .await
            .unwrap();
        closed_event(&mut events, &request.request_id).await;
        assert!(
            login::requests(directory.path(), None)
                .await
                .unwrap()
                .requests
                .is_empty()
        );
        assert!(
            login::decide(directory.path(), request.request_id.clone(), true)
                .await
                .is_err()
        );
        let response = login_frame(&mut socket).await;
        if allowed {
            let Message::Binary(bytes) = response else {
                panic!("encrypted completion required")
            };
            let completed: Value =
                serde_json::from_slice(&channel.decrypt_response(&bytes, now_ms()).unwrap())
                    .unwrap();
            assert_eq!(completed["status"], "COMPLETED");
            let shared = session
                .compute_shared_key(&plain_rs::utils::base64::base64_decode(
                    completed["ecdhPublicKey"].as_str().unwrap(),
                ))
                .unwrap();
            assert_eq!(
                state.db.session_get(client_id).unwrap().unwrap().token,
                plain_rs::utils::base64::base64_encode(&shared)
            );
        } else {
            assert!(matches!(response, Message::Close(Some(close)) if close.reason == "rejected"));
            assert!(state.db.session_get(client_id).unwrap().is_none());
        }
    }
    server.stop_core().await;
    service.shutdown().await.unwrap();
    server.shutdown().await;
    assert!(login::requests(directory.path(), None).await.is_err());
}

#[test]
fn the_login_key_is_a_truncation_of_the_same_hex_the_client_sends() {
    let digest = Sha512::digest(b"secret");
    let hex = plain_rs::utils::hex::bytes_to_hex(&digest);
    let key = hash_to_token(&hex);
    assert_eq!(key.len(), 32);
    // The comparison is against the FULL digest hex (SHA-512 = 128 chars); the
    // key is the first 32 ASCII characters of it. Conflating the two silently
    // rejects every login.
    assert_eq!(key, hex.as_bytes()[..32].to_vec());
    assert_eq!(hex.len(), 128);
    assert_ne!(key, hex.as_bytes()[..128].to_vec());
}

#[test]
fn peer_blocks_are_only_accepted_when_usable() {
    let good = json!({"port": 8080, "signaturePublicKey": plain_rs::utils::base64::base64_encode(&[7u8; 32])});
    assert!(peer_chat_paired(&json!({"peer": good})));
    assert!(!peer_chat_paired(&json!({})));
    assert!(!peer_chat_paired(&json!({"peer": null})));
    assert!(!peer_chat_paired(
        &json!({"peer": {"port": 0, "signaturePublicKey": ""}})
    ));
    assert!(!peer_chat_paired(
        &json!({"peer": {"port": 8080, "signaturePublicKey": "not-a-key"}})
    ));
}

#[test]
fn signature_data_matches_the_client_verification_format() {
    assert_eq!(
        signature_data("device", "COMPLETED", "pk", 42, false),
        "device|COMPLETED|pk|42"
    );
    assert_eq!(
        signature_data("device", "COMPLETED", "pk", 42, true),
        "device|COMPLETED|pk|42|true"
    );
}

#[test]
fn login_attempts_are_rate_limited_per_key() {
    let mut attempts = LoginAttempts::default();
    for index in 0..LOGIN_ATTEMPT_LIMIT {
        assert!(attempts.acquire("fixture-a", &index.to_string()));
    }
    assert!(
        !attempts.acquire("fixture-a", "blocked"),
        "the window must lock out"
    );
    assert!(
        attempts.acquire("fixture-b", "unrelated"),
        "another address is unaffected"
    );
}

#[test]
fn signing_uses_the_core_keypair() {
    let dir = std::env::temp_dir().join(format!(
        "plain-ws-login-sign-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("prefs.json")).unwrap();
    let (keypair, public) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&keypair),
        )
        .unwrap();

    let signature = sign(&prefs, "device|COMPLETED|pk|42").unwrap();
    assert!(
        !signature.is_empty(),
        "an empty signature looks like a valid answer and fails only on the client"
    );
    assert!(plain_rs::crypto::ed25519_verify(
        &plain_rs::utils::base64::base64_encode(&public),
        b"device|COMPLETED|pk|42",
        &signature
    ));
}

#[test]
fn signing_a_broken_keypair_is_an_error_not_an_empty_signature() {
    let dir = std::env::temp_dir().join(format!(
        "plain-ws-login-broken-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let prefs = Prefs::load(&dir.join("prefs.json")).unwrap();
    prefs.set("signature_key_pair", "AAAA").unwrap();
    assert!(sign(&prefs, "device|COMPLETED|pk|42").is_err());
}

#[cfg(feature = "http_transport")]
type LoginSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[cfg(feature = "http_transport")]
async fn passwordless_server() -> (
    tempfile::TempDir,
    crate::assembly::server::ContentServer,
    u16,
) {
    let directory = tempfile::tempdir().unwrap();
    let server = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = server.runtime_state();
    state.prefs.set("password_type", 2_i32).unwrap();
    state.prefs.set("login_require_approval", true).unwrap();
    state.prefs.set("password", "unexposed-password").unwrap();
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
    (directory, server, port)
}

#[cfg(feature = "http_transport")]
async fn login_frame(socket: &mut LoginSocket) -> tokio_tungstenite::tungstenite::Message {
    use futures_util::StreamExt;
    tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[cfg(feature = "http_transport")]
async fn passwordless_browser(
    state: &ServerState,
    port: u16,
    client_id: &str,
) -> (
    LoginSocket,
    super::super::login_channel::Channel,
    plain_rs::crypto::EcdhSession,
    Vec<u8>,
) {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/?cid={client_id}&auth=2"))
            .await
            .unwrap();
    let (client, hello) = super::super::login_channel::Client::start(client_id).unwrap();
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let Message::Text(hello) = login_frame(&mut socket).await else {
        panic!("signed hello required")
    };
    let public = crate::services::peers::peer_wire::signing_keypair(&state.prefs).unwrap();
    let channel = client
        .accept(
            &serde_json::from_str(&hello).unwrap(),
            &plain_rs::utils::base64::base64_encode(&public[32..]),
            &state.native_client_id().unwrap(),
            now_ms(),
        )
        .unwrap();
    let session = plain_rs::crypto::EcdhSession::generate();
    let request = json!({"browserName":"Fixture","browserVersion":"1","osName":"Fixture","osVersion":"1","isMobile":false,"ecdhPublicKey":plain_rs::utils::base64::base64_encode(&session.public_key_bytes),"loginContext":channel.context});
    let encrypted = channel
        .encrypt_request(&serde_json::to_vec(&request).unwrap(), now_ms())
        .unwrap();
    socket
        .send(Message::Binary(encrypted.clone().into()))
        .await
        .unwrap();
    let Message::Binary(pending) = login_frame(&mut socket).await else {
        panic!("encrypted login response required")
    };
    let pending: Value =
        serde_json::from_slice(&channel.decrypt_response(&pending, now_ms()).unwrap()).unwrap();
    assert_eq!(
        pending["status"],
        if state.prefs.get_or("login_require_approval", true) {
            "PENDING"
        } else {
            "COMPLETED"
        }
    );
    (socket, channel, session, encrypted)
}

#[cfg(feature = "http_transport")]
async fn pending_event(
    events: &mut tokio::sync::broadcast::Receiver<plain_rs::events::WsEvent>,
) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event_type == "WEB_LOGIN_REQUESTED" {
                break serde_json::from_str(&event.payload).unwrap();
            }
        }
    })
    .await
    .unwrap()
}

#[cfg(feature = "http_transport")]
async fn closed_event(
    events: &mut tokio::sync::broadcast::Receiver<plain_rs::events::WsEvent>,
    request_id: &str,
) {
    let payload: Value = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event_type == "WEB_LOGIN_CLOSED" {
                break serde_json::from_str(&event.payload).unwrap();
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(payload, json!({"requestId":request_id}));
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn passwordless_hello_consumes_the_same_ip_limiter_before_key_exchange() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    let (_directory, server, port) = passwordless_server().await;
    let (_, hello) = super::super::login_channel::Client::start("browser").unwrap();
    for index in 0..=LOGIN_ATTEMPT_LIMIT {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/?cid=browser&auth=2"))
                .await
                .unwrap();
        socket
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        let response = login_frame(&mut socket).await;
        if index < LOGIN_ATTEMPT_LIMIT {
            assert!(matches!(response, Message::Text(_)));
        } else {
            assert!(
                matches!(response, Message::Close(Some(close)) if close.reason == "too_many_login_attempts")
            );
        }
        let _ = socket.close(None).await;
    }
    assert!(
        server
            .runtime_state()
            .db
            .session_get("browser")
            .unwrap()
            .is_none()
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn passwordless_browser_requires_approval_when_enabled_and_only_approval_issues_the_session()
{
    use tokio_tungstenite::tungstenite::Message;
    let (_directory, server, port) = passwordless_server().await;
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    for allowed in [false, true] {
        let client_id = if allowed {
            "approved-browser"
        } else {
            "rejected-browser"
        };
        let (mut socket, channel, session, _) = passwordless_browser(&state, port, client_id).await;
        let request = pending_event(&mut events).await;
        assert!(state.db.session_get(client_id).unwrap().is_none());
        assert_eq!(
            request["request"]["verificationCode"],
            channel.verification_code
        );
        assert_eq!(request["request"]["password"], "");
        server
            .native_management()
            .unwrap()
            .confirm_web_login(request["requestId"].as_str().unwrap().into(), allowed)
            .await
            .unwrap();
        closed_event(&mut events, request["requestId"].as_str().unwrap()).await;
        let response = login_frame(&mut socket).await;
        if !allowed {
            assert!(matches!(response, Message::Close(Some(close)) if close.reason == "rejected"));
            assert!(state.db.session_get(client_id).unwrap().is_none());
            continue;
        }
        let Message::Binary(response) = response else {
            panic!("encrypted completion required")
        };
        let response: Value =
            serde_json::from_slice(&channel.decrypt_response(&response, now_ms()).unwrap())
                .unwrap();
        assert_eq!(response["status"], "COMPLETED");
        assert_eq!(response["loginContext"], channel.context);
        assert!(response.get("token").is_none());
        let shared = session
            .compute_shared_key(&plain_rs::utils::base64::base64_decode(
                response["ecdhPublicKey"].as_str().unwrap(),
            ))
            .unwrap();
        assert_eq!(
            state.db.session_get(client_id).unwrap().unwrap().token,
            plain_rs::utils::base64::base64_encode(&shared)
        );
        let signing = crate::services::peers::peer_wire::signing_keypair(&state.prefs).unwrap();
        let signed = format!(
            "{}|COMPLETED|{}|{}|{}",
            response["clientId"].as_str().unwrap(),
            response["ecdhPublicKey"].as_str().unwrap(),
            response["timestamp"].as_u64().unwrap(),
            channel.context
        );
        assert!(plain_rs::crypto::ed25519_verify(
            &plain_rs::utils::base64::base64_encode(&signing[32..]),
            signed.as_bytes(),
            response["signature"].as_str().unwrap()
        ));
    }
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn disconnect_expiry_mode_changes_and_repeated_requests_cannot_create_a_passwordless_session()
{
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    for failure in ["disconnect", "expiry", "mode", "replay"] {
        let (_directory, server, port) = passwordless_server().await;
        let state = server.runtime_state();
        let mut events = state.events.subscribe();
        let (mut socket, _, _, frame) = passwordless_browser(&state, port, "browser").await;
        let event = pending_event(&mut events).await;
        let id = event["requestId"].as_str().unwrap();
        match failure {
            "disconnect" => {
                socket.close(None).await.unwrap();
                tokio::time::timeout(Duration::from_secs(3), async {
                    while state.main_ws.login_pending(id) {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
            }
            "expiry" => {
                state
                    .login_attempts
                    .lock()
                    .unwrap()
                    .pending
                    .get_mut(id)
                    .unwrap()
                    .at = Instant::now() - Duration::from_secs(301)
            }
            "mode" => {
                state.prefs.set("password_type", 1_i32).unwrap();
            }
            "replay" => {
                socket.send(Message::Binary(frame.into())).await.unwrap();
                assert!(matches!(login_frame(&mut socket).await, Message::Close(_)));
            }
            _ => unreachable!(),
        }
        assert!(
            server
                .native_management()
                .unwrap()
                .confirm_web_login(id.into(), true)
                .await
                .is_err(),
            "{failure}"
        );
        assert!(
            state.db.session_get("browser").unwrap().is_none(),
            "{failure}"
        );
        closed_event(&mut events, id).await;
        server.shutdown().await;
    }
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn successful_passwordless_logins_skip_disabled_approval_and_do_not_exhaust_attempts() {
    let (_directory, server, port) = passwordless_server().await;
    let state = server.runtime_state();
    state.prefs.set("login_require_approval", false).unwrap();
    admit_attempt(&state, "failed-client", "127.0.0.1", "failed-attempt").unwrap();
    for index in 0..LOGIN_ATTEMPT_LIMIT + 2 {
        let client_id = format!("successful-browser-{index}");
        let (mut socket, _, _, _) = passwordless_browser(&state, port, &client_id).await;
        assert!(state.db.session_get(&client_id).unwrap().is_some());
        assert!(state.login_attempts.lock().unwrap().pending.is_empty());
        let attempts = state.login_attempts.lock().unwrap();
        let remaining = attempts.entries.get("127.0.0.1").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0, "failed-attempt");
        drop(attempts);
        socket.close(None).await.unwrap();
    }
    server.shutdown().await;
}

#[tokio::test]
async fn completing_an_attempt_preserves_other_attempts_from_the_same_address() {
    let directory = tempfile::tempdir().unwrap();
    let server = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = server.runtime_state();
    admit_attempt(&state, "client", "fixture", "failure").unwrap();
    admit_attempt(&state, "client", "fixture", "success").unwrap();
    finish_attempt(&state, "success");
    let attempts = state.login_attempts.lock().unwrap();
    let remaining = attempts.entries.get("fixture").unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].0, "failure");
}
