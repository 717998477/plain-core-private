use super::*;

#[tokio::test]
async fn sdk_file_access_without_a_system_media_catalog_records_portable_file_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let source = tempfile::Builder::new()
        .prefix("catalog-source-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    let path = source.path().join("document.txt");
    std::fs::write(&path, b"owned catalog document").unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let host = Arc::new(Host::default());
    let platform = FilePlatform::new(
        host.clone(),
        state.db.clone(),
        state.directory.clone(),
        state.cache.root().to_owned(),
        state.events.clone(),
        #[cfg(feature = "media")]
        state.media_scans.clone(),
    );
    platform.use_sdk_for_test();
    let (generation, mut requests) = host.connect();
    let responder = host.clone();
    let worker = tokio::spawn(async move {
        let mut scans = 0;
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "fileTaskScan");
            scans += 1;
            responder
                .reply(generation, json!({"id":request["id"],"result":true}))
                .unwrap();
        }
        scans
    });
    platform
        .scan(&[path.to_string_lossy().into_owned()])
        .await
        .unwrap();
    let count =
        super::super::media_catalog::count(&state.db, plain_rs::enums::DataType::Doc, "").unwrap();
    host.disconnect(generation);
    let host_scans = worker.await.unwrap();
    core.stop_core().await;
    assert_eq!(count, 1);
    assert_eq!(host_scans, 0);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn native_recent_files_use_the_owned_preferences_without_media_or_host_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let first = directory.path().join("first.txt");
    let last = directory.path().join("last.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&last, b"last").unwrap();
    let state = core.runtime_state();
    state
        .prefs
        .set(
            "recent_files",
            vec![
                first.to_string_lossy().into_owned(),
                directory
                    .path()
                    .join("missing.txt")
                    .to_string_lossy()
                    .into_owned(),
                last.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
    let (_generation, mut requests) = state.host.connect();
    let response = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "{ recentFiles { name path size isDir } }",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let files = data["recentFiles"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], first.to_string_lossy().as_ref());
    assert_eq!(files[1]["path"], last.to_string_lossy().as_ref());
    assert_eq!(files[0]["size"], 5);
    assert_eq!(files[0]["isDir"], false);
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

async fn sdk_recents(
    properties: serde_json::Value,
    provider_rows: serde_json::Value,
) -> (Result<Vec<serde_json::Value>>, Vec<String>) {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let host =
        Arc::new(Host::with_startup_properties(properties.as_object().unwrap().clone()).unwrap());
    let platform = FilePlatform::new(
        host.clone(),
        state.db.clone(),
        directory.path().to_path_buf(),
        state.cache.root().to_owned(),
        state.events.clone(),
        #[cfg(feature = "media")]
        state.media_scans.clone(),
    );
    platform.use_sdk_for_test();
    let (generation, mut requests) = host.connect();
    let responder = host.clone();
    let worker = tokio::spawn(async move {
        let mut calls = Vec::new();
        while let Some(request) = requests.recv().await {
            let method = request["method"].as_str().unwrap();
            calls.push(method.to_owned());
            let result = match method {
                "systemFileIndexRows" => {
                    assert_eq!(request["params"]["limit"], 100);
                    assert_eq!(request["params"]["offset"], 0);
                    assert_eq!(request["params"]["sortBy"], "DATE_DESC");
                    assert_eq!(
                        request["params"]["plan"]["clauses"],
                        json!(["mime_type != ?"])
                    );
                    assert_eq!(
                        request["params"]["plan"]["args"],
                        json!(["vnd.android.document/directory"])
                    );
                    provider_rows.clone()
                }
                other => panic!("unexpected recent file method {other}"),
            };
            responder
                .reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
        calls
    });
    let result = platform.recent_files(&state.prefs).await;
    host.disconnect(generation);
    let calls = worker.await.unwrap();
    core.stop_core().await;
    (result, calls)
}

#[tokio::test]
async fn sdk_catalog_reuses_startup_metadata_and_queries_only_provider_rows() {
    let rows = json!([{"name":"catalog.txt","path":"/catalog.txt","size":42}]);
    let (result, calls) = sdk_recents(
        json!({"files.recentCatalog":true,"files.recentRoot":null}),
        rows.clone(),
    )
    .await;
    assert_eq!(result.unwrap(), rows.as_array().unwrap().clone());
    assert_eq!(calls, ["systemFileIndexRows"]);
}

#[tokio::test]
async fn sdk_filesystem_scan_runs_in_rust_without_provider_or_per_file_host_calls() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), b"sample").unwrap();
    let (result, calls) = sdk_recents(
        json!({"files.recentCatalog":false,"files.recentRoot":dir.path()}),
        json!([]),
    )
    .await;
    let files = result.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["name"], "file.txt");
    assert_eq!(files[0]["size"], 6);
    assert!(files[0]["createdAt"].is_null());
    assert!(files[0]["mediaId"].is_null());
    assert!(calls.is_empty());
}

#[tokio::test]
async fn invalid_recent_source_is_rejected_without_guessing_or_provider_fallback() {
    for properties in [
        json!({}),
        json!({"files.recentCatalog":false,"files.recentRoot":"relative"}),
    ] {
        let (result, calls) = sdk_recents(properties, json!([])).await;
        assert!(result.is_err());
        assert!(calls.is_empty());
    }
}

#[tokio::test]
async fn sdk_without_a_catalog_or_filesystem_source_uses_the_shared_history() {
    let (result, calls) = sdk_recents(
        json!({"files.recentCatalog":false,"files.recentRoot":null}),
        json!([{ "name":"must not request provider rows" }]),
    )
    .await;
    assert!(result.unwrap().is_empty());
    assert!(calls.is_empty());
}

