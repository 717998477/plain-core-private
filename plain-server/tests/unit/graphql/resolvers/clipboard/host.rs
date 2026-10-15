use crate::assembly::server::ContentServer;
use serde_json::{Value, json};
use std::sync::Arc;

async fn call(port: u16, token: &str, query: &str) -> Value {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query":query}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}

#[tokio::test]
async fn native_record_http_search_delete_and_restart_share_one_clipboard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = plain_rs::utils::base64::base64_encode(&[9; 32]);
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let mut socket = server.runtime_state().events.subscribe();
    async fn event(
        socket: &mut tokio::sync::broadcast::Receiver<plain_rs::events::WsEvent>,
    ) -> serde_json::Value {
        let event = socket.recv().await.unwrap();
        serde_json::json!({"type":event.event_type,"payload":event.payload})
    }
    let query = r#"mutation {recordClipboard(input:{text:"100%_ 🌍",source:"peer",label:"label",sensitive:true}) {inserted item {id text source label sensitive createdAt}}}"#;
    let first = serde_json::to_value(
        server
            .graphql_executor()
            .execute(async_graphql::Request::new(query))
            .await,
    )
    .unwrap();
    assert!(first.get("errors").is_none(), "{first}");
    assert_eq!(first["data"]["recordClipboard"]["inserted"], true);
    let changed = tokio::time::timeout(std::time::Duration::from_secs(2), event(&mut socket))
        .await
        .unwrap();
    assert_eq!(changed["type"], "CONTENT_CHANGED");
    assert_eq!(changed["payload"], "{}");
    drop(socket);

    let item = &first["data"]["recordClipboard"]["item"];
    assert_eq!(item["source"], "peer");
    assert_eq!(item["sensitive"], true);
    assert!(chrono::DateTime::parse_from_rfc3339(item["createdAt"].as_str().unwrap()).is_ok());
    assert_eq!(
        call(server.port, &token, query).await["data"]["recordClipboard"]["inserted"],
        false
    );
    let page = call(
        server.port,
        &token,
        r#"{clipboardItems(offset:0,limit:50,query:"%_") {id} clipboardItemCount(query:"%_")}"#,
    )
    .await;
    assert_eq!(page["data"]["clipboardItemCount"], 1);
    assert_eq!(page["data"]["clipboardItems"][0]["id"], item["id"]);
    for invalid in [
        r#"mutation {recordClipboard(input:{text:" ",source:"",label:"",sensitive:false}) {inserted}}"#,
        r#"mutation {deleteClipboardItems(query:" ") {affectedCount}}"#,
        r#"mutation {deleteClipboardItems(query:"source:peer") {affectedCount}}"#,
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
            "mutation {deleteClipboardItemsByIds(ids:[]) {affectedCount}}"
        )
        .await["data"]["deleteClipboardItemsByIds"]["affectedCount"],
        0
    );
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
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
