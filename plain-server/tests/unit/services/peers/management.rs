use serde_json::json;
use super::super::clipboard::receive as synchronize_clipboard;
use super::*;
use plain_rs::{chat::enums::DeviceType, db::chat_store::SaveMode};

fn peer() -> DPeer {
    let mut peer = DPeer::new("peer", "Fixture", "::1", Some(18443), DeviceType::Computer);
    peer.token = plain_rs::utils::base64::base64_encode(&[7; 32]);
    peer
}

async fn fixture() -> (
    tempfile::TempDir,
    crate::assembly::server::ContentServer,
    ServerState,
) {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    let state = core.runtime_state();
    state.management.shutdown().await;
    (directory, core, state)
}

#[test]
fn connection_identity_uses_the_canonical_tls_address_and_encodes_the_local_client_id() {
    let peer = peer();
    let actor = "actor&auth=1 /?";
    let result = identity(&peer, actor, "https://[::1]:18443").unwrap();
    let url = reqwest::Url::parse(&result.url).unwrap();
    assert_eq!(url.scheme(), "wss");
    assert_eq!(url.host_str(), Some("[::1]"));
    assert_eq!(url.port(), Some(18443));
    assert_eq!(url.path(), "/");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        vec![("cid".into(), actor.into())]
    );
    assert!(!result.url.contains(&peer.token));
    let mut renamed = peer.clone();
    renamed.name = "Renamed".into();
    assert!(identity(&renamed, actor, "https://[::1]:18443").unwrap() == result);
}

#[test]
fn chat_only_self_invalid_token_and_missing_address_records_do_not_open_management_sessions() {
    let mut peer = peer();
    assert!(identity(&peer, "peer", "https://[::1]:18443").is_none());
    assert!(identity(&peer, "", "https://[::1]:18443").is_none());
    assert!(identity(&peer, "actor", "").is_none());
    assert!(identity(&peer, "actor", "http://[::1]:8080").is_none());
    peer.port = None;
    assert!(identity(&peer, "actor", "https://[::1]:18443").is_none());
    peer.port = Some(18443);
    for token in [
        String::new(),
        "invalid".into(),
        plain_rs::utils::base64::base64_encode(&[7; 31]),
    ] {
        peer.token = token;
        assert!(identity(&peer, "actor", "https://[::1]:18443").is_none());
    }
}

#[tokio::test]
async fn reconciliation_reuses_unchanged_workers_and_retires_replaced_and_removed_workers() {
    let (_directory, core, state) = fixture().await;
    let identity = identity(&peer(), "actor", "https://[::1]:18443").unwrap();
    let (cancel, cancelled) = watch::channel(false);
    let job = tokio::spawn(std::future::pending::<()>());
    let id = job.id();
    let mut workers = HashMap::from([(
        "peer".into(),
        Worker {
            identity: identity.clone(),
            cancel,
            job,
        },
    )]);
    reconcile(
        &state,
        &mut workers,
        HashMap::from([("peer".into(), identity.clone())]),
    )
    .await;
    assert_eq!(workers["peer"].job.id(), id);
    assert!(!*cancelled.borrow());
    core.stop_core().await;
    let mut replacement = identity;
    replacement.token = plain_rs::utils::base64::base64_encode(&[8; 32]);
    reconcile(
        &state,
        &mut workers,
        HashMap::from([("peer".into(), replacement)]),
    )
    .await;
    assert_ne!(workers["peer"].job.id(), id);
    assert!(*cancelled.borrow());
    let removed = workers["peer"].cancel.subscribe();
    reconcile(&state, &mut workers, HashMap::new()).await;
    assert!(workers.is_empty());
    assert!(*removed.borrow());
}

