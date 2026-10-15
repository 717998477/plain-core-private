use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::graphql::types::facts::id;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::optional_instant;
use crate::graphql::types::facts::rows;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use async_graphql::{Context, Object};
use chrono::{DateTime, Utc};
use plain_rs::enums::DataType;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

pub(crate) const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

pub use super::types::{MediaBucket, MediaDataType};

/// The fields every library row shares. Kept as a helper rather than a
/// GraphQL interface: the contract declares `MediaItem` as an interface, but
/// nothing in it can be queried on its own — the three concrete types each
/// add fields and are what the lists return.
pub(crate) struct Row {
    pub(crate) id: async_graphql::ID,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) size: Long,
    pub(crate) bucket_id: async_graphql::ID,
    pub(crate) created_at: Instant,
    pub(crate) updated_at: Instant,
}

pub(crate) fn row(item: &Value) -> Row {
    Row {
        id: id(item, "id"),
        title: text(item, "title"),
        path: text(item, "path"),
        size: Long(integer(item, "size")),
        bucket_id: id(item, "bucketId"),
        created_at: instant(item, "createdAt"),
        updated_at: instant(item, "updatedAt"),
    }
}

#[derive(Default)]
pub struct MediaQuery;

#[Object]
impl MediaQuery {
    async fn media_buckets(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
    ) -> async_graphql::Result<Vec<MediaBucket>> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(Vec::new());
        }
        let data_type = data_type_of(r#type.as_str())?;
        let facts = catalog(ctx)
            .buckets(data_type)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(aggregate_buckets(&facts)
            .into_iter()
            .map(Bucket::into_media)
            .collect())
    }
}

/// Groups rows into folders and orders them by the lowercased folder name,
/// the same rule plain-app's bucket bar uses. The platform reports one row
/// per item; the contract wants one row per folder.
struct Bucket {
    id: String,
    name: String,
    sort_name: String,
    item_count: i32,
    top_item_paths: Vec<String>,
}

impl Bucket {
    fn into_media(self) -> MediaBucket {
        MediaBucket {
            id: async_graphql::ID::from(self.id),
            name: self.name,
            item_count: self.item_count,
            top_item_paths: self.top_item_paths,
        }
    }
}

/// At most this many sample paths per folder — enough for a thumbnail
/// grid, and a bound so one huge folder cannot drag every path over the
/// wire.
const BUCKET_SAMPLES: usize = 4;

fn aggregate_buckets(value: &Value) -> Vec<Bucket> {
    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    for item in rows(value, |item| item.clone()) {
        let key = text(&item, "id");
        if key.is_empty() {
            continue;
        }
        let path = text(&item, "path");
        match buckets.get_mut(&key) {
            Some(existing) => {
                existing.item_count += 1;
                if existing.top_item_paths.len() < BUCKET_SAMPLES {
                    existing.top_item_paths.push(path);
                }
            }
            None => {
                buckets.insert(
                    key.clone(),
                    Bucket {
                        id: key,
                        name: text(&item, "name"),
                        sort_name: text(&item, "sortName").to_lowercase(),
                        item_count: 1,
                        top_item_paths: vec![path],
                    },
                );
            }
        }
    }
    let mut aggregated: Vec<Bucket> = buckets.into_values().collect();
    aggregated.sort_by(|a, b| a.sort_name.cmp(&b.sort_name).then_with(|| a.id.cmp(&b.id)));
    aggregated
}

/// The DSL's bare-token name filter, pulled out on its own: plain-app splits
/// `text` off the query and hands the rest to the platform as the
/// structured filter, so the platform never sees the text twice.
pub(crate) fn text_field(query: &str) -> String {
    plain_rs::utils::search_dsl::parse(query)
        .into_iter()
        .find(|field| field.name == "text")
        .map(|field| field.value)
        .unwrap_or_default()
}

/// One page of library rows, still in the host's raw JSON shape.
pub(crate) async fn media_rows(ctx: &Context<'_>, params: Value) -> async_graphql::Result<Value> {
    let request: RowsRequest = serde_json::from_value(params)?;
    Ok(Value::Array(
        catalog(ctx)
            .rows(
                request.data_type,
                &request.query,
                request.sort_by,
                request.limit,
                request.offset,
            )
            .await
            .map_err(async_graphql::Error::new)?,
    ))
}

pub(crate) async fn media_count(
    ctx: &Context<'_>,
    data_type: &str,
    query: String,
) -> async_graphql::Result<i32> {
    let count = catalog(ctx)
        .count(data_type_of(data_type)?, &query)
        .await
        .map_err(async_graphql::Error::new)?;
    i32::try_from(count).map_err(|error| async_graphql::Error::new(error.to_string()))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowsRequest {
    data_type: DataType,
    query: String,
    sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    limit: u32,
    offset: u32,
}

fn data_type_of(value: &str) -> async_graphql::Result<DataType> {
    Ok(match value {
        "AUDIO" => DataType::Audio,
        "VIDEO" => DataType::Video,
        "IMAGE" => DataType::Image,
        "DOC" => DataType::Doc,
        other => {
            return Err(async_graphql::Error::new(format!(
                "Unsupported media catalog type: {other}"
            )));
        }
    })
}

pub(crate) fn catalog(ctx: &Context<'_>) -> crate::services::files::catalog::MediaCatalog {
    crate::services::files::catalog::MediaCatalog::new(
        ctx.data_unchecked::<Arc<plain_rs::db::Db>>().clone(),
        ctx.data_unchecked::<Arc<Host>>().clone(),
    )
}

pub(crate) async fn library_rows(
    ctx: &Context<'_>,
    data_type: DataType,
    query: String,
    sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    offset: u32,
    limit: u32,
) -> async_graphql::Result<Vec<Value>> {
    catalog(ctx)
        .rows(data_type, &query, sort_by, limit, offset)
        .await
        .map_err(async_graphql::Error::new)
}

pub(crate) async fn library_keys(
    ctx: &Context<'_>,
    data_type: DataType,
    query: String,
) -> async_graphql::Result<Vec<String>> {
    catalog(ctx)
        .ids(data_type, &query, false)
        .await
        .map_err(async_graphql::Error::new)
}

pub(crate) fn instant(value: &Value, key: &str) -> Instant {
    optional_instant(value, key)
        .unwrap_or_else(|| Instant(DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default()))
}

pub(crate) fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<plain_rs::prefs::Prefs> {
    ctx.data_unchecked::<Arc<plain_rs::prefs::Prefs>>()
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/media/public.rs"]
mod tests;
