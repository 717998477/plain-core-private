use super::*;

#[tokio::test(start_paused = true)]
async fn receiver_starts_without_http_and_retries_when_platform_permissions_change() {
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    let _runtime = TEST_LOCK.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "fixture-resident-receiver").unwrap();
    prefs
        .set("mdns_hostname", "fixture-resident-receiver.local")
        .unwrap();
    let server = crate::assembly::server::ContentServer::start(
        directory.path(),
        &plain_rs::utils::base64::base64_encode(&[8; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let (generation, mut requests) = crate::test_support::HostRequests::connect(&state.host);
    let supported = Arc::new(AtomicBool::new(false));
    let granted = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let host = state.host.clone();
    let capabilities = supported.clone();
    let permission = granted.clone();
    let sampled = calls.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemCapabilities" => {
                    sampled.fetch_add(1, Ordering::SeqCst);
                    if capabilities.load(Ordering::SeqCst) {
                        json!(["PEER_DISCOVERY"])
                    } else {
                        json!([])
                    }
                }
                "mdnsMulticast" => json!(
                    request["params"]["acquire"] == false || permission.load(Ordering::SeqCst)
                ),
                method => panic!("unexpected host method {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    Runtime::host_connected(state.clone(), generation.wrapping_sub(1)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Runtime::host_connected(state.clone(), generation).await;
    assert_eq!(state.mdns.snapshot()["receiver"], false);
    supported.store(true, Ordering::SeqCst);
    Runtime::host_connected(state.clone(), generation).await;
    assert_eq!(state.mdns.snapshot()["receiver"], false);
    granted.store(true, Ordering::SeqCst);
    let mut events = state.events.subscribe();
    state.mdns.permissions_changed(state.clone());
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(800)).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while events.recv().await.unwrap().event_type != WS_MDNS_UPDATED {}
    })
    .await
    .unwrap();
    let snapshot = state.mdns.snapshot();
    assert_eq!(snapshot["receiver"], true);
    assert_eq!(snapshot["published"], false);
    assert_eq!(snapshot["scanning"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    state.mdns.permissions_changed(state.clone());
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    server.shutdown().await;
    assert!(OWNER.lock().await.is_none());
    responder.abort();
}

#[tokio::test]
async fn authenticated_runtime_preserves_radio_modes_and_releases_global_owner() {
    let _runtime = crate::services::discovery::mdns_runtime::TEST_LOCK
        .lock()
        .await;
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "fixture-mdns-root").unwrap();
    prefs
        .set("mdns_hostname", "fixture-mdns-root.local")
        .unwrap();
    prefs.set("https_port", 2443).unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[5; 32]);
    let server =
        crate::assembly::server::ContentServer::start(dir.path(), &token, prefs.clone()).unwrap();
    let state = server.runtime_state();
    // The advertised port is the live public listener's, so the test has to
    // start one — announcing port 0 is refused by contract.
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let (generation, mut rx) = crate::test_support::HostRequests::connect(&state.host);
    let allowed = Arc::new(AtomicBool::new(false));
    let permission = allowed.clone();
    let acquired = Arc::new(AtomicBool::new(false));
    let lease = acquired.clone();
    let host = state.host.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemCapabilities" => json!(["PEER_DISCOVERY"]),
                "mdnsMulticast" => {
                    let acquire = request["params"]["acquire"].as_bool().unwrap();
                    let ok = !acquire || permission.load(Ordering::SeqCst);
                    if ok {
                        lease.store(acquire, Ordering::SeqCst);
                    }
                    json!(ok)
                }
                method => panic!("Unexpected host method: {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    allowed.store(true, Ordering::SeqCst);
    state
        .nearby_devices
        .scanning_modes(None, Some(true), vec![])
        .unwrap();
    let start: Value = execute(&state, Request::Start {}).await;
    assert_eq!(start["receiver"], true);
    assert_eq!(start["scanning"], true);
    assert!(acquired.load(Ordering::SeqCst));
    allowed.store(false, Ordering::SeqCst);
    let stop: Value = execute(&state, Request::Stop {}).await;
    assert_eq!(stop["scanning"], false);
    assert!(
        state.nearby_devices.active(),
        "Stopping LAN must preserve active BLE"
    );
    state
        .nearby_devices
        .scanning_modes(None, Some(false), vec![])
        .unwrap();
    assert!(!state.nearby_devices.active());
    allowed.store(true, Ordering::SeqCst);
    let published: Value = execute(&state, Request::Publish {}).await;
    assert_eq!(published["published"], true);
    prefs.set("mdns_hostname", "fixture-new.local").unwrap();
    let updated: Value = execute(&state, Request::Update {}).await;
    assert_eq!(updated["published"], true);
    assert_eq!(
        *state
            .mdns
            .session
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .hostname
            .read()
            .unwrap(),
        "fixture-new.local"
    );
    allowed.store(false, Ordering::SeqCst);
    let withdrawn: Value = execute(&state, Request::Unpublish {}).await;
    assert_eq!(withdrawn["published"], false);
    let updated: Value = execute(&state, Request::Update {}).await;
    assert_eq!(
        updated["published"], false,
        "Update cannot republish an inactive service"
    );
    allowed.store(true, Ordering::SeqCst);
    let debug_a = uuid::Uuid::new_v4();
    let debug_b = uuid::Uuid::new_v4();
    let token_string = token.clone();
    let debug = crate::native::management::NativeManagement::new(state.clone()).acquire_mdns_debug_session(debug_a.to_string()).await.unwrap();
    assert_eq!(debug["scanning"], true);
    assert_eq!(debug["capturing"], true);
    let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    let bytes = plain_rs::protocols::mdns::packet_codec::build_ptr_query("_plainapp._tcp.local");
    plain_rs::protocols::mdns::host_responder::send_recorded(
        &sender,
        &bytes,
        receiver.local_addr().unwrap(),
    )
    .unwrap();
    let mut buffer = [0; 1500];
    let (size, src) = receiver.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], bytes.as_slice());
    plain_rs::protocols::mdns::host_responder::record_received(&receiver, src, &buffer[..size]);
    let captured = state.mdns.snapshot();
    assert!(
        captured["packetsIn"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["srcPort"] == sender.local_addr().unwrap().port()
                && p["summary"].as_str().unwrap().contains("PTR"))
    );
    assert!(captured["packetsOut"].as_array().unwrap().iter().any(|p| {
        p["dstPort"] == receiver.local_addr().unwrap().port()
            && p["detail"]
                .as_str()
                .unwrap()
                .contains("_plainapp._tcp.local")
    }));
    crate::native::management::NativeManagement::new(state.clone()).acquire_mdns_debug_session(debug_b.to_string()).await.unwrap();
    execute(&state, Request::Stop {}).await;
    assert_eq!(
        state.mdns.snapshot()["scanning"],
        true,
        "Debug leases must keep scanning independently of manual stop"
    );
    crate::native::management::NativeManagement::new(state.clone()).release_mdns_debug_session(debug_a.to_string()).await.unwrap();
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    crate::native::management::NativeManagement::new(state.clone()).release_mdns_debug_session(debug_b.to_string()).await.unwrap();
    assert_eq!(state.mdns.snapshot()["scanning"], false);
    assert_eq!(state.mdns.snapshot()["capturing"], false);
    assert_eq!(state.mdns.snapshot()["packetsIn"], json!([]));
    assert_eq!(state.mdns.snapshot()["packetsOut"], json!([]));
    execute(&state, Request::Start {}).await;
    crate::native::management::NativeManagement::new(state.clone()).acquire_mdns_debug_session(debug_a.to_string()).await.unwrap();
    crate::native::management::NativeManagement::new(state.clone()).release_mdns_debug_session(debug_a.to_string()).await.unwrap();
    assert_eq!(
        state.mdns.snapshot()["scanning"],
        true,
        "Debug page disposal must preserve manual discovery"
    );
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    execute(
        &state,
        Request::Acquire {
            token: first,
            owner: "remote:first".into(),
        },
    )
    .await;
    execute(
        &state,
        Request::Acquire {
            token: second,
            owner: "remote:second".into(),
        },
    )
    .await;
    execute(&state, Request::Stop {}).await;
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    let subscription = state.subscription_sessions.open(
        state.clone(),
        crate::graphql::executor::RequestContext::authenticated_remote(state.clone(), "first".into()),
        "first".into(),
        Some(vec![1; 32]),
        crate::native::graphql::NativeRequestScope::default(),
    ).unwrap();
    subscription.mark_online().unwrap();
    state
        .mdns
        .release_discovery_client(&state, "first")
        .await
        .unwrap();
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    crate::native::management::NativeManagement::new(state.clone()).acquire_mdns_debug_session(debug_a.to_string()).await.unwrap();
    execute(
        &state,
        Request::Release {
            token: second,
            owner: "remote:second".into(),
        },
    )
    .await;
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    crate::native::management::NativeManagement::new(state.clone()).release_mdns_debug_session(debug_a.to_string()).await.unwrap();
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    state.subscription_sessions.close(subscription.id);
    state.mdns.release_discovery_client(&state, "first").await.unwrap();
    assert_eq!(state.mdns.snapshot()["scanning"], false);
    state.host.disconnect(generation);
    responder.abort();
    acquired.store(false, Ordering::SeqCst);
    Runtime::host_connected(state.clone(), generation).await;
    assert!(
        !acquired.load(Ordering::SeqCst),
        "An old host generation must not reacquire permission"
    );
    let (generation, mut rx) = crate::test_support::HostRequests::connect(&state.host);
    let host = state.host.clone();
    let lease = acquired.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            if request["method"] == "systemCapabilities" {
                host.reply(
                    generation,
                    json!({"id":request["id"],"result":["PEER_DISCOVERY"]}),
                )
                .unwrap();
                continue;
            }
            assert_eq!(request["method"], "mdnsMulticast");
            lease.store(
                request["params"]["acquire"].as_bool().unwrap(),
                Ordering::SeqCst,
            );
            host.reply(generation, json!({"id":request["id"],"result":true}))
                .unwrap();
        }
    });
    Runtime::host_connected(state.clone(), generation).await;
    assert!(
        acquired.load(Ordering::SeqCst),
        "The current host must restore the actual multicast lease"
    );
    let other = Arc::new(Runtime::default());
    assert!(other.execute(&state, Request::Receiver {}).await.is_err());
    server.shutdown().await;
    assert!(!acquired.load(Ordering::SeqCst));
    assert_eq!(state.mdns.snapshot()["receiver"], false);
    assert!(OWNER.lock().await.is_none());
    assert!(!host_responder::is_running());
    state.host.disconnect(generation);
    responder.abort();
}

