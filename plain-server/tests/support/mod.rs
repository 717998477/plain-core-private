pub(crate) async fn graphql_http(port: u16, token: &str, query: &str) -> serde_json::Value {
    graphql_http_variables(port, token, query, serde_json::json!({})).await
}

pub(crate) async fn graphql_http_variables(
    port: u16,
    token: &str,
    query: &str,
    variables: serde_json::Value,
) -> serde_json::Value {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(serde_json::json!({"query":query,"variables":variables}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}
