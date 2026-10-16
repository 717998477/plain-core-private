use super::*;

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn app_and_remote_access_share_listeners_without_background_service() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set_user("service", false).unwrap();
    prefs.set_user("desktop_access", false).unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[19; 32]);
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let server = ContentServer::start_http(
        directory.path(),
        &token,
        prefs.clone(),
        serde_json::Map::new(),
        0,
        0,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
        false,
        "",
    )
    .await
    .unwrap();
    assert!(
        server.task.is_none(),
        "unified mode must not create a separate loopback listener"
    );
    let (http, https) = server.http_ports().await.unwrap();
    assert_eq!(http, server.port);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    for url in [
        format!("http://127.0.0.1:{http}"),
        format!("https://127.0.0.1:{https}"),
    ] {
        let response = client
            .get(format!("{url}/health"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .bearer_auth(&token)
                .header("content-type", "application/json")
                .body(serde_json::json!({"query":"{ __typename }"}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .post(format!("{url}/graphql"))
                .header("content-type", "application/json")
                .body(serde_json::json!({"query":"{ __typename }"}).to_string())
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            client
                .get(format!("{url}/host"))
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
    let router =
        crate::http::routes::public::build_router(server.state.clone(), server.stop.subscribe());
    use tower::ServiceExt;
    for peer in [None, Some("192.0.2.42:1234"), Some("127.0.0.1:1234")] {
        let mut request = axum::http::Request::builder()
            .method("GET")
            .uri("/health")
            .header("authorization", format!("Bearer {token}"))
            .header("x-forwarded-for", "127.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        if let Some(peer) = peer {
            request.extensions_mut().insert(axum::extract::ConnectInfo(
                peer.parse::<std::net::SocketAddr>().unwrap(),
            ));
        }
        let expected = if peer == Some("127.0.0.1:1234") {
            200
        } else {
            403
        };
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            expected
        );
    }
    prefs.set_user("desktop_access", true).unwrap();
    assert_eq!(
        client
            .post(format!("http://127.0.0.1:{http}/graphql"))
            .header("content-type", "application/json")
            .body(serde_json::json!({"query":"{ __typename }"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn configured_public_uses_core_settings_and_keeps_listeners_on_tls_preflight_error() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let response = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            "mutation{updateUserSettings(input:{httpPort:0,httpsPort:0}){result{affectedCount}}}",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        core.runtime_state()
            .prefs
            .get_user::<i32>("http_port")
            .unwrap(),
        Some(0)
    );
    assert_eq!(
        core.runtime_state()
            .prefs
            .get_user::<i32>("https_port")
            .unwrap(),
        Some(0)
    );
    let ports = core.start_configured_public().await.unwrap();
    assert_ne!(ports.0, 0);
    assert_ne!(ports.1, 0);
    assert_eq!(core.http_ports().await, Some(ports));
    assert!(core.task.is_none());
    std::fs::write(directory.path().join("tls-identity.json"), "{}").unwrap();
    assert!(core.runtime_state().restart_public().await.is_err());
    assert_eq!(core.http_ports().await, Some(ports));
    core.stop_core().await;
    assert_eq!(core.http_ports().await, None);
    assert!(core.native_url_token().is_err());
    assert!(core.native_graphql().is_err());
    assert!(core.start_configured_public().await.is_err());
    assert!(core.runtime_state().restart_public().await.is_err());
}

#[cfg(feature = "core")]
#[tokio::test]
async fn shell_preferences_share_the_core_owner() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let shell = core.shared_preferences().unwrap();
    assert!(Arc::ptr_eq(&shell, &core.state.prefs));
    shell.set_user("device_name", "Shared device").unwrap();
    assert_eq!(
        core.preferences()
            .unwrap()
            .get_user::<String>("device_name")
            .unwrap(),
        Some("Shared device".into())
    );
    core.stop_core().await;
    assert!(core.shared_preferences().is_err());
}

#[tokio::test]
async fn every_transport_build_serves_the_main_guest_and_peer_contracts_without_private_routes() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set_user("desktop_access", true).unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[3; 32]);
    let server = ContentServer::start(directory.path(), &token, prefs).unwrap();
    let client = reqwest::Client::new();
    let document = serde_json::json!({"query":"query Main { __typename audioQueuedMediaIds }","operationName":"Main"});
    let native = server
        .graphql_executor()
        .execute(serde_json::from_value(document.clone()).unwrap())
        .await;
    assert!(native.errors.is_empty(), "{:?}", native.errors);
    let http = client
        .post(format!("http://127.0.0.1:{}/graphql", server.port))
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(document.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(http.status(), 200);
    let body: serde_json::Value = serde_json::from_slice(&http.bytes().await.unwrap()).unwrap();
    assert_eq!(body, serde_json::to_value(native).unwrap());
    for path in ["/guest_graphql", "/peer_graphql"] {
        let response = client
            .post(format!("http://127.0.0.1:{}{path}", server.port))
            .body(document.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            401,
            "{path} must reach its own authentication gate"
        );
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
    }
    for path in [
        "/host",
        "/host/graphql",
        "/hostGraphql",
        "/host_graphql",
        "/private/graphql",
        "/private_graphql",
        "/content_graphql",
        "/system/tls",
        "/system/provider-plan",
        "/providers/media-buckets",
        "/images/models",
        "/images/search",
        "/chat/mdns",
        "/chat/prewarm",
        "/chat/store",
        "/chat/download",
        "/chat/transport",
        "/shares/batch",
        "/shares/client",
        "/shares/client/file",
        "/shares/client/zip",
    ] {
        let response = client
            .post(format!("http://127.0.0.1:{}{path}", server.port))
            .bearer_auth(&token)
            .body(document.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 404, "obsolete endpoint {path}");
    }
    server.stop_core().await;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn startup_device_name_initialization_is_shared_and_preserves_user_changes_without_host_calls()
 {
    for (initial, expected) in [
        (None, "Startup Name"),
        (Some(" \t"), "Startup Name"),
        (Some("Chosen Name"), "Chosen Name"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let prefs = Arc::new(
            plain_rs::prefs::Prefs::load(
                &plain_rs::data_paths::DataPaths::new(directory.path()).system_prefs(),
            )
            .unwrap(),
        );
        if let Some(initial) = initial {
            prefs.set_user("device_name", initial).unwrap();
        }
        let properties = serde_json::json!({"name":"Startup Name"})
            .as_object()
            .unwrap()
            .clone();
        let token = plain_rs::utils::base64::base64_encode(&[7; 32]);
        let (state, stop, _stopped) = ContentServer::build_state_with_properties(
            directory.path(),
            &token,
            prefs.clone(),
            properties,
        )
        .unwrap();
        assert_eq!(
            prefs.get_user::<String>("device_name").unwrap().as_deref(),
            Some(expected)
        );
        let (_generation, mut requests) = state.host.connect();
        for name in [expected, "Updated Name"] {
            prefs.set_user("device_name", name).unwrap();
            let response = crate::graphql::executor::execute_graphql(
                &state.public,
                async_graphql::Request::new("{app{deviceName}}"),
                crate::graphql::executor::RequestContext::local(state.clone()),
            )
            .await;
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            assert_eq!(
                response.data.into_json().unwrap()["app"]["deviceName"],
                name
            );
        }
        assert!(requests.try_recv().is_err());
        stop.send(true).unwrap();
    }
}
