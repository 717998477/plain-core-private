use super::*;
use crate::assembly::server::ContentServer;
use serde_json::{Value, json};
use std::sync::Arc;

async fn call(port: u16, token: &str, query: &str) -> Value {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query": query}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}

fn start(dir: &tempfile::TempDir) -> (ContentServer, String, Arc<plain_rs::prefs::Prefs>) {
    let token = plain_rs::utils::base64::base64_encode(&[9; 32]);
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    prefs.set("api_permissions", json!(["CLIPBOARD"])).unwrap();
    (
        ContentServer::start(dir.path(), &token, prefs.clone()).unwrap(),
        token,
        prefs,
    )
}

#[tokio::test]
async fn native_report_http_search_delete_and_restart_share_one_clipboard() {
    let dir = tempfile::tempdir().unwrap();
    let (server, token, prefs) = start(&dir);
    let bridge = server.host_events();
    let mut events = server.runtime_state().events.subscribe();
    let payload = json!({"text": "100%_ 🌍", "sensitive": true}).to_string();
    bridge
        .publish_text("CLIPBOARD_REPORTED", payload.clone())
        .await
        .unwrap();
    let changed = events.try_recv().unwrap();
    assert_eq!(changed.event_type, "CONTENT_CHANGED");
    assert_eq!(changed.payload, "{}");
    assert!(events.try_recv().is_err());
    let page = call(server.port, &token,
        r#"{clipboardItems(offset:0,limit:50,query:"%_") {id text source label sensitive createdAt} clipboardItemCount(query:"%_")}"#).await;
    assert!(page.get("errors").is_none(), "{page}");
    assert_eq!(page["data"]["clipboardItemCount"], 1);
    let item = &page["data"]["clipboardItems"][0];
    assert_eq!(item["text"], "100%_ 🌍");
    assert_eq!(item["source"], "");
    assert_eq!(item["label"], "");
    assert_eq!(item["sensitive"], true);
    assert!(chrono::DateTime::parse_from_rfc3339(item["createdAt"].as_str().unwrap()).is_ok());
    bridge
        .publish_text("CLIPBOARD_REPORTED", payload)
        .await
        .unwrap();
    assert!(events.try_recv().is_err());
    for invalid in [
        json!({"text":" ", "sensitive":false}),
        json!({"text":"forged", "sensitive":false, "source":"peer"}),
    ] {
        assert!(
            bridge
                .publish_text("CLIPBOARD_REPORTED", invalid.to_string())
                .await
                .is_err()
        );
    }
    for invalid in [
        r#"mutation {deleteClipboardItems(query:" ") {affectedCount}}"#,
        r#"mutation {deleteClipboardItems(query:"source:peer") {affectedCount}}"#,
        r#"mutation {recordClipboard(input:{text:"forged",source:"peer",label:"",sensitive:false}) {inserted}}"#,
    ] {
        assert!(
            call(server.port, &token, invalid)
                .await
                .get("errors")
                .is_some()
        );
    }
    assert_eq!(
        call(
            server.port,
            &token,
            r#"mutation {deleteClipboardItems(query:"ids:missing") {affectedCount}}"#
        )
        .await["data"]["deleteClipboardItems"]["affectedCount"],
        0
    );
    server.shutdown().await;
    drop(bridge);
    let server = ContentServer::start(dir.path(), &token, prefs).unwrap();
    assert_eq!(
        call(server.port, &token, r#"{clipboardItemCount(query:"")}"#).await["data"]["clipboardItemCount"],
        1
    );
    assert_eq!(
        call(
            server.port,
            &token,
            r#"mutation {deleteClipboardItems(query:"all:true") {affectedCount}}"#
        )
        .await["data"]["deleteClipboardItems"]["affectedCount"],
        1
    );
    server.shutdown().await;
}

#[tokio::test]
async fn reports_recheck_permission_deduplicate_and_publish_only_nonsensitive_text() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _, prefs) = start(&dir);
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    prefs.set("api_permissions", json!([])).unwrap();
    let payload = json!({"text":"visible 🌍", "sensitive":false}).to_string();
    report(&state, &payload).unwrap();
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 0);
    assert!(events.try_recv().is_err());
    prefs.set("api_permissions", json!(["CLIPBOARD"])).unwrap();
    report(&state, &payload).unwrap();
    assert_eq!(events.try_recv().unwrap().event_type, "CONTENT_CHANGED");
    let event = events.try_recv().unwrap();
    assert_eq!(event.event_type, "CLIPBOARD_CHANGED");
    let body: Value = serde_json::from_str(&event.payload).unwrap();
    let item = state.db.clipboard_latest().unwrap().unwrap();
    assert_eq!(
        body,
        json!({"text":item.text,"sensitive":false,
        "time":chrono::DateTime::parse_from_rfc3339(&item.created_at).unwrap().timestamp_millis()})
    );
    report(&state, &payload).unwrap();
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 1);
    assert!(events.try_recv().is_err());
    for invalid in [
        json!({"text":"", "sensitive":false}),
        json!({"text":"🌍".repeat(128 * 1024 + 1), "sensitive":false}),
    ] {
        assert!(report(&state, &invalid.to_string()).is_err());
    }
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 1);
    assert!(events.try_recv().is_err());
    server.shutdown().await;
}
