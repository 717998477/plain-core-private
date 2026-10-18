use crate::assembly::server::ServerState;
use crate::services::casting::receiver::control::{self, Request};
use async_graphql::{Context, Enum, InputObject, Json, Object};
use serde::Serialize;
use serde_json::Value;

#[derive(Enum, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DlnaReceiverAction {
    Retry,
    RemoveRule,
    Accept,
    Reject,
}

#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlnaReceiverInput {
    action: DlnaReceiverAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remember: Option<bool>,
}

async fn execute(state: &ServerState, request: Request) -> async_graphql::Result<Json<Value>> {
    let mut stop = state.stop.clone();
    if *stop.borrow() {
        return Err("Core stopped".into());
    }
    tokio::select! {
        biased;
        _ = stop.changed() => Err("Core stopped".into()),
        result = control::execute(state, request) => result.map(Json).map_err(async_graphql::Error::new),
    }
}

#[derive(Default)]
pub struct DlnaReceiverQuery;
#[Object]
impl DlnaReceiverQuery {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn dlna_receiver(&self, ctx: &Context<'_>) -> async_graphql::Result<Json<Value>> {
        let state = ctx.data::<ServerState>()?;
        let mut snapshot = execute(state, Request::Snapshot {}).await?;
        snapshot.0["enabled"] = Value::Bool(plain_rs::prefs::dlna::enabled(&state.prefs));
        snapshot.0["rules"] = execute(state, Request::Rules {}).await?.0;
        Ok(snapshot)
    }
}

#[derive(Default)]
pub struct DlnaReceiverMutation;
#[Object]
impl DlnaReceiverMutation {
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn control_dlna_receiver(
        &self,
        ctx: &Context<'_>,
        input: DlnaReceiverInput,
    ) -> async_graphql::Result<Json<Value>> {
        let request: Request = serde_json::from_value(serde_json::to_value(input)?)?;
        execute(ctx.data::<ServerState>()?, request).await
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/casting/receiver.rs"]
mod tests;
