use crate::assembly::server::ServerState;
use async_graphql::{Context, Object, SimpleObject};
use plain_rs::transport::router::TransportType;

#[derive(SimpleObject)]
struct ActivePeerTransfer {
    peer_id: async_graphql::ID,
    transport: TransportType,
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
}
