use crate::assembly::server::ServerState;
use async_graphql::{Context, Object, SimpleObject};
use plain_rs::transport::router::TransportType;

#[derive(SimpleObject)]
struct ActivePeerTransfer {
    peer_id: async_graphql::ID,
    transport: TransportType,
}
#[derive(SimpleObject)]
struct PeerAwareConfig {
    pmk: Option<String>,
    is_client: bool,
    local_port: i32,
}
#[derive(Default)]
pub struct PeerTransportQuery;
#[Object]
impl PeerTransportQuery {
    async fn active_peer_transfers(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ActivePeerTransfer>> {
        let mut active = ctx
            .data::<ServerState>()?
            .transport
            .active()
            .into_iter()
            .map(|(peer_id, transport)| ActivePeerTransfer {
                peer_id: peer_id.into(),
                transport,
            })
            .collect::<Vec<_>>();
        active.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        Ok(active)
    }
    async fn peer_aware_config(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<PeerAwareConfig> {
        let config =
            crate::services::peers::peer_transport::aware_config(ctx.data::<ServerState>()?, &id)
                .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(PeerAwareConfig {
            pmk: config.pmk,
            is_client: config.is_client,
            local_port: i32::from(config.local_port),
        })
    }
}
