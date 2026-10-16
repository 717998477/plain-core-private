use super::*;

#[test]
fn restart_receipts_are_serial_and_keep_live_ports_on_preflight_failure() {
    let runtime = Runtime::default();
    runtime.started((8080, 8443));
    let receipt = runtime.begin().unwrap();
    assert!(receipt.restarting);
    assert_eq!(receipt.revision.0, 1);
    assert!(runtime.begin().is_err());
    runtime.finish(receipt.revision, Err("TLS identity invalid".into()));
    let failed = runtime.snapshot();
    assert!(!failed.restarting);
    assert_eq!((failed.http_port, failed.https_port), (8080, 8443));
    assert_eq!(failed.error, "TLS identity invalid");
    let next = runtime.begin().unwrap();
    assert_eq!(next.revision.0, 2);
    assert!(next.error.is_empty());
    runtime.finish(receipt.revision, Ok((1, 2)));
    assert!(runtime.snapshot().restarting);
    runtime.stopped();
    runtime.started((8180, 8543));
    assert!(runtime.snapshot().restarting);
    runtime.finish(next.revision, Ok((8180, 8543)));
    let completed = runtime.snapshot();
    assert!(!completed.restarting);
    assert_eq!((completed.http_port, completed.https_port), (8180, 8543));
}

#[cfg(feature = "http_transport")]
async fn complete(runtime: &Runtime) -> HttpServerState {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let changed = runtime.completion.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            let state = runtime.snapshot();
            if !state.restarting {
                return state;
            }
            changed.await;
        }
    })
    .await
    .unwrap()
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn main_schema_acknowledges_restart_without_waiting_for_the_shutdown_lock() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let lock = state.shutdown.lock().await;
    let (_generation, mut requests) = state.host.connect();
    let response = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "mutation{restartHttpServer(httpPort:0,httpsPort:0){revision restarting error}}",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["restartHttpServer"]["restarting"],
        true
    );
    assert!(state.http_server.snapshot().restarting);
    let duplicate = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "mutation{restartHttpServer{revision}}",
        ))
        .await;
    assert!(!duplicate.errors.is_empty());
    let status = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "{httpServer{revision restarting}}",
        ))
        .await;
    assert!(status.errors.is_empty());
    assert_eq!(
        status.data.into_json().unwrap()["httpServer"]["revision"],
        1
    );
    assert!(requests.try_recv().is_err());
    drop(lock);
    let result = complete(&state.http_server).await;
    assert!(result.error.is_empty(), "{}", result.error);
    assert!(result.http_port > 0);
    assert!(result.https_port > 0);
    core.stop_core().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn http_restart_returns_its_acknowledgment_before_its_listener_drains() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let configured = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "mutation{updateUserSettings(input:{httpPort:0,httpsPort:0}){result{affectedCount}}}",
        ))
        .await;
    assert!(configured.errors.is_empty(), "{:?}", configured.errors);
    let (http, _) = core.start_configured_public().await.unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/graphql"))
        .bearer_auth(state.token.as_ref())
        .timeout(std::time::Duration::from_secs(10))
        .header("content-type", "application/json")
        .body(serde_json::json!({"query":"mutation{restartHttpServer(httpPort:0,httpsPort:0){revision restarting}}"}).to_string())
        .send().await.unwrap().text().await.unwrap();
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(response.get("errors").is_none(), "{response}");
    assert_eq!(response["data"]["restartHttpServer"]["restarting"], true);
    let completed = complete(&state.http_server).await;
    assert!(completed.error.is_empty(), "{}", completed.error);
    assert_eq!(
        core.http_ports().await,
        Some((completed.http_port as u16, completed.https_port as u16))
    );
    core.stop_core().await;
}

#[tokio::test]
async fn remote_and_invalid_restart_requests_never_start_a_runtime_operation() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let remote = crate::graphql::executor::execute_graphql(
        &state.public,
        async_graphql::Request::new("mutation{restartHttpServer(httpPort:0){revision}}"),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!remote.errors.is_empty());
    for document in [
        "mutation{restartHttpServer(httpPort:-1){revision}}",
        "mutation{restartHttpServer(httpsPort:65536){revision}}",
    ] {
        assert!(
            !core
                .native_graphql()
                .unwrap()
                .execute(async_graphql::Request::new(document))
                .await
                .errors
                .is_empty()
        );
    }
    assert_eq!(state.http_server.snapshot().revision.0, 0);
    core.stop_core().await;
}

#[test]
fn listener_started_notifications_are_internal_core_events() {
    let event =
        plain_rs::events::WsEvent::broadcast(plain_rs::events::HTTP_SERVER_STARTED, "{}".into());
    assert!(event.is_internal());
    assert!(
        plain_rs::events::WsEvent::host_text(plain_rs::events::HTTP_SERVER_STARTED, "{}".into())
            .is_none()
    );
}
