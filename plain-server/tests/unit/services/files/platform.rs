use super::*;

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
async fn thumbnail_cache_uses_owned_data_root_and_keeps_authorization_live() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let worker = tokio::spawn(async move {
        for allowed in [true, true, false] {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["method"], "thumbnailAuthorize");
            assert_eq!(request["params"], json!({"paths":["content://images/1"]}));
            host.reply(generation, json!({"id":request["id"],"result":allowed}))
                .unwrap();
        }
        assert!(requests.try_recv().is_err());
    });
    for _ in 0..2 {
        let cache = state
            .file_platform
            .thumbnail_cache("content://images/1")
            .await
            .unwrap();
        assert_eq!(cache, state.cache.root().to_owned());
    }
    assert!(
        state
            .file_platform
            .thumbnail_cache("content://images/1")
            .await
            .is_err()
    );
    worker.await.unwrap();
    core.stop_core().await;
}

#[tokio::test]
async fn metadata_authorization_batches_sdk_paths_without_requiring_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
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
    let expected = paths.clone();
    let host = state.host.clone();
    let worker = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "fileTaskAuthorize");
        assert_eq!(
            request["params"],
            json!({"operation":"read", "paths":expected})
        );
        host.reply(generation, json!({"id":request["id"], "result":true}))
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