#[tokio::test]
async fn thumbnail_cache_opens_the_file_without_sdk_requests_and_rechecks_access() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let (_, mut requests) = state.host.connect();
    let path = directory.path().join("image");
    std::fs::write(&path, []).unwrap();
    for _ in 0..2 {
        let cache = state
            .file_platform
            .thumbnail_cache(path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(cache, state.cache.root().to_owned());
    }
    std::fs::remove_file(&path).unwrap();
    assert!(
        state
            .file_platform
            .thumbnail_cache(path.to_str().unwrap())
            .await
            .is_err()
    );
    std::fs::create_dir(&path).unwrap();
    assert!(
        state
            .file_platform
            .thumbnail_cache(path.to_str().unwrap())
            .await
            .is_err()
    );
    assert!(
        state
            .file_platform
            .thumbnail_cache("content://images/1")
            .await
            .is_err()
    );
    assert!(
        state
            .file_platform
            .thumbnail_cache("relative")
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

#[cfg(unix)]
#[tokio::test]
async fn thumbnail_cache_rechecks_current_os_read_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let (_, mut requests) = state.host.connect();
    let path = directory.path().join("image");
    std::fs::write(&path, []).unwrap();
    state
        .file_platform
        .thumbnail_cache(path.to_str().unwrap())
        .await
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0)).unwrap();
    let can_open = std::fs::File::open(&path).is_ok();
    assert_eq!(
        state
            .file_platform
            .thumbnail_cache(path.to_str().unwrap())
            .await
            .is_ok(),
        can_open
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    state
        .file_platform
        .thumbnail_cache(path.to_str().unwrap())
        .await
        .unwrap();
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

#[tokio::test]
async fn metadata_authorization_batches_sdk_paths_without_requiring_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let (generation, mut requests) = state.host.connect();
    assert!(
        state
            .file_platform
            .authorize_metadata_paths(&["relative".into()])
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    let paths = vec![
        directory
            .path()
            .join("missing-a")
            .to_string_lossy()
            .into_owned(),
        directory
            .path()
            .join("missing-b")
            .to_string_lossy()
            .into_owned(),
    ];
    let host = state.host.clone();
    let worker = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "systemPermissionFacts");
        assert_eq!(
            request["params"],
            json!({"permissions":["WRITE_EXTERNAL_STORAGE"]})
        );
        host.reply(
            generation,
            json!({"id":request["id"], "result":{"granted":{"WRITE_EXTERNAL_STORAGE":true}}}),
        )
        .unwrap();
        requests
    });
    state
        .file_platform
        .authorize_metadata_paths(&paths)
        .await
        .unwrap();
    let mut requests = worker.await.unwrap();
    assert!(requests.try_recv().is_err());
    state.host.disconnect(generation);
    core.stop_core().await;
}

#[tokio::test]
async fn sdk_private_paths_skip_host_and_external_task_batches_sample_one_live_grant() {
    let temp = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(&temp.path().join("core")).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let (generation, mut requests) = state.host.connect();
    let private = vec![
        state
            .directory
            .join("new/file")
            .to_string_lossy()
            .into_owned(),
        state
            .cache
            .root()
            .join("new/file")
            .to_string_lossy()
            .into_owned(),
    ];
    state
        .file_platform
        .authorize_paths(&private, true)
        .await
        .unwrap();
    state.file_platform.scan(&private).await.unwrap();
    assert!(
        state
            .file_platform
            .identities(&private)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(requests.try_recv().is_err());
    assert!(
        state
            .file_platform
            .authorize_paths(&["/".into()], true)
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    let host = state.host.clone();
    let worker = tokio::spawn(async move {
        let mut calls = 0;
        for granted in [true, false] {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["method"], "systemPermissionFacts");
            assert_eq!(
                request["params"],
                json!({"permissions":["WRITE_EXTERNAL_STORAGE"]})
            );
            calls += 1;
            host.reply(
                generation,
                json!({"id":request["id"],"result":{"granted":{"WRITE_EXTERNAL_STORAGE":granted}}}),
            )
            .unwrap();
        }
        (calls, requests)
    });
    let ops = (0..64)
        .map(|index| FileTaskOp {
            src: temp
                .path()
                .join(format!("source-{index}"))
                .to_string_lossy()
                .into_owned(),
            dst: temp
                .path()
                .join(format!("target-{index}"))
                .to_string_lossy()
                .into_owned(),
            overwrite: false,
        })
        .collect::<Vec<_>>();
    state
        .file_platform
        .authorize_task(FileTaskType::Move, &ops)
        .await
        .unwrap();
    assert!(
        state
            .file_platform
            .authorize_task(FileTaskType::Copy, &ops)
            .await
            .is_err()
    );
    let (calls, mut requests) = worker.await.unwrap();
    assert_eq!(calls, 2);
    assert!(requests.try_recv().is_err());
    state.host.disconnect(generation);
    core.stop_core().await;
}

#[test]
fn native_read_access_rechecks_real_targets_and_rejects_missing_or_relative_paths() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file");
    std::fs::write(&file, b"bytes").unwrap();
    assert!(authorize_native(&file, false).is_ok());
    assert!(authorize_native(temp.path(), false).is_ok());
    assert!(authorize_native(Path::new("relative"), false).is_err());
    assert!(authorize_native(&temp.path().join("missing"), false).is_err());
    #[cfg(unix)]
    {
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(authorize_native(&link, false).is_ok());
        std::fs::remove_file(file).unwrap();
        assert!(authorize_native(&link, false).is_err());
    }
}