#[tokio::test]
async fn relay_rechecks_credentials_and_addresses_under_the_database_lock() {
    let (_directory, core, state) = fixture().await;
    let mut peer = peer();
    peer.port = None;
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let expected = Identity {
        url: "wss://[::1]/?cid=actor".into(),
        token: peer.token.clone(),
        ip: peer.ip.clone(),
        port: None,
    };
    let mut events = state.events.subscribe();
    assert!(
        publish_clipboard_change(
            &state,
            "peer",
            &expected,
            &watch::channel(false).1
        )
        .await
        .unwrap()
    );
    let relayed = std::iter::from_fn(|| events.try_recv().ok())
        .find(|event| event.event_type == plain_rs::events::PEER_CLIPBOARD_CHANGED)
        .unwrap();
    assert_eq!(relayed.event_type, plain_rs::events::PEER_CLIPBOARD_CHANGED);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&relayed.payload).unwrap(),
        json!({"peerId":"peer"})
    );
    assert!(relayed.into_public().is_none());
    state.db.with_conn(|connection| {
        connection
            .execute("UPDATE peers SET token='' WHERE id='peer'", [])
            .unwrap()
    });
    assert!(
        !publish_clipboard_change(
            &state,
            "peer",
            &expected,
            &watch::channel(false).1
        )
        .await
        .unwrap()
    );
    state.db.with_conn(|connection| {
        connection
            .execute(
                "UPDATE peers SET token=?1,ip='127.0.0.1' WHERE id='peer'",
                [&peer.token],
            )
            .unwrap()
    });
    assert!(
        !publish_clipboard_change(
            &state,
            "peer",
            &expected,
            &watch::channel(false).1
        )
        .await
        .unwrap()
    );
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.event_type, plain_rs::events::PEER_CLIPBOARD_CHANGED);
    }
    core.stop_core().await;
}

#[tokio::test]
async fn a_stopped_core_cannot_relay_an_event_even_when_credentials_still_match() {
    let (_directory, core, state) = fixture().await;
    let mut peer = peer();
    peer.port = None;
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let expected = Identity {
        url: "wss://[::1]/".into(),
        token: peer.token,
        ip: peer.ip,
        port: None,
    };
    core.stop_core().await;
    assert!(
        !publish_clipboard_change(
            &state,
            "peer",
            &expected,
            &watch::channel(false).1
        )
        .await
        .unwrap()
    );
}

#[test]
fn reconnect_backoff_has_the_existing_one_to_thirty_second_bounds() {
    let mut delay = Duration::from_secs(1);
    for seconds in [2, 4, 8, 16, 30, 30] {
        delay = next_delay(delay);
        assert_eq!(delay, Duration::from_secs(seconds));
    }
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn actual_management_socket_authenticates_and_relays_only_decrypted_native_events() {
    let remote_directory = tempfile::tempdir().unwrap();
    let remote = crate::assembly::server::ContentServer::open_with_cache(
        &remote_directory.path().join("data"),
        &remote_directory.path().join("cache"),
    )
    .unwrap();
    let remote_state = remote.runtime_state();
    remote_state.prefs.set("api_permissions", vec!["CLIPBOARD"]).unwrap();
    let remote_host = remote_state.host.clone();
    let (generation, mut remote_requests) = remote_host.connect();
    let (ready, mut ready_rx) = tokio::sync::mpsc::channel(4);
    let responder = tokio::spawn(async move {
        while let Some(request) = remote_requests.recv().await {
            let method = request["method"].as_str().unwrap_or_default();
            let result = match method {
                "systemCapabilities" => json!(["CLIPBOARD"]),
                "systemPermissionFacts" => {
                    let granted = request["params"]["permissions"].as_array().unwrap().iter()
                        .filter_map(|name| name.as_str()).map(|name| (name.to_owned(), json!(true))).collect::<serde_json::Map<_, _>>();
                    if granted.contains_key("CLIPBOARD") { let _ = ready.try_send(()); }
                    json!({"granted":granted})
                },
                _ => json!({}),
            };
            let _ = remote_host.reply(generation, json!({"id":request["id"],"result":result}));
        }
    });
    let session = remote
        .native_management()
        .unwrap()
        .create_session("Fixture manager".into())
        .unwrap();
    remote_state.prefs.set("http_port", 0).unwrap();
    remote_state.prefs.set("https_port", 0).unwrap();
    let (_, tls_port) = remote.start_configured_public().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let prefs = plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(&data)).unwrap();
    prefs
        .set(plain_rs::prefs::pref_keys::CLIENT_ID, &session.client_id)
        .unwrap();
    let manager = crate::assembly::server::ContentServer::open_with_cache(
        &data,
        &directory.path().join("cache"),
    )
    .unwrap();
    let state = manager.runtime_state();
    let (host_generation, mut requests) = state.host.connect();
    let mut local_events = state.events.subscribe();
    let mut peer = DPeer::new(
        &remote_state.native_client_id().unwrap(),
        "Fixture server",
        "127.0.0.1",
        Some(tls_port),
        DeviceType::Computer,
    );
    peer.token = session.token;
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    tokio::time::timeout(Duration::from_secs(10), ready_rx.recv()).await.unwrap().unwrap();
    let payload = json!({"text":"fixture clipboard hint", "sensitive":false, "time":1});
    let relayed = tokio::time::timeout(Duration::from_secs(10), async {
        let mut ticks = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                _ = ticks.tick() => {
                    remote_state.events.send(WsEvent::broadcast("CLIPBOARD_CHANGED", payload.to_string())).unwrap();
                }
                event = local_events.recv() => {
                    let event = event.unwrap();
                    if event.event_type == plain_rs::events::PEER_CLIPBOARD_CHANGED { break event; }
                }
            }
        }
    }).await.unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&relayed.payload).unwrap(),
        json!({"peerId":peer.id})
    );
    assert!(relayed.into_public().is_none());
    state.host.disconnect(host_generation);
    manager.stop_core().await;
    responder.abort();
    remote.stop_core().await;
}

