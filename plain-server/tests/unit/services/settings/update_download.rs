use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

fn setup() -> (tempfile::TempDir, ContentServer, ServerState, Identity) {
    let dir = tempfile::tempdir().unwrap();
    let core = ContentServer::open_with_cache(&dir.path().join("data"), &dir.path().join("cache"))
        .unwrap();
    let state = core.runtime_state();
    updates::update(
        &state,
        json!({"newVersion":"999.0.0","downloadUrl":"https://example.invalid/update","size":4}),
    )
    .unwrap();
    let identity = Identity::from(
        &crate::services::settings::updates::info(&state).unwrap(),
    );
    (dir, core, state, identity)
}

fn allow_updates(state: &ServerState) {
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "systemCapabilities");
            host.reply(
                generation,
                json!({"id":request["id"],"result":["APP_UPDATES"]}),
            )
            .unwrap();
        }
    });
}

#[test]
fn progress_is_bounded_without_overflow_and_unknown_lengths_do_not_invent_progress() {
    assert_eq!(percentage(u64::MAX, Some(u64::MAX)), 99);
    assert_eq!(percentage(u64::MAX, Some(1)), 99);
    assert_eq!(percentage(50, Some(100)), 50);
    assert_eq!(percentage(50, Some(0)), 0);
    assert_eq!(percentage(50, None), 0);
    for raw in [
        "http://example.invalid/update",
        "file:///tmp/update",
        "https://user:secret@localhost/update",
    ] {
        assert!(validate_url(raw).is_err());
    }
}

