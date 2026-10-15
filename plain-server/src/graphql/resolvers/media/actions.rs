use crate::services::files::media_actions::MediaActions;
use async_graphql::{Context, ID, Object, Result, SimpleObject};
use plain_rs::enums::DataType;
use plain_rs::media::actions::Action;
use std::sync::Arc;
#[derive(SimpleObject)]
struct MediaActionOutcome {
    affected_count: i32,
    failed_ids: Vec<ID>,
}
#[derive(Default)]
pub struct MediaActionMutation;
#[Object]
impl MediaActionMutation {
    async fn media_action(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        action: Action,
        ids: Vec<ID>,
        from_trash: bool,
        dest_dir: String,
    ) -> Result<MediaActionOutcome> {
        crate::http::auth::public_gate::require(
            ctx,
            ctx.data::<Arc<plain_rs::prefs::Prefs>>()?,
            &["WRITE_EXTERNAL_STORAGE"],
        )?;
        let result = ctx
            .data::<Arc<MediaActions>>()?
            .run(
                r#type,
                action,
                ids.into_iter().map(|id| id.to_string()).collect(),
                from_trash,
                dest_dir,
            )
            .await?;
        Ok(MediaActionOutcome {
            affected_count: result.affected_count,
            failed_ids: result.failed_ids.into_iter().map(ID).collect(),
        })
    }
}