async fn execute(state: &ServerState, request: Request) -> Value {
    control(state, request)
        .await
        .unwrap_or_else(|error| panic!("mDNS request failed: {error}"))
}

#[tokio::test(start_paused = true)]
async fn network_change_burst_restarts_once_and_accepts_the_next_burst() {
    use std::sync::atomic::AtomicUsize;
    let runtime = Runtime::default();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let calls = Arc::new(AtomicUsize::new(0));
    let restart = || {
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
        }
    };
    assert!(runtime.schedule_network_restart(stopped.clone(), restart()));
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(799)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.schedule_network_restart(stopped.clone(), restart()));
    tokio::time::advance(std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(runtime.schedule_network_restart(stopped, restart()));
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(800)).await;
    tokio::task::yield_now().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn network_restart_cancels_on_runtime_stop_and_explicit_close() {
    use std::sync::atomic::AtomicUsize;
    for close in [false, true] {
        let runtime = Runtime::default();
        let (stop, stopped) = tokio::sync::watch::channel(false);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        assert!(
            runtime.schedule_network_restart(stopped.clone(), async move {
                count.fetch_add(1, Ordering::SeqCst);
            })
        );
        tokio::task::yield_now().await;
        if close {
            runtime.cancel_network_restart();
        } else {
            stop.send_replace(true);
        }
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        stop.send_replace(true);
        assert!(!runtime.schedule_network_restart(stopped, async { panic!("stopped runtime") }));
    }
}

