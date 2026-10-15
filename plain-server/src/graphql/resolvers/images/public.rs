use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::id;
use crate::graphql::types::facts::optional_instant;
use crate::graphql::types::facts::rows;
use crate::http::auth::public_gate;
use async_graphql::{Context, Enum, Object, SimpleObject};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc};

use crate::graphql::resolvers::media::public::STORAGE;
use crate::graphql::resolvers::media::public::prefs;
use crate::graphql::resolvers::media::public::row;
use crate::graphql::resolvers::media::public::text_field;

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Image {
    pub id: async_graphql::ID,
    pub title: String,
    pub path: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}

#[async_graphql::ComplexObject]
impl Image {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Image,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(Default)]
pub struct ImagesQuery;

#[Object]
impl ImagesQuery {
    async fn image_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        crate::services::images::search::count(
            &crate::graphql::resolvers::media::public::catalog(ctx),
            ctx.data_unchecked::<Arc<crate::services::images::models::Runtime>>(),
            &text_field(&query),
            &query,
        )
        .await
        .map_err(async_graphql::Error::new)
    }

    async fn images(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    ) -> async_graphql::Result<Vec<Image>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let items = Value::Array(
            crate::services::images::search::rows(
                &crate::graphql::resolvers::media::public::catalog(ctx),
                ctx.data_unchecked::<Arc<crate::services::images::models::Runtime>>(),
                &text_field(&query),
                &query,
                offset,
                limit,
                sort_by.as_str(),
            )
            .await
            .map_err(async_graphql::Error::new)?,
        );
        Ok(rows(&items, |item| {
            let row = row(item);
            Image {
                id: row.id,
                title: row.title,
                path: row.path,
                size: row.size,
                bucket_id: row.bucket_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                taken_at: optional_instant(item, "takenAt"),
                is_favorite: flag(item, "isFavorite"),
            }
        }))
    }
}
