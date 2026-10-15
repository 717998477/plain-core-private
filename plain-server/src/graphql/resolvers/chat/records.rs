use crate::assembly::server::ServerState;
use crate::graphql::types::{Instant, Long};
use async_graphql::{Context, Object, SimpleObject};
use serde::Deserialize;

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatChannelRecord {
    id: async_graphql::ID,
    name: String,
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    key: String,
    owner_id: async_graphql::ID,
    members: Vec<super::public::ChatChannelMember>,
    version: Long,
    status: String,
    created_at: Instant,
    updated_at: Instant,
}
#[derive(Default)]
pub struct ChatChannelRecordQuery;
#[Object]
impl ChatChannelRecordQuery {
    async fn chat_channel_record(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<ChatChannelRecord>> {
        let db = ctx.data::<ServerState>()?.db.clone();
        let row =
            tokio::task::spawn_blocking(move || plain_rs::db::chat_store::channels::get(&db, &id))
                .await??;
        row.map(|row| {
            let mut value = plain_rs::chat::events::channel_to_json(&row);
            value["key"] = serde_json::Value::String(row.key);
            serde_json::from_value(value).map_err(Into::into)
        })
        .transpose()
    }
}
