use super::client_inputs::{SharedEntryInput, SharedLinkInput, SharedMessageCardInput};
use super::client_models::{SharedBrowseResult, SharedLink, SharedLinkInfo};
use crate::assembly::{server::ServerState, services::Services};
use crate::services::peers::shared_client;
use async_graphql::{Context, Object};
use plain_rs::{chat::messages::share_card, files::shares::client};

fn error(value: anyhow::Error) -> async_graphql::Error {
    async_graphql::Error::new(value.to_string())
}

#[derive(Default)]
pub struct SharedClientQuery;
#[Object]
impl SharedClientQuery {
    async fn share_link(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        host: Option<String>,
    ) -> async_graphql::Result<SharedLink> {
        Ok(
            shared_client::own_link(ctx.data::<ServerState>()?, &id, host)
                .await
                .map_err(error)?
                .into(),
        )
    }
    async fn initial_shared_link(
        &self,
        ctx: &Context<'_>,
        message_id: async_graphql::ID,
        expected: SharedMessageCardInput,
    ) -> async_graphql::Result<SharedLink> {
        let expected: share_card::Card = expected.try_into()?;
        Ok(
            shared_client::initial_link(ctx.data::<ServerState>()?, &message_id, &expected)
                .await
                .map_err(error)?
                .into(),
        )
    }
    async fn shared_link_info(
        &self,
        ctx: &Context<'_>,
        link: SharedLinkInput,
        virtual_path: Option<String>,
    ) -> async_graphql::Result<SharedLinkInfo> {
        let link: client::Link = link.try_into()?;
        shared_client::info(ctx.data::<ServerState>()?, &link, virtual_path.as_deref())
            .await
            .map_err(error)?
            .try_into()
    }
    async fn shared_file_url(
        &self,
        ctx: &Context<'_>,
        link: SharedLinkInput,
        url_token: String,
        virtual_path: String,
        #[graphql(default)] zip: bool,
    ) -> async_graphql::Result<String> {
        let link: client::Link = link.try_into()?;
        shared_client::file_url(
            ctx.data::<ServerState>()?,
            &link,
            &url_token,
            &virtual_path,
            zip,
        )
        .await
        .map_err(error)
    }
}
#[derive(Default)]
pub struct SharedClientMutation;
#[Object]
impl SharedClientMutation {
    async fn cache_shared_file(
        &self,
        ctx: &Context<'_>,
        link: SharedLinkInput,
        url_token: String,
        entry: SharedEntryInput,
        thumbnail: bool,
    ) -> async_graphql::Result<String> {
        shared_client::cache_file(
            ctx.data::<ServerState>()?,
            &ctx.data::<Services>()?.remote_cache,
            link.try_into()?,
            url_token,
            entry.into(),
            thumbnail,
        )
        .await
        .map_err(error)
    }
    async fn browse_shared_link(
        &self,
        ctx: &Context<'_>,
        message_id: async_graphql::ID,
        expected: SharedMessageCardInput,
        virtual_path: Option<String>,
    ) -> async_graphql::Result<Option<SharedBrowseResult>> {
        let expected: share_card::Card = expected.try_into()?;
        shared_client::browse(
            ctx.data::<ServerState>()?,
            &message_id,
            &expected,
            virtual_path.as_deref(),
        )
        .await
        .map_err(error)?
        .map(TryInto::try_into)
        .transpose()
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/shares/client.rs"]
mod tests;
