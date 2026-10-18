use crate::assembly::server::ServerState;
use crate::graphql::types::{Instant, Long};
use crate::services::peers::shared_client::{self, Request};
use async_graphql::{Context, Enum, InputObject, Object, SimpleObject};
use plain_rs::chat::messages::share_card;
use plain_rs::files::shares::{batch_plan, client};
use serde::{Deserialize, Serialize};

pub(super) fn domain<T: serde::de::DeserializeOwned>(
    value: impl Serialize,
) -> async_graphql::Result<T> {
    Ok(serde_json::from_value(serde_json::to_value(value)?)?)
}
async fn execute<T: serde::de::DeserializeOwned>(
    ctx: &Context<'_>,
    request: Request,
) -> async_graphql::Result<T> {
    let result = shared_client::execute(ctx.data::<ServerState>()?, request)
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(serde_json::from_value(result)?)
}

#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SharedLinkInput {
    host: String,
    port: i32,
    shared_id: async_graphql::ID,
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_url: Option<String>,
}

#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedPeerAddressInput {
    id: async_graphql::ID,
    ip: String,
    port: i32,
}

#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
struct SharedMessageCardInput {
    share_id: async_graphql::ID,
    url_token: String,
    peer_info: SharedPeerAddressInput,
    name: String,
    item_count: Long,
    total_size: Long,
    expires_at: Option<Instant>,
}

#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SharedEntryInput {
    name: String,
    virtual_path: String,
    is_dir: bool,
    size: Long,
    mime_type: String,
    has_thumb: bool,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SharedLink {
    host: String,
    port: i32,
    shared_id: async_graphql::ID,
    token: String,
    page_url: String,
}

#[derive(SimpleObject)]
struct SharedPeerAddress {
    id: async_graphql::ID,
    ip: String,
    port: i32,
}

#[derive(SimpleObject)]
struct SharedMessageCard {
    share_id: async_graphql::ID,
    url_token: String,
    peer_info: SharedPeerAddress,
    name: String,
    item_count: Long,
    total_size: Long,
    expires_at: Option<Instant>,
}

#[derive(SimpleObject, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SharedEntry {
    name: String,
    virtual_path: String,
    is_dir: bool,
    size: Long,
    mime_type: String,
    has_thumb: bool,
}

#[derive(SimpleObject)]
struct SharedLinkInfo {
    name: String,
    read_only: bool,
    requires_password: bool,
    expires_at: Option<Instant>,
    url_token: String,
    entries: Vec<SharedEntry>,
}

#[derive(SimpleObject)]
struct SharedBrowseResult {
    info: SharedLinkInfo,
    link: SharedLink,
    card: SharedMessageCard,
}

impl From<client::Link> for SharedLink {
    fn from(row: client::Link) -> Self {
        Self {
            host: row.host,
            port: i32::from(row.port),
            shared_id: row.shared_id.into(),
            token: row.token,
            page_url: row.page_url,
        }
    }
}

impl From<share_card::Peer> for SharedPeerAddress {
    fn from(row: share_card::Peer) -> Self {
        Self {
            id: row.id.into(),
            ip: row.ip,
            port: i32::from(row.port),
        }
    }
}

impl From<share_card::Card> for SharedMessageCard {
    fn from(row: share_card::Card) -> Self {
        Self {
            share_id: row.share_id.into(),
            url_token: row.url_token,
            peer_info: row.peer_info.into(),
            name: row.name,
            item_count: Long(row.item_count),
            total_size: Long(row.total_size),
            expires_at: row.expires_at.map(Instant),
        }
    }
}

impl From<client::File> for SharedEntry {
    fn from(row: client::File) -> Self {
        Self {
            name: row.name,
            virtual_path: row.virtual_path,
            is_dir: row.is_dir,
            size: Long(row.size),
            mime_type: row.mime_type,
            has_thumb: row.has_thumb,
        }
    }
}

