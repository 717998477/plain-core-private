use super::*;
use crate::assembly::server::ContentServer;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn platform_callbacks_before_write_reply_do_not_republish_received_text() {
    let directory = tempfile::tempdir().unwrap();
    let (server, _, _) = start(&directory);
    let state = server.runtime_state();
    let bridge = Arc::new(server.host_events());
    let callbacks = bridge.clone();
    let serving = tokio::spawn(bridge.clone().serve(
        move |method, params| {
            let callbacks = callbacks.clone();
            async move {
                match method.as_str() {
                    "systemCapabilities" => Ok(json!(["CLIPBOARD"])),
                    "systemSetClipboard" => {
                        let report = json!({"text":params["text"],"sensitive":false}).to_string();
                        for _ in 0..2 {
                            callbacks
                                .publish_text("CLIPBOARD_REPORTED", report.clone())
                                .await?;
                        }
                        Ok(json!(true))
                    }
                    _ => Err(format!("unexpected host method {method}")),
                }
            }
        },
        |_| Ok(()),
    ));
    let mut events = state.events.subscribe();
    let response = server
        .graphql_executor()
        .execute(async_graphql::Request::new(
            r#"mutation { setClipboard(text: "remote callback fixture") }"#,
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["setClipboard"], true);
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 1);
    assert!(!state.clipboard_changes.can_send("remote callback fixture"));
    let mut history_updates = 0;
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.event_type, "CLIPBOARD_CHANGED");
        if event.event_type == plain_rs::events::H_CLIPBOARD_HISTORY_UPDATED {
            history_updates += 1;
        }
    }
    assert_eq!(history_updates, 1);
    server.shutdown().await;
    serving.await.unwrap().unwrap();
    assert!(
        bridge
            .publish_text(
                "CLIPBOARD_REPORTED",
                json!({
                    "text":"after shutdown", "sensitive":false,
                })
                .to_string()
            )
            .await
            .is_err()
    );
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 1);
}

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
    assert_eq!(
        changed.event_type,
        plain_rs::events::H_CLIPBOARD_HISTORY_UPDATED
    );
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
    assert_eq!(
        events.try_recv().unwrap().event_type,
        plain_rs::events::H_CLIPBOARD_HISTORY_UPDATED
    );
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

#[tokio::test]
async fn local_captures_coalesce_duplicates_and_sensitive_or_historical_changes_invalidate_pending_pushes()
 {
    let directory = tempfile::tempdir().unwrap();
    let (server, _, prefs) = start(&directory);
    let state = server.runtime_state();
    let mut changes = state.clipboard_changes.subscribe();
    prefs.set("api_permissions", json!(["CLIPBOARD"])).unwrap();
    report(
        &state,
        &json!({"text":"first","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(changes.has_changed().unwrap());
    let first = changes.borrow_and_update().clone().unwrap();
    assert!(first.eligible);
    report(
        &state,
        &json!({"text":"first","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(!changes.has_changed().unwrap());
    report(
        &state,
        &json!({"text":"first","sensitive":true}).to_string(),
    )
    .unwrap();
    let secret = changes.borrow_and_update().clone().unwrap();
    assert!(!secret.eligible && secret.sensitive);
    report(
        &state,
        &json!({"text":"second","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(changes.borrow_and_update().as_ref().unwrap().eligible);
    report(
        &state,
        &json!({"text":"first","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(!changes.borrow_and_update().as_ref().unwrap().eligible);
    prefs.set("api_permissions", json!([])).unwrap();
    report(
        &state,
        &json!({"text":"disabled","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(!changes.has_changed().unwrap());
    assert_eq!(state.db.clipboard_count_rows().unwrap(), 2);
    prefs.set("api_permissions", json!(["CLIPBOARD"])).unwrap();
    report(
        &state,
        &json!({"text":"third","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(changes.borrow_and_update().as_ref().unwrap().eligible);
    state.clipboard_changes.received("third");
    assert!(changes.has_changed().unwrap());
    assert!(!changes.borrow_and_update().as_ref().unwrap().eligible);
    report(
        &state,
        &json!({"text":"third","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(!changes.has_changed().unwrap());
    state.db.clipboard_delete_query("all:true").unwrap();
    report(
        &state,
        &json!({"text":"third","sensitive":false}).to_string(),
    )
    .unwrap();
    assert!(changes.has_changed().unwrap());
    assert!(changes.borrow_and_update().as_ref().unwrap().eligible);
    server.shutdown().await;
}
