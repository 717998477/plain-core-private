use super::{media_catalog, media_catalog_facts};
use crate::graphql::resolvers::files::types::FileSortBy;
use crate::host::bridge::Host;
use plain_rs::{db::Db, enums::DataType};
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) enum MediaCatalog {
    Native(Arc<Db>),
    Platform(Arc<Host>),
}

impl MediaCatalog {
    pub(crate) fn new(db: Arc<Db>, host: Arc<Host>) -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            Self::Platform(host)
        } else {
            Self::Native(db)
        }
    }

    pub(crate) async fn rows(
        &self,
        kind: DataType,
        query: &str,
        sort: FileSortBy,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Value>, String> {
        match self {
            Self::Platform(host) => {
                let facts = host.call(
                    "systemMediaRows",
                    json!({"dataType":kind,"query":query,"sortBy":sort.as_str(),"limit":limit,"offset":offset}),
                ).await?;
                serde_json::from_value(facts).map_err(|error| error.to_string())
            }
            Self::Native(db) => {
                let db = db.clone();
                let query = query.to_owned();
                tokio::task::spawn_blocking(move || {
                    media_catalog::rows(&db, kind, &query, sort, limit, offset)
                        .map_err(|error| error.to_string())?
                        .iter()
                        .map(media_catalog_facts::row_facts)
                        .collect()
                })
                .await
                .map_err(|error| error.to_string())?
            }
        }
    }

    pub(crate) async fn count(&self, kind: DataType, query: &str) -> Result<i64, String> {
        match self {
            Self::Platform(host) => host
                .call("systemMediaCount", json!({"dataType":kind,"query":query}))
                .await?
                .as_i64()
                .ok_or_else(|| "Invalid media count".to_owned()),
            Self::Native(db) => {
                let db = db.clone();
                let query = query.to_owned();
                tokio::task::spawn_blocking(move || {
                    media_catalog::count(&db, kind, &query).map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())?
            }
        }
    }

    pub(crate) async fn ids(
        &self,
        kind: DataType,
        query: &str,
        from_trash: bool,
    ) -> Result<Vec<String>, String> {
        match self {
            Self::Platform(host) => serde_json::from_value(
                host.call(
                    "systemMediaIdsFacts",
                    json!({"dataType":kind,"query":query,"fromTrash":from_trash}),
                )
                .await?,
            )
            .map_err(|error| error.to_string()),
            Self::Native(db) => {
                let db = db.clone();
                let query = query.to_owned();
                tokio::task::spawn_blocking(move || {
                    media_catalog::ids(&db, kind, &query, from_trash)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())?
            }
        }
    }

    pub(crate) async fn buckets(&self, kind: DataType) -> Result<Value, String> {
        match self {
            Self::Platform(host) => {
                host.call("systemMediaBucketItemFacts", json!({"dataType":kind}))
                    .await
            }
            Self::Native(db) => {
                let db = db.clone();
                tokio::task::spawn_blocking(move || {
                    media_catalog::bucket_item_facts(&db, kind)
                        .map(Value::Array)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())?
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/catalog.rs"]
mod tests;
