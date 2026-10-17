use crate::graphql::types::Long;
use crate::services::pomodoro::PomodoroTickResult;
use crate::services::pomodoro::PomodoroToday;
use crate::services::pomodoro::Service;
use async_graphql::{Context, Object};
use plain_rs::db::Db;
use std::sync::Arc;
#[derive(Default)]
pub struct PomodoroStatsQuery;
#[Object]
impl PomodoroStatsQuery {
    async fn pomodoro_total_completed(&self, ctx: &Context<'_>) -> async_graphql::Result<Long> {
        Ok(Long(ctx.data::<Arc<Db>>()?.pomodoro_total_completed()?))
    }
}
#[derive(Default)]
pub struct PomodoroControlMutation;
#[Object]
impl PomodoroControlMutation {
    async fn skip_pomodoro(&self, ctx: &Context<'_>) -> async_graphql::Result<PomodoroTickResult> {
        Ok(ctx.data::<Arc<Service>>()?.tick(true)?)
    }
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
