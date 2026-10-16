use crate::graphql::resolvers::images::public_index::{ImageSearchStatus, ImageSearchStatusType};
use crate::graphql::types::Long;
use crate::services::images::models::Runtime;
use async_graphql::{Context, Object, Result, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject)]
struct ImageModelsState {
    version: Long,
    status: ImageSearchStatusType,
    download_progress: i32,
    error_message: String,
    model_size: Long,
    model_dir: String,
    is_indexing: bool,
    total_images: i32,
    indexed_images: i32,
}
impl TryFrom<crate::services::images::models::Snapshot> for ImageModelsState {
    type Error = async_graphql::Error;
    fn try_from(snapshot: crate::services::images::models::Snapshot) -> Result<Self> {
        let ImageSearchStatus {
            status,
            download_progress,
            error_message,
            model_size,
            model_dir,
            is_indexing,
            total_images,
            indexed_images,
        } = snapshot.status;
        Ok(Self {
            version: Long(snapshot.version.try_into()?),
            status,
            download_progress,
            error_message,
            model_size,
            model_dir,
            is_indexing,
            total_images,
            indexed_images,
        })
    }
}
#[derive(SimpleObject)]
struct SemanticImageMatch {
    image_id: async_graphql::ID,
    score: f32,
}
#[derive(Default)]
pub struct ImageModelsQuery;
#[Object]
impl ImageModelsQuery {
    async fn image_models_state(&self, ctx: &Context<'_>) -> Result<ImageModelsState> {
        ctx.data::<Arc<Runtime>>()?.snapshot().try_into()
    }
    async fn semantic_image_search(
        &self,
        ctx: &Context<'_>,
        text: String,
        max_results: i32,
    ) -> Result<Vec<SemanticImageMatch>> {
        Ok(ctx
            .data::<Arc<Runtime>>()?
            .search(&text, usize::try_from(max_results)?)
            .await
            .map_err(async_graphql::Error::new)?
            .into_iter()
            .map(|row| SemanticImageMatch {
                image_id: row.image_id.into(),
                score: row.score,
            })
            .collect())
    }
}
#[derive(Default)]
pub struct ImageModelsMutation;
#[Object]
impl ImageModelsMutation {
    async fn release_image_model_memory(&self, ctx: &Context<'_>) -> Result<bool> {
        ctx.data::<Arc<Runtime>>()?
            .release()
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
}
