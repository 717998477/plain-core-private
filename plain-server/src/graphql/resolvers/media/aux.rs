use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::{Context, ID, InputObject, Object, Result, SimpleObject};
use plain_rs::db::Db;
use plain_rs::enums::DataType;
use plain_rs::media::duration_cache::{Duration, DurationCache};
use plain_rs::media::images::embeddings;
use std::sync::Arc;
#[derive(SimpleObject)]
struct MediaHostDuration {
    r#type: DataType,
    media_id: ID,
    duration_ms: Long,
    path: String,
    size_bytes: Long,
    modified_at: Instant,
    updated_at: Instant,
}
fn duration(row: Duration) -> Result<MediaHostDuration> {
    let kind = match row.media_type.as_str() {
        "audio" => DataType::Audio,
        "video" => DataType::Video,
        _ => return Err("invalid stored media type".into()),
    };
    Ok(MediaHostDuration {
        r#type: kind,
        media_id: row.media_id.into(),
        duration_ms: Long(row.duration_ms),
        path: row.path,
        size_bytes: Long(row.size_bytes),
        modified_at: crate::graphql::resolvers::common::content_common::instant(&row.modified_at)?,
        updated_at: crate::graphql::resolvers::common::content_common::instant(&row.updated_at)?,
    })
}
#[derive(InputObject)]
struct ImageHostEmbeddingInput {
    id: ID,
    path: String,
    embedding_base64: String,
}
impl From<ImageHostEmbeddingInput> for embeddings::EmbeddingInput {
    fn from(i: ImageHostEmbeddingInput) -> Self {
        Self {
            id: i.id.to_string(),
            path: i.path,
            embedding_base64: i.embedding_base64,
        }
    }
}
#[derive(SimpleObject)]
struct ImageHostSearchResult {
    image_id: ID,
    score: f32,
}
#[derive(Default)]
pub struct MediaAuxQuery;
#[Object]
impl MediaAuxQuery {
    async fn media_durations(&self, ctx: &Context<'_>) -> Result<Vec<MediaHostDuration>> {
        ctx.data::<Arc<DurationCache>>()?
            .all()?
            .into_iter()
            .map(duration)
            .collect()
    }
    async fn image_embedding_ids(&self, ctx: &Context<'_>) -> Result<Vec<ID>> {
        Ok(embeddings::ids(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(ID)
            .collect())
    }
    async fn image_embedding_count(&self, ctx: &Context<'_>) -> Result<Long> {
        Ok(Long(embeddings::count(ctx.data::<Arc<Db>>()?)?))
    }
    async fn search_image_embeddings(
        &self,
        ctx: &Context<'_>,
        embedding_base64: String,
        max_results: i32,
    ) -> Result<Vec<ImageHostSearchResult>> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let max_results = usize::try_from(max_results)?;
        let rows = tokio::task::spawn_blocking(move || {
            embeddings::search(&db, &embedding_base64, max_results)
        })
        .await??;
        Ok(rows
            .into_iter()
            .map(|r| ImageHostSearchResult {
                image_id: r.image_id.into(),
                score: r.score,
            })
            .collect())
    }
}
#[derive(Default)]
pub struct MediaAuxMutation;
#[Object]
impl MediaAuxMutation {
    async fn remove_media_durations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        media_ids: Vec<ID>,
    ) -> Result<Long> {
        Ok(Long(
            ctx.data::<Arc<DurationCache>>()?
                .remove(
                    r#type.media_type_str().ok_or("invalid media type")?,
                    &media_ids
                        .into_iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>(),
                )?
                .try_into()?,
        ))
    }
    async fn save_image_embeddings(
        &self,
        ctx: &Context<'_>,
        items: Vec<ImageHostEmbeddingInput>,
    ) -> Result<bool> {
        embeddings::save(
            ctx.data::<Arc<Db>>()?,
            &items.into_iter().map(Into::into).collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn remove_image_embeddings(&self, ctx: &Context<'_>, ids: Vec<ID>) -> Result<Long> {
        Ok(Long(
            ctx.data::<Arc<crate::services::images::index::ImageIndex>>()?
                .remove(&ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>())?
                .try_into()?,
        ))
    }
    async fn clear_image_embeddings(&self, ctx: &Context<'_>) -> Result<Long> {
        Ok(Long(
            ctx.data::<Arc<crate::services::images::index::ImageIndex>>()?
                .clear()?
                .try_into()?,
        ))
    }
}
