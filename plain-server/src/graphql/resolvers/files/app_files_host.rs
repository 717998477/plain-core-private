use crate::graphql::resolvers::files::app_file::AppFile;
use async_graphql::{Context, Object};
use plain_rs::files::app_files::FileStore;
use std::sync::Arc;

#[derive(Default)]
pub struct AppFileHostMutation;
#[Object]
impl AppFileHostMutation {
    async fn import_app_file(
        &self,
        ctx: &Context<'_>,
        source: String,
        file_name: String,
        mime_type: String,
        delete_source: bool,
    ) -> async_graphql::Result<AppFile> {
        let file = ctx
            .data::<Arc<FileStore>>()?
            .import(source.into(), file_name, mime_type, delete_source)
            .await
            .map_err(async_graphql::Error::new)?;
        let name =
            plain_rs::chat::attachments::app_file_store::display_name(&file, &Default::default());
        Ok(AppFile::from_dappfile(file, name))
    }
    async fn release_app_file(&self, ctx: &Context<'_>, id: String) -> async_graphql::Result<bool> {
        ctx.data::<Arc<FileStore>>()?
            .release(id)
            .await
            .map_err(async_graphql::Error::new)
    }
}
