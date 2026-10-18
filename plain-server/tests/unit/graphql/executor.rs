use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn local_and_remote_execute_the_same_envelope_and_preserve_errors() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set("api_permissions", vec!["WRITE_EXTERNAL_STORAGE"])
        .unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[8; 32]);
    let server = ContentServer::start(dir.path(), &token, prefs).unwrap();
    let state = server.runtime_state();
    let document = json!({"query":"query Count($q: String!) { noteCount(query: $q) } query Other { __typename }", "operationName":"Count", "variables":{"q":""}}).to_string();
    let local = server
        .graphql_executor()
        .execute(serde_json::from_str(&document).unwrap())
        .await;
    let remote = execute_graphql(
        &state.public,
        serde_json::from_str(&document).unwrap(),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&local).unwrap(),
        serde_json::to_value(&remote).unwrap()
    );
    assert!(local.errors.is_empty());
    for query in ["{ nonexistentField }", "{ audioHostQueueItems { path } }"] {
        let local = server.graphql_executor().execute(Request::new(query)).await;
        let remote = execute_graphql(
            &state.public,
            Request::new(query),
            RequestContext::remote(Some(state.clone())),
        )
        .await;
        assert!(!local.errors.is_empty());
        assert_eq!(
            serde_json::to_value(local).unwrap(),
            serde_json::to_value(remote).unwrap()
        );
    }
    let query = r#"{ audioQueueItems(query: "", offset: 0, limit: 10) { mediaId } }"#;
    let local = server.graphql_executor().execute(Request::new(query)).await;
    let remote = execute_graphql(
        &state.public,
        Request::new(query),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(local.errors.is_empty(), "{:?}", local.errors);
    assert_eq!(
        serde_json::to_value(local).unwrap(),
        serde_json::to_value(remote).unwrap()
    );
    #[cfg(feature = "http_transport")]
    server.stop_public().await;
    let response = server
        .graphql_executor()
        .execute(Request::new(r#"{ noteCount(query: "") }"#))
        .await;
    assert!(
        response.errors.is_empty(),
        "native execution must survive HTTP shutdown"
    );
    server.shutdown().await;
}

#[test]
fn service_owned_media_notifications_are_not_repeated_by_aliases_or_fragments() {
    for query in [
        "mutation { renamed: mediaAction { affectedCount } }",
        "mutation { ... on Mutation { deleteMediaItems { affectedCount } } }",
        "mutation { ...One ...One } fragment One on Mutation { mediaAction { affectedCount } }",
        "mutation { ...One } fragment One on Mutation { ...Two } fragment Two on Mutation { moveMediaItems { affectedCount } }",
    ] {
        assert!(!content_changes(&Request::new(query)), "{query}");
    }
    assert!(content_changes(&Request::new(
        "mutation { mediaAction { affectedCount } createNote { id } }"
    )));
    assert!(content_changes(&Request::new(
        "mutation { ...One } fragment One on Mutation { createNote { id } }"
    )));
    assert!(!content_changes(&Request::new(
        "mutation { ...One } fragment One on Mutation { ...One }"
    )));
    assert!(!content_changes(
        &Request::new(
            "mutation Media { mediaAction { affectedCount } } mutation Other { createNote { id } }"
        )
        .operation_name("Media")
    ));
}

#[test]
fn cache_only_and_task_owned_mutations_skip_the_generic_content_broadcast() {
    for query in [
        "mutation { preview: cacheSharedFile }",
        "mutation { cacheRemoteFile cacheZipEntry cacheAudioCover exportAppLogs exportAppBackup stageAppRestore }",
        "mutation { ...Cache } fragment Cache on Mutation { cacheRemoteFile }",
        "mutation { ... on Mutation { copyFileToDirectory copyFile moveFile renameFile renameFilePath } }",
        "mutation { createFileTask }",
    ] {
        assert!(!content_changes(&Request::new(query)), "{query}");
    }
    assert!(content_changes(&Request::new(
        "mutation { cacheRemoteFile createNote }"
    )));
    assert!(content_changes(&Request::new("mutation { clearAppLogs }")));
    assert!(content_changes(&Request::new(
        "mutation { clearAppLogFiles }"
    )));
}

#[tokio::test]
async fn backup_uses_the_main_schema_and_rejects_remote_secret_export() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(
        plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(directory.path())).unwrap(),
    );
    let token = plain_rs::utils::base64::base64_encode(&[8; 32]);
    let server = ContentServer::start(directory.path(), &token, prefs).unwrap();
    let state = server.runtime_state();
    let remote = execute_graphql(
        &state.public,
        Request::new("mutation { exportAppBackup }"),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!remote.errors.is_empty());
    let restore = execute_graphql(
        &state.public,
        Request::new("mutation { stageAppRestore(path:\"/not-accessible/backup.zip\") }"),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!restore.errors.is_empty());
    assert!(
        !plain_rs::data_paths::DataPaths::new(directory.path())
            .pending_restore()
            .exists()
    );
    assert!(
        std::fs::read_dir(state.cache.root())
            .unwrap()
            .next()
            .is_none()
    );
    let local = server
        .graphql_executor()
        .execute(Request::new("mutation { exportAppBackup }"))
        .await;
    assert!(local.errors.is_empty(), "{:?}", local.errors);
    let json = local.data.into_json().unwrap();
    assert!(std::path::Path::new(json["exportAppBackup"].as_str().unwrap()).exists());
    server.shutdown().await;
}

#[test]
fn runtime_controls_and_viewer_exports_do_not_trigger_generic_content_refreshes() {
    for document in [
        "mutation{controlPeerStatus(action:STOP)}",
        r#"mutation{acquireDiscovery(token:"token") releaseDiscovery(token:"token")}"#,
        r#"mutation{browseMdns{revision} acquireMdnsDebugSession(token:"token"){debugSessionCount}}"#,
        "mutation{controlDlnaReceiver(input:{action:POSITION,positionMs:1,durationMs:2})}",
        r#"mutation{confirmWebLogin(requestId:"request",allowed:false)}"#,
        r#"mutation{createCrashReport(report:"report") importPomodoroSound(source:"source")}"#,
        "mutation{...Control} fragment Control on Mutation{alias:controlPeerStatus(action:RECONNECT)}",
    ] {
        assert!(!content_changes(&Request::new(document)), "{document}");
    }
    assert!(content_changes(&Request::new(
        "mutation{controlPeerStatus(action:STOP) createNote}"
    )));
}

#[test]
fn versioned_audio_selection_owns_its_content_notification() {
    assert!(!content_changes(&Request::new(
        r#"mutation { advanceAudioSelection(action:NEXT,expectedPath:"song",expectedRevision:1) { revision } }"#
    )));
}

#[test]
fn playback_fact_reports_do_not_trigger_generic_content_refresh() {
    assert!(!content_changes(&Request::new(
        r#"mutation { reportAudioPlayback(path:"song",revision:1,positionMs:0,started:true) }"#
    )));
}

#[test]
fn shared_batch_chat_and_share_flows_own_their_events() {
    for query in [
        r#"mutation { sendChatItems(targets:["local"],content:"{}") { id } }"#,
        r#"mutation { forwardChatItems(id:"source",targets:["local"]) { id } }"#,
        r#"mutation { sendShare(targets:["local"],shareId:"share") { id } }"#,
        r#"mutation { sendFolderShare(targets:["local"],path:"/folder",name:"Folder") { id } }"#,
    ] {
        assert!(!content_changes(&Request::new(query)));
    }
}