#[tokio::test]
async fn complete_stream_is_published_with_identity_and_restored_without_client_file_checks() {
    let (_dir, core, state, identity) = setup();
    allow_updates(&state);
    let (_cancel, mut receiver) = watch::channel(false);
    let mut events = state.events.subscribe();
    let stream = futures_util::stream::iter([Ok(&b"a"[..]), Ok(&b"b"[..]), Ok(&b"cd"[..])]);
    let path = save_stream(
        &state,
        "complete",
        &identity,
        stream,
        Some(4),
        &mut receiver,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"abcd");
    assert_eq!(
        Path::new(&path).parent().unwrap().parent().unwrap(),
        state.cache.app_updates()
    );
    let info = crate::services::settings::updates::info(&state).unwrap();
    assert_eq!(restored(&state, &info).unwrap(), Some(path.clone()));
    let mut progress = Vec::new();
    while let Ok(event) = events.try_recv() {
        if event.event_type == "APP_UPDATE_DOWNLOAD_UPDATED" {
            progress.push(
                serde_json::from_str::<serde_json::Value>(&event.payload).unwrap()["progress"]
                    .as_i64()
                    .unwrap(),
            );
        }
    }
    assert_eq!(progress, vec![25, 50, 99]);
    core.stop_core().await;
    let core =
        ContentServer::open_with_cache(&_dir.path().join("data"), &_dir.path().join("cache"))
            .unwrap();
    let state = core.runtime_state();
    let bridge = Arc::new(core.host_events());
    let (reported, mut reports) = tokio::sync::mpsc::unbounded_channel();
    let serving = tokio::spawn(bridge.serve(
        |method, _| async move {
            if method == "systemCapabilities" {
                Ok(json!(["APP_UPDATES"]))
            } else {
                Err(format!("Unexpected recovery platform request: {method}"))
            }
        },
        move |event| reported.send(event).map_err(|error| error.to_string()),
    ));
    let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = reports.recv().await.unwrap();
            if event["type"] == "APP_UPDATE_DOWNLOAD_UPDATED" {
                let snapshot: serde_json::Value =
                    serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
                if snapshot["status"] == "COMPLETED" {
                    break snapshot;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(snapshot["filePath"], path);
    let snapshot = state
        .update_checks
        .download
        .state
        .lock()
        .unwrap()
        .snapshot
        .clone();
    assert_eq!(snapshot.status, AppUpdateDownloadStatus::Completed);
    assert_eq!(snapshot.file_path, path);
    let cancelled = execute_owned(state.clone(), Command::Cancel).await.unwrap();
    assert_eq!(cancelled.status, AppUpdateDownloadStatus::Idle);
    assert!(!Path::new(&path).exists());
    core.stop_core().await;
    serving.await.unwrap().unwrap();
}

#[tokio::test]
async fn truncated_or_failed_streams_remove_staging_and_never_publish_a_download_path() {
    let (_dir, core, state, identity) = setup();
    let (_cancel, mut receiver) = watch::channel(false);
    for chunks in [
        vec![Ok(&b"ab"[..])],
        vec![Ok(&b"abcdx"[..])],
        vec![Err(anyhow!("broken body"))],
        vec![],
    ] {
        assert!(
            save_stream(
                &state,
                "failed",
                &identity,
                futures_util::stream::iter(chunks),
                Some(4),
                &mut receiver
            )
            .await
            .is_err()
        );
        assert_eq!(
            std::fs::read_dir(state.cache.app_updates())
                .unwrap()
                .count(),
            0
        );
        assert!(
            crate::services::settings::updates::info(&state).unwrap()
                .downloaded_apk_path
                .is_empty()
        );
    }
    assert!(
        save_stream(
            &state,
            "wrong-length",
            &identity,
            futures_util::stream::iter([Ok(&b"abcd"[..])]),
            Some(5),
            &mut receiver
        )
        .await
        .is_err()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn cancellation_interrupts_a_pending_body_and_cleans_staging_before_join_returns() {
    let (_dir, core, state, identity) = setup();
    let (cancel, mut receiver) = watch::channel(false);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let mut entered = Some(entered);
    let body = futures_util::stream::poll_fn(move |_| {
        if let Some(entered) = entered.take() {
            let _ = entered.send(());
        }
        std::task::Poll::<Option<Result<&'static [u8]>>>::Pending
    });
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        save_stream(
            &worker_state,
            "cancelled",
            &identity,
            body,
            None,
            &mut receiver,
        )
        .await
    });
    waiting.await.unwrap();
    cancel.send_replace(true);
    assert!(worker.await.unwrap().is_err());
    assert_eq!(
        std::fs::read_dir(state.cache.app_updates())
            .unwrap()
            .count(),
        0
    );
    core.stop_core().await;
}

#[tokio::test]
async fn changed_release_or_missing_artifact_cannot_be_reported_as_completed() {
    let (_dir, core, state, identity) = setup();
    allow_updates(&state);
    let (_cancel, mut receiver) = watch::channel(false);
    updates::update(
        &state,
        json!({"downloadUrl":"https://example.invalid/replaced"}),
    )
    .unwrap();
    assert!(
        save_stream(
            &state,
            "stale",
            &identity,
            futures_util::stream::iter([Ok(&b"abcd"[..])]),
            Some(4),
            &mut receiver
        )
        .await
        .is_err()
    );
    assert_eq!(
        std::fs::read_dir(state.cache.app_updates())
            .unwrap()
            .count(),
        0
    );
    updates::update(&state, json!({"downloadUrl":identity.url})).unwrap();
    let path = save_stream(
        &state,
        "removed",
        &identity,
        futures_util::stream::iter([Ok(&b"abcd"[..])]),
        Some(4),
        &mut receiver,
    )
    .await
    .unwrap();
    std::fs::remove_file(path).unwrap();
    core.stop_core().await;
    let core =
        ContentServer::open_with_cache(&_dir.path().join("data"), &_dir.path().join("cache"))
            .unwrap();
    let state = core.runtime_state();
    let snapshot = execute_owned(state.clone(), Command::Reconcile)
        .await
        .unwrap();
    assert_ne!(snapshot.status, AppUpdateDownloadStatus::Completed);
    assert!(
        crate::services::settings::updates::info(&state).unwrap()
            .downloaded_apk_path
            .is_empty()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn cancel_command_awaits_worker_cleanup_and_shutdown_rejects_later_commands() {
    let (_dir, core, state, _) = setup();
    let runtime = state.update_checks.download.clone();
    let (cancel, mut receiver) = watch::channel(false);
    let cleaned = Arc::new(AtomicBool::new(false));
    let completed = cleaned.clone();
    *runtime.command.lock().await = Some(Task {
        cancel,
        join: tokio::spawn(async move {
            receiver.wait_for(|cancel| *cancel).await.unwrap();
            tokio::task::yield_now().await;
            completed.store(true, Ordering::Release);
        }),
    });
    execute_owned(state.clone(), Command::Cancel).await.unwrap();
    assert!(cleaned.load(Ordering::Acquire));
    runtime.shutdown().await;
    assert!(execute_owned(state.clone(), Command::Start).await.is_err());
    core.stop_core().await;
}

#[tokio::test]
async fn native_download_command_requires_current_capability_before_starting_network_work() {
    let (_dir, core, state, _) = setup();
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let reply = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "systemCapabilities");
        host.reply(generation, json!({"id":request["id"],"result":[]}))
            .unwrap();
        requests
    });
    let response = core
        .native_management()
        .unwrap()
        .start_app_update_download()
        .await;
    assert!(
        response
            .err()
            .unwrap()
            .contains("unsupported capability APP_UPDATES")
    );
    let retired = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "{ appUpdateDownload { status } }",
        ))
        .await;
    assert!(!retired.errors.is_empty());
    assert!(reply.await.unwrap().try_recv().is_err());
    assert!(!state.cache.app_updates().exists());
    let remote = crate::graphql::executor::execute_graphql(
        &state.public,
        async_graphql::Request::new("mutation { cancelAppUpdateDownload { status } }"),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!remote.errors.is_empty());
    core.stop_core().await;
}

