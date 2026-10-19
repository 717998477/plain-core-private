use super::*;
use crate::assembly::server::ContentServer;
use plain_rs::{
    chat::enums::DeviceType,
    db::{
        DPeer,
        chat_store::{SaveMode, peers},
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

async fn fixture() -> (
    tempfile::TempDir,
    ContentServer,
    ServerState,
    DPeer,
    Identity,
) {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    let state = core.runtime_state();
    state.management.shutdown().await;
    let mut peer = DPeer::new(
        "peer",
        "Original",
        "127.0.0.1",
        Some(18443),
        DeviceType::Computer,
    );
    peer.token = plain_rs::utils::base64::base64_encode(&[7; 32]);
    peer.key = "pairing-key".into();
    peer.public_key = "signature-key".into();
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let identity = Identity {
        url: "wss://127.0.0.1:18443/?cid=fixture".into(),
        token: peer.token.clone(),
        ip: peer.ip.clone(),
        port: peer.port,
    };
    (directory, core, state, peer, identity)
}

#[tokio::test]
async fn observed_names_update_only_the_current_peer_and_identical_or_empty_names_do_not_write() {
    let (_directory, core, state, before, identity) = fixture().await;
    let (_cancel, cancelled) = watch::channel(false);
    let mut events = state.events.subscribe();
    state.db.with_conn(|connection| {
        let writes = connection.total_changes();
        assert!(
            apply(
                connection, &state, &before.id, &identity, &cancelled, "Original"
            )
            .unwrap()
        );
        assert!(apply(connection, &state, &before.id, &identity, &cancelled, "").unwrap());
        assert_eq!(connection.total_changes(), writes);
        assert!(
            apply(
                connection, &state, &before.id, &identity, &cancelled, "Renamed"
            )
            .unwrap()
        );
        assert_eq!(connection.total_changes(), writes + 1);
        assert!(
            apply(
                connection, &state, &before.id, &identity, &cancelled, "Renamed"
            )
            .unwrap()
        );
        assert_eq!(connection.total_changes(), writes + 1);
    });
    let after = peers::get(&state.db, &before.id).unwrap().unwrap();
    assert_eq!(after.name, "Renamed");
    assert_eq!(
        (
            after.key,
            after.token,
            after.public_key,
            after.ip,
            after.port,
            after.status,
            after.created_at
        ),
        (
            before.key,
            before.token,
            before.public_key,
            before.ip,
            before.port,
            before.status,
            before.created_at
        )
    );
    let event = events.try_recv().unwrap();
    assert_eq!(event.event_type, plain_rs::chat::events::WS_PEER_UPDATED);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&event.payload).unwrap(),
        json!({"id":"peer"})
    );
    assert!(events.try_recv().is_err());
    core.stop_core().await;
}

