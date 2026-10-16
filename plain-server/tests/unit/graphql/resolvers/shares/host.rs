use crate::assembly::server::ContentServer;
use async_graphql::Request;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn main_share_content_uses_saved_roots_identity_and_live_port() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set(
            "master_secret",
            plain_rs::utils::base64::base64_encode(&[11; 32]),
        )
        .unwrap();
    let server = ContentServer::start(
        dir.path(),
        &plain_rs::utils::base64::base64_encode(&[7; 32]),
        prefs,
    )
    .unwrap();
    let file = dir.path().join("shared.txt");
    std::fs::write(&file, b"abc").unwrap();
    let (_generation, mut requests) = server.runtime_state().host.connect();
    let executor = server.graphql_executor();
    let result = executor.execute(Request::new(format!(
        "mutation {{ createShare(name: \"A share\", realPaths: [{}], readOnly: true) {{ id }} }}", json!(file.to_str().unwrap())
    ))).await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let id = data["createShare"]["id"].as_str().unwrap();
    let result = executor
        .execute(Request::new(format!(
            "{{ shareMessageContent(id: {}) shareToken(id: {}) }}",
            json!(id),
            json!(id)
        )))
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let data = result.data.into_json().unwrap();
    let message: Value =
        serde_json::from_str(data["shareMessageContent"].as_str().unwrap()).unwrap();
    assert_eq!(message["type"], "SHARE");
    assert_eq!(message["value"]["shareId"], id);
    assert_eq!(message["value"]["urlToken"], data["shareToken"]);
    assert_eq!(message["value"]["name"], "A share");
    assert_eq!(message["value"]["itemCount"], 1);
    assert_eq!(message["value"]["totalSize"], 3);
    let state = server.runtime_state();
    assert_eq!(
        message["value"]["peerInfo"]["id"],
        state.native_client_id().unwrap()
    );
    assert_eq!(
        message["value"]["peerInfo"]["port"],
        state.public_ports().await.map(|ports| ports.1).unwrap_or(0)
    );
    assert!(requests.try_recv().is_err());
    server.shutdown().await;
}