impl TryFrom<client::Info> for SharedLinkInfo {
    type Error = async_graphql::Error;
    fn try_from(row: client::Info) -> async_graphql::Result<Self> {
        Ok(Self {
            name: row.name,
            read_only: row.read_only,
            requires_password: row.requires_password,
            expires_at: row
                .expires_at
                .map(|value| {
                    chrono::DateTime::from_timestamp_millis(value)
                        .map(Instant)
                        .ok_or_else(|| async_graphql::Error::new("Invalid share expiry"))
                })
                .transpose()?,
            url_token: row.url_token,
            entries: row.entries.into_iter().map(Into::into).collect(),
        })
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum SharedDownloadKind {
    File,
    Sync,
    Multi,
    Zip,
}
impl From<SharedDownloadKind> for batch_plan::Kind {
    fn from(value: SharedDownloadKind) -> Self {
        match value {
            SharedDownloadKind::File => Self::File,
            SharedDownloadKind::Sync => Self::Sync,
            SharedDownloadKind::Multi => Self::Multi,
            SharedDownloadKind::Zip => Self::Zip,
        }
    }
}
#[derive(Deserialize)]
struct BrowseReceipt {
    info: client::Info,
    link: client::Link,
    card: share_card::Card,
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
        Ok(execute::<client::Link>(
            ctx,
            Request::OwnLink {
                id: id.to_string(),
                host,
            },
        )
        .await?
        .into())
    }
    async fn initial_shared_link(
        &self,
        ctx: &Context<'_>,
        message_id: async_graphql::ID,
        expected: SharedMessageCardInput,
    ) -> async_graphql::Result<SharedLink> {
        Ok(execute::<client::Link>(
            ctx,
            Request::InitialLink {
                message_id: message_id.to_string(),
                expected: domain(expected)?,
            },
        )
        .await?
        .into())
    }
    async fn shared_link_info(
        &self,
        ctx: &Context<'_>,
        link: SharedLinkInput,
        virtual_path: Option<String>,
    ) -> async_graphql::Result<SharedLinkInfo> {
        Ok(execute::<client::Info>(
            ctx,
            Request::Fetch {
                link: domain(link)?,
                virtual_path,
            },
        )
        .await?
        .try_into()?)
    }
    async fn shared_file_url(
        &self,
        ctx: &Context<'_>,
        link: SharedLinkInput,
        url_token: String,
        virtual_path: String,
        #[graphql(default)] zip: bool,
    ) -> async_graphql::Result<String> {
        execute(
            ctx,
            Request::FileUrl {
                link: domain(link)?,
                url_token,
                virtual_path,
                zip,
            },
        )
        .await
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
        let link: client::Link = domain(link)?;
        if entry.is_dir {
            return Err("Shared preview requires a file".into());
        }
        let mut url = link.file_url(&url_token, &entry.virtual_path, false)?;
        let (name, key) = if thumbnail {
            url.push_str("&w=200&h=200");
            ("thumbnail".to_owned(), Some(serde_json::json!({"host":link.host,"port":link.port,"shareId":link.shared_id,"token":link.token,"urlToken":url_token,"path":entry.virtual_path,"size":entry.size.0}).to_string()))
        } else {
            (entry.name, None)
        };
        let state = ctx.data::<ServerState>()?;
        ctx.data::<crate::assembly::services::Services>()?
            .remote_cache
            .download(
                &url,
                &name,
                key.as_deref(),
                state.stop.clone(),
                Some(&state.lan.client),
            )
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn browse_shared_link(
        &self,
        ctx: &Context<'_>,
        message_id: async_graphql::ID,
        expected: SharedMessageCardInput,
        virtual_path: Option<String>,
    ) -> async_graphql::Result<Option<SharedBrowseResult>> {
        Ok(execute::<Option<BrowseReceipt>>(
            ctx,
            Request::Browse {
                message_id: message_id.to_string(),
                expected: domain(expected)?,
                virtual_path,
            },
        )
        .await?
        .map(|receipt| -> async_graphql::Result<SharedBrowseResult> {
            Ok(SharedBrowseResult {
                info: receipt.info.try_into()?,
                link: receipt.link.into(),
                card: receipt.card.into(),
            })
        })
        .transpose()?)
    }
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/shares/client.rs"]
mod tests;