#[tokio::test]
async fn settings_changes_cancel_the_current_transfer_and_clear_its_persisted_artifact() {
    let (_dir, core, state, _) = setup();
    let (cancel, receiver) = watch::channel(false);
    state.update_checks.download.state.lock().unwrap().cancel = Some(cancel);
    updates::update(&state, json!({"downloadedApkPath":"/not-owned/package"})).unwrap();
    assert!(updates::set_setting(&state, "skip_version", json!("999.0.0")).await.unwrap());
    assert!(*receiver.borrow());
    assert!(
        crate::services::settings::updates::info(&state).unwrap()
            .downloaded_apk_path
            .is_empty()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn dropping_the_request_does_not_detach_worker_cleanup_from_the_serialized_command() {
    let (_dir, core, state, _) = setup();
    let (cancel, mut receiver) = watch::channel(false);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let cleaned = Arc::new(AtomicBool::new(false));
    let completed = cleaned.clone();
    *state.update_checks.download.command.lock().await = Some(Task {
        cancel,
        join: tokio::spawn(async move {
            receiver.wait_for(|cancel| *cancel).await.unwrap();
            let _ = entered.send(());
            released.await.unwrap();
            completed.store(true, Ordering::Release);
        }),
    });
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let capabilities = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "systemCapabilities");
        host.reply(
            generation,
            json!({"id":request["id"],"result":["APP_UPDATES"]}),
        )
        .unwrap();
        requests
    });
    let worker_state = state.clone();
    let caller = tokio::spawn(async move { execute(&worker_state, Command::Cancel).await });
    waiting.await.unwrap();
    caller.abort();
    let _ = caller.await;
    assert!(!cleaned.load(Ordering::Acquire));
    release.send(()).unwrap();
    let snapshot = execute_owned(state.clone(), Command::Reconcile)
        .await
        .unwrap();
    assert_eq!(snapshot.status, AppUpdateDownloadStatus::Idle);
    assert!(cleaned.load(Ordering::Acquire));
    assert!(capabilities.await.unwrap().try_recv().is_err());
    core.stop_core().await;
}

#[tokio::test]
async fn clearing_application_cache_waits_for_the_active_body_before_deleting_its_directory() {
    let (_dir, core, state, identity) = setup();
    let (cancel, mut receiver) = watch::channel(false);
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let mut entered = Some(entered);
    let body = futures_util::stream::poll_fn(move |_| {
        if let Some(entered) = entered.take() {
            let _ = entered.send(());
        }
        std::task::Poll::<Option<Result<&'static [u8]>>>::Pending
    });
    let cleaned = Arc::new(AtomicBool::new(false));
    let completed = cleaned.clone();
    let worker_state = state.clone();
    let join = tokio::spawn(async move {
        assert!(
            save_stream(
                &worker_state,
                "cache-clear",
                &identity,
                body,
                None,
                &mut receiver
            )
            .await
            .is_err()
        );
        completed.store(true, Ordering::Release);
    });
    *state.update_checks.download.command.lock().await = Some(Task { cancel, join });
    waiting.await.unwrap();
    crate::services::cache::clear_application(&state)
        .await
        .unwrap();
    assert!(cleaned.load(Ordering::Acquire));
    assert!(!state.cache.app_updates().exists());
    assert_eq!(
        state
            .update_checks
            .download
            .state
            .lock()
            .unwrap()
            .snapshot
            .status,
        AppUpdateDownloadStatus::Idle
    );
    core.stop_core().await;
}