#[tokio::test(start_paused = true)]
async fn stopping_a_network_restart_cancels_its_in_progress_work() {
    use std::sync::atomic::AtomicUsize;
    let runtime = Runtime::default();
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let (started, running) = tokio::sync::oneshot::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    assert!(runtime.schedule_network_restart(stopped, async move {
        started.send(()).unwrap();
        std::future::pending::<()>().await;
        count.fetch_add(1, Ordering::SeqCst);
    }));
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(800)).await;
    running.await.unwrap();
    stop.send_replace(true);
    tokio::task::yield_now().await;
    assert!(
        runtime
            .network_restart
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .is_finished()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn file_sharing_capability_does_not_authorize_peer_discovery_or_acquire_multicast() {
    let directory = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[17; 32]);
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let server =
        crate::assembly::server::ContentServer::start(directory.path(), &token, prefs).unwrap();
    let state = server.runtime_state();
    let (generation, mut incoming) = crate::test_support::HostRequests::connect(&state.host);
    let worker = tokio::spawn({
        let state = state.clone();
        async move { control(&state, Request::Publish {}).await }
    });
    let request = incoming.recv().await.unwrap();
    assert_eq!(request["method"], "systemCapabilities");
    state
        .host
        .reply(
            generation,
            json!({"id":request["id"],"result":["LAN_SHARE"]}),
        )
        .unwrap();
    assert!(
        worker
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("PEER_DISCOVERY")
    );
    assert!(incoming.try_recv().is_err());
    assert_eq!(state.mdns.snapshot()["receiver"], false);
    state.host.disconnect(generation);
    server.shutdown().await;
}

#[tokio::test]
async fn inactive_runtime_cleanup_and_updates_do_not_request_platform_capabilities() {
    use futures_util::FutureExt;
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let (generation, mut requests) = crate::test_support::HostRequests::connect(&state.host);
    for request in [
        Request::Update {},
        Request::Restart {},
        Request::Stop {},
        Request::Unpublish {},
    ] {
        state
            .mdns
            .execute(&state, request)
            .now_or_never()
            .expect("Inactive runtime commands must complete without a host request")
            .unwrap();
    }
    assert!(requests.try_recv().is_err());
    state.host.disconnect(generation);
    core.stop_core().await;
}
