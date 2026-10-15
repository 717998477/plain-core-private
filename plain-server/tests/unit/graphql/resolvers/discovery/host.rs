#[tokio::test]
async fn nearby_snapshot_uses_the_main_contract_without_starting_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let native = core.native_graphql().unwrap();
    assert!(
        core.runtime_state()
            .public
            .sdl()
            .contains("startDiscovery: Boolean!")
    );
    let stopped = native
        .execute(async_graphql::Request::new("mutation { stopDiscovery }"))
        .await;
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    assert_eq!(
        stopped.data.into_json().unwrap()["stopDiscovery"],
        serde_json::json!(true)
    );

    let selection = "{ nearbyDevices { id name ips port deviceType version platform lastSeen discoveryMethods bestIp status } }";
    let response = native.execute(async_graphql::Request::new(selection)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["nearbyDevices"],
        serde_json::json!([])
    );
    assert!(
        !core.runtime_state().mdns.snapshot()["scanning"]
            .as_bool()
            .unwrap()
    );
    let state = core.runtime_state();
    let response = crate::graphql::executor::execute_graphql(
        &state.public,
        async_graphql::Request::new(selection),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["nearbyDevices"],
        serde_json::json!([])
    );
    core.stop_core().await;
    let response = native.execute(async_graphql::Request::new(selection)).await;
    assert!(!response.errors.is_empty());
}

#[tokio::test]
async fn is_discovering_reports_the_manual_scan_loop() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let native = core.native_graphql().unwrap();
    let selection = "{ isDiscovering }";
    let response = native.execute(async_graphql::Request::new(selection)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["isDiscovering"],
        serde_json::json!(false),
        "a core that never started the scan loop reports no discovery"
    );
    assert!(
        !core.runtime_state().mdns.manual_scanning(),
        "reading the query must not start the scan loop"
    );
    core.stop_core().await;
}