#[tokio::test]
async fn peer_snapshot_waits_for_the_database_without_blocking_the_async_thread() {
    let (_directory, core, state) = fixture().await;
    let (locked, ready) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let db = state.db.clone();
    let holder = std::thread::spawn(move || {
        db.with_conn(|_| {
            locked.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(3));
        });
    });
    ready.await.unwrap();
    let mut pending = Box::pin(desired(&state, "actor"));
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(pending.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    release.send(()).unwrap();
    assert!(pending.await.unwrap().1.is_empty());
    holder.join().unwrap();
    core.stop_core().await;
}

#[tokio::test]
async fn a_cancelled_worker_cannot_publish_even_while_credentials_still_match() {
    let (_directory, core, state) = fixture().await;
    let peer = peer();
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let expected = identity(&peer, "actor", "https://[::1]:18443").unwrap();
    let mut events = state.events.subscribe();
    let (_cancel, cancelled) = watch::channel(true);
    assert!(
        !publish_clipboard_change(
            &state,
            "peer",
            &expected,
            &cancelled
        )
        .await
        .unwrap()
    );
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.event_type, plain_rs::events::PEER_CLIPBOARD_CHANGED);
    }
    core.stop_core().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn encrypted_clipboard_sync_seeds_baseline_retries_failed_writes_and_rechecks_grants() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    fn seed(db: &plain_rs::db::Db, text: &str, sensitive: bool, sequence: u8) {
        let (item, _) = plain_rs::clipboard::record(db, text, "", "", sensitive).unwrap();
        let time = format!("2026-01-01T00:00:{sequence:02}.000Z");
        db.with_conn(|connection| {
            connection
                .execute(
                    "UPDATE clipboards SET created_at=?1 WHERE id=?2",
                    [&time, &item.id],
                )
                .unwrap()
        });
    }
    let remote_directory = tempfile::tempdir().unwrap();
    let remote = crate::assembly::server::ContentServer::open_with_cache(
        &remote_directory.path().join("data"),
        &remote_directory.path().join("cache"),
    )
    .unwrap();
    let remote_state = remote.runtime_state();
    remote_state
        .prefs
        .set("api_permissions", json!(["CLIPBOARD"]))
        .unwrap();
    let session = remote
        .native_management()
        .unwrap()
        .create_session("Fixture sync".into())
        .unwrap();
    remote_state.prefs.set("http_port", 0).unwrap();
    remote_state.prefs.set("https_port", 0).unwrap();
    let (_, port) = remote.start_configured_public().await.unwrap();
    let (_directory, manager, state) = fixture().await;
    state.prefs.set("client_id", &session.client_id).unwrap();
    let mut peer = DPeer::new(
        "remote-clipboard",
        "Fixture",
        "127.0.0.1",
        Some(port),
        DeviceType::Computer,
    );
    peer.token = session.token;
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let expected = identity(
        &peer,
        &session.client_id,
        &format!("https://127.0.0.1:{port}"),
    )
    .unwrap();
    let (cancel, cancelled) = watch::channel(false);
    let (generation, mut requests) = state.host.connect();
    let writes = Arc::new(AtomicUsize::new(0));
    let succeeds = Arc::new(AtomicBool::new(false));
    let supports = Arc::new(AtomicBool::new(true));
    let (count, success, support, host) = (
        writes.clone(),
        succeeds.clone(),
        supports.clone(),
        state.host.clone(),
    );
    let responder = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemCapabilities" => {
                    if support.load(Ordering::SeqCst) {
                        json!(["CLIPBOARD"])
                    } else {
                        json!([])
                    }
                }
                "systemSetClipboard" => {
                    count.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(request["params"]["text"], "new clipboard");
                    json!(success.load(Ordering::SeqCst))
                }
                method => panic!("Unexpected host primitive {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    seed(&remote_state.db, "old clipboard", false, 1);
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    seed(&remote_state.db, "new clipboard", false, 2);
    assert!(
        synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
            .await
            .is_err()
    );
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    succeeds.store(true, Ordering::SeqCst);
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 2);
    let history = state.db.clipboard_page("", 10, 0).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].text, "new clipboard");
    assert_eq!(history[0].source, peer.id);
    state
        .prefs
        .set("api_permissions", json!(["CLIPBOARD"]))
        .unwrap();
    let mut echoes = state.events.subscribe();
    crate::services::clipboard::report(
        &state,
        &json!({"text":"new clipboard","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(echoes.try_recv().is_err());
    seed(&remote_state.db, "private clipboard", true, 3);
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 2);
    seed(&remote_state.db, "must not apply", false, 4);
    supports.store(false, Ordering::SeqCst);
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    supports.store(true, Ordering::SeqCst);
    state
        .prefs
        .set("clipboard_directions", json!({peer.id.clone():"off"}))
        .unwrap();
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    state.prefs.set("clipboard_directions", json!({})).unwrap();
    cancel.send_replace(true);
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    cancel.send_replace(false);
    state.db.with_conn(|connection| {
        connection
            .execute("UPDATE peers SET token='' WHERE id=?1", [&peer.id])
            .unwrap()
    });
    synchronize_clipboard(&state, &peer.id, &expected, &cancelled)
        .await
        .unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 2);
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 1);
    let (remote_generation, mut remote_requests) = remote_state.host.connect();
    let (remote_host, remote_db) = (remote_state.host.clone(), remote_state.db.clone());
    let expected_source = session.client_id.clone();
    let remote_responder = tokio::spawn(async move {
        while let Some(request) = remote_requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemCapabilities" => json!(["CLIPBOARD"]),
                "systemSetClipboard" => {
                    assert_eq!(request["params"]["text"], "encrypted fixture write");
                    let item = remote_db
                        .clipboard_page("encrypted fixture write", 1, 0)
                        .unwrap();
                    assert_eq!(item[0].source, expected_source);
                    json!(true)
                }
                method => panic!("Unexpected remote host primitive {method}"),
            };
            remote_host
                .reply(
                    remote_generation,
                    json!({"id":request["id"],"result":result}),
                )
                .unwrap();
        }
    });
    let result = super::super::management_http::query(
        &state,
        &reqwest::Url::parse(&format!("https://127.0.0.1:{port}")).unwrap(),
        &session.client_id,
        &plain_rs::utils::base64::base64_decode(&peer.token),
        json!({"query":"mutation($text:String!){setClipboard(text:$text)}", "variables":{"text":"encrypted fixture write"}}),
        4 * 1024 * 1024,
    ).await.unwrap();
    assert_eq!(result["setClipboard"], true);
    let mut remote_echoes = remote_state.events.subscribe();
    crate::services::clipboard::report(
        &remote_state,
        &json!({"text":"encrypted fixture write","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(remote_echoes.try_recv().is_err());
    remote_state.host.disconnect(remote_generation);
    remote_responder.abort();
    state.host.disconnect(generation);
    responder.abort();
    manager.stop_core().await;
    remote.stop_core().await;
}
