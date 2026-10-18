use crate::assembly::server::{ContentServer, ServerState};
use crate::http::routes::graphql;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use plain_rs::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn device() -> (tempfile::TempDir, ContentServer, HeaderMap) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    let server =
        ContentServer::start(dir.path(), &plain_rs::base64_encode(&[3; 32]), prefs).unwrap();
    let session = serde_json::to_value(
        crate::services::sessions::create(&server.runtime_state(), "Controller".into()).unwrap(),
    )
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "c-id",
        session["client_id"].as_str().unwrap().parse().unwrap(),
    );
    headers.insert(
        "authorization",
        format!("Bearer {}", session["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    (dir, server, headers)
}
async fn call(state: ServerState, headers: &HeaderMap, document: &str) -> (StatusCode, Value) {
    let response = graphql::call(
        State(state),
        headers.clone(),
        axum::body::Bytes::from(json!({"query":document}).to_string()),
    )
    .await;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
#[tokio::test]
async fn remote_settings_authentication_device_isolation_and_disable_reply() {
    let (_a, first, headers) = device();
    let (_b, second, other) = device();
    let (status, result) = call(first.runtime_state(), &headers, r#"mutation{font:setUserPref(key:"editor_font_size",value:20) interval:setUserPref(key:"feed_auto_refresh_interval",value:3600)}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(result["data"]["font"], true);
    let (_, result) = call(first.runtime_state(), &headers, "{userPrefs}").await;
    assert_eq!(result["data"]["userPrefs"]["editor_font_size"], 20);
    assert_eq!(
        result["data"]["userPrefs"]["feed_auto_refresh_interval"],
        3600
    );
    let (_, result) = call(second.runtime_state(), &other, "{userPrefs} ").await;
    assert!(result["data"]["userPrefs"]["editor_font_size"].is_null());
    let (_, result) = call(first.runtime_state(), &headers, "{systemPrefs}").await;
    assert!(
        result["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown field")
    );
    assert_eq!(
        call(second.runtime_state(), &headers, "{userPrefs}")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, result) = call(
        first.runtime_state(),
        &headers,
        r#"mutation{setUserPref(key:"desktop_access",value:false)}"#,
    )
    .await;
    assert_eq!(result["data"]["setUserPref"], true);
    assert_eq!(
        call(first.runtime_state(), &headers, "{userPrefs}").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(second.runtime_state(), &other, "{userPrefs}").await.0,
        StatusCode::OK
    );
    let local = reqwest::Client::new().post(format!("http://127.0.0.1:{}/graphql",first.port))
        .bearer_auth(plain_rs::base64_encode(&[3;32])).header("content-type","application/json")
        .body(json!({"query":"mutation($key: String!, $value: JSON!) {setSystemPref(key:$key,value:$value) {affectedCount}}", "variables":{"key":"sms_send_test","value":"result"}}).to_string())
        .send().await.unwrap().text().await.unwrap();
    let local: Value = serde_json::from_str(&local).unwrap();
    assert!(local.get("errors").is_none(), "{local}");
    let snapshot = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/graphql", first.port))
        .bearer_auth(plain_rs::base64_encode(&[3; 32]))
        .header("content-type", "application/json")
        .body(json!({"query":"{userPrefs}"}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(
        first
            .native_management()
            .unwrap()
            .system_preferences()
            .unwrap()["sms_send_test"],
        "result"
    );
    assert_eq!(snapshot["data"]["userPrefs"]["desktop_access"], false);
    first.shutdown().await;
    second.shutdown().await;
}
