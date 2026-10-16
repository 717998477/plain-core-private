use crate::assembly::server::ServerState;
use crate::graphql::types::{Instant, Long};
use async_graphql::{Context, Object, SimpleObject};
use serde::Deserialize;

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatChannelRecord {
    actions: super::public::ChatChannelActions,
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
        let state = ctx.data::<ServerState>()?;
        let actor = state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
            .filter(|actor| !actor.is_empty())
            .ok_or_else(|| async_graphql::Error::new("Missing client identity"))?;
        let db = state.db.clone();
        let row =
            tokio::task::spawn_blocking(move || plain_rs::db::chat_store::channels::get(&db, &id))
                .await??;
        row.map(|row| {
            let mut value = plain_rs::chat::events::channel_to_json(&row, &actor);
            value["key"] = serde_json::Value::String(row.key);
            serde_json::from_value(value).map_err(Into::into)
        })
        .transpose()
    }
}
