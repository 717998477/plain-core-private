use super::*;
use crate::graphql::executor::{RequestContext, execute_graphql};

#[tokio::test]
async fn retired_management_roots_are_unavailable_to_local_and_authenticated_remote_graphql() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    let state = core.runtime_state();
    let before = state.db.session_list().unwrap().len();
    let native = core.native_graphql().unwrap();
    for (field, document) in [
        ("systemSettings", "{systemSettings{password}}"),
        ("userSettings", "{userSettings{service}}"),
        ("systemPrefs", "{systemPrefs}"),
        (
            "toggleNotificationApp",
            "mutation{toggleNotificationApp(packageName:\"fixture\"){affectedCount}}",
        ),
        ("cycleAudioPlayMode", "mutation{cycleAudioPlayMode}"),
        ("httpServer", "{httpServer{revision}}"),
        ("verifyAdbToken", "{verifyAdbToken(token:\"fixture\")}"),
        (
            "restartHttpServer",
            "mutation{restartHttpServer(httpPort:0){revision}}",
        ),
        (
            "regenerateTlsCertificate",
            "mutation{regenerateTlsCertificate}",
        ),
        (
            "importTlsCertificatePem",
            "mutation{importTlsCertificatePem(certificatePem:\"invalid\",privateKeyPem:\"invalid\")}",
        ),
        (
            "importTlsCertificatePkcs12",
            "mutation{importTlsCertificatePkcs12(data:\"invalid\",password:\"fixture\")}",
        ),
        (
            "createSession",
            "mutation{createSession(name:\"Fixture\"){clientId}}",
        ),
        (
            "confirmWebLogin",
            "mutation{confirmWebLogin(requestId:\"fixture\",allowed:true)}",
        ),
        (
            "stageAppRestore",
            "mutation{stageAppRestore(path:\"fixture\")}",
        ),
        ("exportAppBackup", "mutation{exportAppBackup}"),
        (
            "createCrashReport",
            "mutation{createCrashReport(report:\"fixture\")}",
        ),
        (
            "startAppUpdateDownload",
            "mutation{startAppUpdateDownload{taskId}}",
        ),
        (
            "cancelAppUpdateDownload",
            "mutation{cancelAppUpdateDownload{taskId}}",
        ),
        ("checkAppUpdate", "mutation{checkAppUpdate}"),
    ] {
        let local = native.execute(async_graphql::Request::new(document)).await;
        assert!(
            local
                .errors
                .iter()
                .any(|error| error.message.contains("Unknown field")
                    && error.message.contains(field)),
            "{document}: {:?}",
            local.errors
        );
        let remote = execute_graphql(
            &state.public,
            async_graphql::Request::new(document),
            RequestContext::authenticated_remote(state.clone(), "fixture".into()),
        )
        .await;
        assert!(
            remote
                .errors
                .iter()
                .any(|error| error.message.contains("Unknown field")
                    && error.message.contains(field)),
            "{document}: {:?}",
            remote.errors
        );
    }
    assert_eq!(state.db.session_list().unwrap().len(), before);
    assert_eq!(state.http_server.snapshot().revision.0, 0);
    assert!(
        !plain_rs::data_paths::DataPaths::new(&directory.path().join("data"))
            .pending_restore()
            .exists()
    );
    core.stop_core().await;
}

#[tokio::test]
async fn native_management_handles_reject_every_operation_after_core_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    let management = core.native_management().unwrap();
    macro_rules! stopped {
        ($result:expr) => {
            assert_eq!($result.err().as_deref(), Some("Rust core stopped"));
        };
    }
    core.stop_core().await;
    stopped!(core.native_management());
    stopped!(management.restart_http_server(Some(0), Some(0)).await);
    stopped!(management.regenerate_tls_certificate().await);
    stopped!(
        management
            .import_tls_certificate_pem("invalid".into(), "invalid".into())
            .await
    );
    stopped!(
        management
            .import_tls_certificate_pkcs12(vec![], "fixture".into())
            .await
    );
    stopped!(management.create_session("Fixture".into()));
    stopped!(management.confirm_web_login("fixture".into(), true).await);
    stopped!(management.stage_app_restore("fixture".into()).await);
    stopped!(management.export_app_backup().await);
    stopped!(management.create_crash_report("fixture".into()).await);
    stopped!(management.verify_adb_token("fixture".into()));
    stopped!(management.system_settings());
    stopped!(management.user_settings());
    stopped!(management.system_preferences());
    stopped!(management.toggle_notification_app("fixture".into()).await);
    stopped!(management.start_app_update_download().await);
    stopped!(management.cancel_app_update_download().await);
    stopped!(management.check_app_update(false).await);
}

#[tokio::test]
async fn native_preference_reads_and_notification_changes_preserve_shared_state_and_events() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open_with_cache(
        &directory.path().join("data"),
        &directory.path().join("cache"),
    )
    .unwrap();
    let state = core.runtime_state();
    let management = core.native_management().unwrap();
    state.prefs.set("password", "fixture-password").unwrap();
    state
        .prefs
        .set("url_token", "fixture-resource-token")
        .unwrap();
    let system = serde_json::to_value(management.system_settings().unwrap()).unwrap();
    assert_eq!(system["password"], "fixture-password");
    assert_eq!(system["urlToken"], "fixture-resource-token");
    for private in ["signature_key_pair", "master_secret"] {
        assert!(
            management
                .system_preferences()
                .unwrap()
                .get(private)
                .is_none()
        );
    }
    let mut events = state.events.subscribe();
    for expected in [serde_json::json!(["fixture.app"]), serde_json::json!([])] {
        assert!(
            management
                .toggle_notification_app("fixture.app".into())
                .await
                .unwrap()
        );
        assert_eq!(
            management.user_settings().unwrap().notification_filter["apps"],
            expected
        );
        let mut changed = 0;
        let mut refreshed = 0;
        while let Ok(event) = events.try_recv() {
            changed += usize::from(event.event_type == "PREFS_UPDATED");
            refreshed += usize::from(event.event_type == "NOTIFICATION_REFRESHED");
        }
        assert_eq!(changed, 1);
        assert_eq!(refreshed, 1);
    }
    core.stop_core().await;
}