#[tokio::test]
async fn cancellation_credentials_and_address_changes_reject_stale_observations_without_writes() {
    let (_directory, core, state, peer, identity) = fixture().await;
    let (cancel, cancelled) = watch::channel(false);
    let mut events = state.events.subscribe();
    for field in ["token", "ip", "port", "cancel"] {
        let mut old = identity.clone();
        match field {
            "token" => old.token = plain_rs::utils::base64::base64_encode(&[8; 32]),
            "ip" => old.ip = "127.0.0.2".into(),
            "port" => old.port = Some(19443),
            _ => {
                cancel.send_replace(true);
            }
        }
        state.db.with_conn(|connection| {
            let before = connection.total_changes();
            assert!(!apply(connection, &state, &peer.id, &old, &cancelled, "Stale").unwrap());
            assert_eq!(connection.total_changes(), before);
        });
        cancel.send_replace(false);
    }
    assert_eq!(
        peers::get(&state.db, &peer.id).unwrap().unwrap().name,
        "Original"
    );
    assert!(events.try_recv().is_err());
    core.stop_core().await;
    state.db.with_conn(|connection| {
        assert!(
            !apply(
                connection, &state, &peer.id, &identity, &cancelled, "Stopped"
            )
            .unwrap()
        )
    });
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn encrypted_app_observations_work_without_notification_support_and_cannot_outlive_credentials()
 {
    let remote_dir = tempfile::tempdir().unwrap();
    let remote = ContentServer::open_with_cache(
        &remote_dir.path().join("data"),
        &remote_dir.path().join("cache"),
    )
    .unwrap();
    let remote_state = remote.runtime_state();
    remote_state
        .prefs
        .set_user(
            plain_rs::prefs::user::keys::DEVICE_NAME,
            "Authenticated name",
        )
        .unwrap();
    let session = remote
        .native_management()
        .unwrap()
        .create_session("Fixture metadata".into())
        .unwrap();
    remote_state.prefs.set_user("http_port", 0).unwrap();
    remote_state.prefs.set_user("https_port", 0).unwrap();
    let (_, port) = remote.start_configured_public().await.unwrap();
    let (generation, mut requests) = remote_state.host.connect();
    let hold = Arc::new(AtomicBool::new(true));
    let requested = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (sdk_hold, sdk_requested, sdk_release, host) = (
        hold.clone(),
        requested.clone(),
        release.clone(),
        remote_state.host.clone(),
    );
    let responder = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let value = match request["method"].as_str().unwrap() {
                "systemCapabilities" => json!([]),
                "systemPermissionFacts" => {
                    if sdk_hold.swap(false, Ordering::SeqCst) {
                        sdk_requested.notify_one();
                        sdk_release.notified().await;
                    }
                    let granted: serde_json::Map<String, serde_json::Value> =
                        request["params"]["permissions"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|name| (name.as_str().unwrap().to_owned(), json!(false)))
                            .collect();
                    json!({"granted":granted})
                }
                method => panic!("Unexpected metadata primitive {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":value}))
                .unwrap();
        }
    });
    let (_directory, core, state, mut peer, mut identity) = fixture().await;
    state.prefs.set("client_id", &session.client_id).unwrap();
    peer.token = session.token;
    peer.port = Some(port);
    identity.token = peer.token.clone();
    identity.port = peer.port;
    identity.url = format!("wss://127.0.0.1:{port}/?cid={}", session.client_id);
    state.db.with_conn(|connection| {
        connection
            .execute(
                "UPDATE peers SET token=?1,port=?2 WHERE id=?3",
                rusqlite::params![peer.token, port, peer.id],
            )
            .unwrap()
    });
    let (_cancel, cancelled) = watch::channel(false);
    let mut events = state.events.subscribe();
    let pending = refresh(&state, &peer.id, &identity, &cancelled);
    tokio::pin!(pending);
    tokio::select! {
        _=&mut pending=>panic!("Observation must wait for permission sample"),
        ready=tokio::time::timeout(Duration::from_secs(10),requested.notified())=>ready.unwrap(),
    }
    state.db.with_conn(|connection| {
        connection
            .execute("UPDATE peers SET token='' WHERE id=?1", [&peer.id])
            .unwrap()
    });
    release.notify_one();
    assert!(pending.await.unwrap().is_none());
    assert_eq!(
        peers::get(&state.db, &peer.id).unwrap().unwrap().name,
        "Original"
    );
    assert!(events.try_recv().is_err());
    state.db.with_conn(|connection| {
        connection
            .execute(
                "UPDATE peers SET token=?1 WHERE id=?2",
                [&peer.token, &peer.id],
            )
            .unwrap()
    });
    let snapshot = refresh(&state, &peer.id, &identity, &cancelled)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.device_name, "Authenticated name");
    assert!(
        !snapshot
            .capabilities
            .iter()
            .any(|value| value == "NOTIFICATIONS")
    );
    assert_eq!(
        peers::get(&state.db, &peer.id).unwrap().unwrap().name,
        "Authenticated name"
    );
    assert_eq!(
        events.try_recv().unwrap().event_type,
        plain_rs::chat::events::WS_PEER_UPDATED
    );
    refresh(&state, &peer.id, &identity, &cancelled)
        .await
        .unwrap()
        .unwrap();
    assert!(events.try_recv().is_err());
    remote_state.host.disconnect(generation);
    responder.abort();
    core.stop_core().await;
    remote.stop_core().await;
}
