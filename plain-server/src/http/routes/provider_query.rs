use crate::assembly::server::ServerState;
use crate::host::query::Provider;
use crate::host::query::prepare;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Request {
    provider: Provider,
    query: String,
    #[serde(default)]
    resolved_parent_id: Option<String>,
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(
        prepare(
            &state.db,
            &state.host,
            request.provider,
            &request.query,
            request.resolved_parent_id.as_deref(),
        )
        .await?,
    )?)
}
pub(crate) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
