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

#[derive(SimpleObject)]
struct MdnsActivity {
    time: Long,
    event: String,
    detail: String,
}
#[derive(SimpleObject)]
struct MdnsDebug {
    services: Vec<MdnsService>,
    activity: Vec<MdnsActivity>,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerPrewarmAdvertisement {
    short_id: String,
    aware_supported: bool,
    aware_running: bool,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryReply {
    id: async_graphql::ID,
    name: String,
    port: i32,
    device_type: String,
    version: String,
    platform: String,
    ips: Vec<String>,
    aware_supported: bool,
    aware_running: bool,
}

async fn advertisement(
    ctx: &Context<'_>,
    request: crate::services::discovery::discovery_advertisement::Request,
) -> async_graphql::Result<serde_json::Value> {
    crate::services::discovery::discovery_advertisement::execute(
        ctx.data::<ServerState>()?,
        request,
    )
    .await
    .map_err(|error| async_graphql::Error::new(error.to_string()))
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
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn mdns_debug(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsDebug> {
        let runtime = &ctx.data::<ServerState>()?.mdns;
        let services = serde_json::from_value(serde_json::to_value(runtime.browser_snapshot())?)?;
        let activity = runtime
            .browser_activity()
            .into_iter()
            .map(|entry| {
                Ok(MdnsActivity {
                    time: Long(i64::try_from(entry.time)?),
                    event: entry.event,
                    detail: entry.detail,
                })
            })
            .collect::<async_graphql::Result<Vec<_>>>()?;
        Ok(MdnsDebug { services, activity })
    }
    async fn discovery_reply(&self, ctx: &Context<'_>) -> async_graphql::Result<DiscoveryReply> {
        Ok(serde_json::from_value(
            advertisement(
                ctx,
                crate::services::discovery::discovery_advertisement::Request::Reply {},
            )
            .await?,
        )?)
    }
    async fn discovery_ble_advertisement(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<String> {
        Ok(serde_json::from_value(
            advertisement(
                ctx,
                crate::services::discovery::discovery_advertisement::Request::Ble {},
            )
            .await?,
        )?)
    }
    async fn mdns_state(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Snapshot {}).await
    }
}
#[derive(Default)]
pub struct MdnsMutation;
#[Object]
impl MdnsMutation {
    async fn acquire_discovery(
        &self,
        ctx: &Context<'_>,
        token: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        mdns_runtime::control(
            ctx.data::<ServerState>()?,
            Request::Acquire {
                token: uuid::Uuid::parse_str(&token)?,
                owner: discovery_owner(ctx)?,
            },
        )
        .await?;
        Ok(true)
    }
    async fn release_discovery(
        &self,
        ctx: &Context<'_>,
        token: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        mdns_runtime::control(
            ctx.data::<ServerState>()?,
            Request::Release {
                token: uuid::Uuid::parse_str(&token)?,
                owner: discovery_owner(ctx)?,
            },
        )
        .await?;
        Ok(true)
    }

    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn begin_ble_scan(&self, ctx: &Context<'_>) -> async_graphql::Result<async_graphql::ID> {
        Ok(ctx.data::<ServerState>()?.ble_scans.begin()?.into())
    }
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn end_ble_scan(
        &self,
        ctx: &Context<'_>,
        scan_id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        ctx.data::<ServerState>()?.ble_scans.end(&scan_id);
        Ok(true)
    }
    async fn start_mdns_receiver(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Receiver {}).await
    }
    async fn browse_mdns(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Browse {}).await
    }
    async fn publish_mdns(&self, ctx: &Context<'_>) -> async_graphql::Result<MdnsState> {
        control(ctx, Request::Publish {}).await
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

fn discovery_owner(ctx: &Context<'_>) -> async_graphql::Result<String> {
    if crate::http::auth::public_gate::is_local(ctx) {
        return Ok(ctx
            .data_opt::<crate::native::graphql::NativeRequestScope>()
            .map(|scope| format!("native:{}", scope.id()))
            .unwrap_or_else(|| "native".into()));
    }
    let client = ctx
        .data::<crate::graphql::executor::RequestClient>()?
        .client_id();
    if client.is_empty() {
        return Err("Missing discovery client".into());
    }
    Ok(mdns_runtime::remote_discovery_owner(client))
}
