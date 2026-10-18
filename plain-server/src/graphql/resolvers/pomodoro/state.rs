use crate::services::pomodoro::PomodoroToday;
use crate::services::pomodoro::Service;
use async_graphql::{Context, Object};
use std::sync::Arc;
#[derive(Default)]
pub struct PomodoroControlMutation;
#[Object]
impl PomodoroControlMutation {
    async fn adjust_pomodoro(
        &self,
        ctx: &Context<'_>,
        duration_sec: i32,
    ) -> async_graphql::Result<PomodoroToday> {
        Ok(ctx
            .data::<Arc<Service>>()?
            .command("adjust", Some(duration_sec))?)
    }
}
