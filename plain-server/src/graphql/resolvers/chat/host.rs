use super::public::{ChatItem, item};
use crate::assembly::server::ServerState;
use crate::graphql::types::ActionResult;
use crate::services::chat::chat_actions;
use async_graphql::{Context, ID, Object, SimpleObject};
use plain_rs::chat::messages::target as message_target;

#[derive(SimpleObject)]
pub struct ChatConversation {
    pub id: ID,
    pub chat: ChatItem,
}
#[derive(Default)]
pub struct ChatHostQuery;
#[Object]
impl ChatHostQuery {
    async fn chat_conversations(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ChatConversation>> {
        Ok(
            plain_rs::chat::cacher::ChatCacher::conversations(&ctx.data::<ServerState>()?.db)?
                .into_iter()
                .map(|(id, chat)| ChatConversation {
                    id: ID::from(id),
                    chat: item(&chat),
                })
                .collect(),
        )
    }
    async fn chat_records(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let state = ctx.data::<ServerState>()?;
        let value = plain_rs::db::chat_store::messages::list(
            &state.db,
            &filter(&query, offset, limit, false),
        )?;
        let rows: Vec<plain_rs::db::DChat> = serde_json::from_value(value)?;
        Ok(rows.iter().map(item).collect())
    }
    async fn chat_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        let state = ctx.data::<ServerState>()?;
        let value =
            plain_rs::db::chat_store::messages::list(&state.db, &filter(&query, 0, 0, true))?;
        Ok(i32::try_from(value.as_i64().unwrap_or_default())?)
    }
    async fn chat_ids(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<Vec<ID>> {
        Ok(
            plain_rs::db::chat_store::messages::ids(&ctx.data::<ServerState>()?.db, &query)?
                .into_iter()
                .map(ID::from)
                .collect(),
        )
    }
}
#[derive(SimpleObject)]
pub struct ChatTextEdit {
    pub changed: bool,
    pub chat: ChatItem,
}
#[derive(Default)]
pub struct ChatHostMutation;
#[Object]
impl ChatHostMutation {
    async fn send_chat_items(
        &self,
        ctx: &Context<'_>,
        targets: Vec<String>,
        content: String,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        Ok(
            chat_actions::send_many(ctx.data::<ServerState>()?, targets, content)
                .await?
                .iter()
                .map(item)
                .collect(),
        )
    }

    async fn send_share(
        &self,
        ctx: &Context<'_>,
        targets: Vec<String>,
        share_id: ID,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        Ok(crate::services::chat::share_send::existing(
            ctx.data::<ServerState>()?,
            ctx.data::<std::sync::Arc<plain_rs::files::shares::Service>>()?
                .clone(),
            targets,
            share_id.to_string(),
        )
        .await?
        .iter()
        .map(item)
        .collect())
    }

    async fn send_folder_share(
        &self,
        ctx: &Context<'_>,
        targets: Vec<String>,
        path: String,
        name: String,
        expires_at: Option<crate::graphql::types::Instant>,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        Ok(crate::services::chat::share_send::folder(
            ctx.data::<ServerState>()?,
            ctx.data::<std::sync::Arc<plain_rs::files::shares::Service>>()?
                .clone(),
            targets,
            path,
            name,
            expires_at.map(|time| time.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        )
        .await?
        .iter()
        .map(item)
        .collect())
    }

    async fn forward_chat_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        targets: Vec<String>,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        Ok(
            chat_actions::forward_many(ctx.data::<ServerState>()?, id.as_str(), targets)
                .await?
                .iter()
                .map(item)
                .collect(),
        )
    }
    async fn deliver_chat_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
        recipients: Option<Vec<ID>>,
    ) -> async_graphql::Result<ChatItem> {
        let recipients = recipients.map(|ids| ids.into_iter().map(|id| id.to_string()).collect());
        let receipt = crate::services::chat::chat_delivery::deliver(
            ctx.data::<ServerState>()?,
            id.as_str(),
            recipients,
        )
        .await?;
        Ok(item(&receipt.chat.ok_or_else(|| {
            async_graphql::Error::new("Message unavailable")
        })?))
    }
    async fn clear_chat_items(
        &self,
        ctx: &Context<'_>,
        target: String,
    ) -> async_graphql::Result<ActionResult> {
        use plain_rs::chat::attachments::app_file_store::chat_deletion::{self, Selection};
        let state = ctx.data::<ServerState>()?;
        let (peer, channel) = message_target::parse(&target)?;
        let count = chat_deletion::delete(
            &state.db,
            &state.directory,
            if channel.is_empty() {
                Selection::Peer(&peer)
            } else {
                Selection::Channel(&channel)
            },
        )?;
        let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::chat::events::WS_MESSAGE_DELETED,
            serde_json::json!(target).to_string(),
        ));
        Ok(ActionResult {
            affected_count: count as i32,
        })
    }
    async fn edit_chat_text(
        &self,
        ctx: &Context<'_>,
        id: ID,
        text: String,
    ) -> async_graphql::Result<ChatTextEdit> {
        let state = ctx.data::<ServerState>()?;
        let result =
            crate::services::link_preview::edit(&state.db, &state.directory, id.as_str(), &text)?;
        state.previews.request(id.as_str());
        if result.changed {
            chat_actions::committed(
                state,
                &result.chat,
                plain_rs::chat::events::WS_MESSAGE_UPDATED,
            );
        }
        Ok(ChatTextEdit {
            changed: result.changed,
            chat: item(&result.chat),
        })
    }
}

fn filter(
    query: &str,
    offset: i32,
    limit: i32,
    count_only: bool,
) -> plain_rs::db::chat_store::messages::Filter {
    let fields = plain_rs::utils::search_dsl::parse(query);
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.value.clone())
    };
    plain_rs::db::chat_store::messages::Filter {
        peer: field("peer"),
        channel: field("channel"),
        text: field("text").unwrap_or_default(),
        offset: i64::from(offset),
        limit: Some(i64::from(limit)),
        descending: field("sort").as_deref() == Some("desc"),
        latest: field("latest").as_deref() == Some("true"),
        count_only,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/chat/host.rs"]
mod tests;
