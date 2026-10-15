use crate::assembly::server::ServerState;
use crate::graphql::types::Long;
use crate::services::discovery::mdns_runtime::{self, Request};
use async_graphql::{Context, Object, SimpleObject};
use serde::Deserialize;

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdnsPacket {
    time: Long,
    direction: String,
    src_ip: String,
    src_port: i32,
    dst_ip: String,
    dst_port: i32,
    size: Long,
    summary: String,
    detail: String,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdnsService {
    service_type: String,
    instance_name: String,
    instance_fqdn: String,
    hostname: String,
    port: i32,
    txt_records: Vec<String>,
    ips: Vec<String>,
    ipv6: Vec<String>,
    complete: bool,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdnsState {
    runtime_id: String,
    packets_in: Vec<MdnsPacket>,
    packets_out: Vec<MdnsPacket>,
    revision: Long,
    receiver: bool,
    running: bool,
    scanning: bool,
    published: bool,
    manual_scanning: bool,
    debug_session_count: i32,
    capturing: bool,
    services: Vec<MdnsService>,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerPrewarmAdvertisement {
    short_id: String,
    aware_supported: bool,
    aware_running: bool,
}

async fn control(ctx: &Context<'_>, request: Request) -> async_graphql::Result<MdnsState> {
    let value = mdns_runtime::control(ctx.data::<ServerState>()?, request)
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(serde_json::from_value(value)?)
}
#[derive(Default)]
pub struct MdnsQuery;
#[Object]
impl MdnsQuery {
    async fn mdns_state(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Snapshot {}).await
    }
}
#[derive(Default)]
pub struct MdnsMutation;
#[Object]
impl MdnsMutation {
    async fn start_mdns_receiver(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Receiver {}).await
    }
    async fn start_mdns_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Start {}).await
    }
    async fn stop_mdns_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Stop {}).await
    }
    async fn browse_mdns(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Browse {}).await
    }
    async fn restart_mdns_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Restart {}).await
    }
    async fn publish_mdns(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Publish {}).await
    }
    async fn update_mdns_publication(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Update {}).await
    }
    async fn unpublish_mdns(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Unpublish {}).await
    }
    async fn set_mdns_capture(
        &self,
        ctx: &Context<'_>,
        enabled: bool,
    ) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Capture { enabled }).await
    }
    async fn acquire_mdns_debug_session(
        &self,
        ctx: &Context<'_>,
        token: String,
    ) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::DebugStart { token }).await
    }
    async fn release_mdns_debug_session(
        &self,
        ctx: &Context<'_>,
        token: String,
    ) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::DebugStop { token }).await
    }
    async fn prewarm_peer_transport(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<PeerPrewarmAdvertisement>> {
        let result = crate::services::chat::prewarm::execute(ctx.data::<ServerState>()?, &id)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        result
            .map(|value| serde_json::from_value(serde_json::to_value(value)?).map_err(Into::into))
            .transpose()
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/discovery/mdns.rs"]
mod tests;
