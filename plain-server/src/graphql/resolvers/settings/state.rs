use crate::graphql::types::ActionResult;
use crate::services::settings::security_preferences::SecurityPreferences;
use crate::services::settings::preferences::Preferences;
use async_graphql::{Context, Object, Result};
use plain_rs::{events::WsEvent, prefs::Prefs};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

fn prefs<'a>(ctx: &'a Context<'_>) -> Result<&'a Arc<Prefs>> {
    Ok(ctx.data::<Arc<Prefs>>()?)
}
fn events<'a>(ctx: &'a Context<'_>) -> Option<&'a broadcast::Sender<WsEvent>> {
    ctx.data_opt::<broadcast::Sender<WsEvent>>()
}
#[derive(Default)]
pub struct PreferencesMutation;
#[Object]
impl PreferencesMutation {
    async fn reset_password(&self, ctx: &Context<'_>) -> Result<bool> {
        super::public::require_local(ctx)?;
        security_preferences(ctx)?.reset_password()?;
        Ok(true)
    }
    async fn reset_url_token(&self, ctx: &Context<'_>) -> Result<bool> {
        super::public::require_local(ctx)?;
        security_preferences(ctx)?.reset_url_token()?;
        Ok(true)
    }
    async fn reset_adb_token(&self, ctx: &Context<'_>) -> Result<bool> {
        super::public::require_local(ctx)?;
        security_preferences(ctx)?.reset_adb_token()?;
        Ok(true)
    }
    async fn set_notification_filter_mode(
        &self,
        ctx: &Context<'_>,
        mode: String,
    ) -> Result<ActionResult> {
        super::public::require_local(ctx)?;
        let changed = preferences(ctx)?
            .action(plain_rs::prefs::settings::Action::SetNotificationMode(mode))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_notification_filter_apps(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        super::public::require_local(ctx)?;
        let changed = preferences(ctx)?
            .action(plain_rs::prefs::settings::Action::ClearNotificationApps)
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
}
#[cfg(all(test, feature = "core"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/state.rs"]
mod tests;

#[cfg(all(test, feature = "core", feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/http.rs"]
mod http_tests;

pub(crate) fn preferences<'a>(ctx: &'a Context<'_>) -> Result<Preferences<'a>> {
    Ok(Preferences {
        prefs: prefs(ctx)?,
        events: events(ctx),
        #[cfg(feature = "core")]
        server: ctx.data_opt::<crate::assembly::server::ServerState>(),
    })
}

pub(crate) async fn update(
    ctx: &Context<'_>,
    patch: plain_rs::prefs::settings::SettingsPatch,
) -> Result<bool> {
    let fields = serde_json::to_value(&patch)?;
    for (field, value) in fields.as_object().unwrap() {
        if value.is_null() { continue; }
        let key = match field.as_str() {
            "feedAutoRefreshIntervalSec" => "feed_auto_refresh_interval".to_owned(),
            "packageSortBy" => "pkg_sort_by".to_owned(),
            _ => field.chars().flat_map(|c| if c.is_ascii_uppercase() { vec!['_', c.to_ascii_lowercase()] } else { vec![c] }).collect(),
        };
        super::public::authorize(ctx, &key)?;
    }
    Ok(preferences(ctx)?.patch(patch).await?)
}

fn security_preferences<'a>(ctx: &'a Context<'_>) -> Result<SecurityPreferences<'a>> {
    Ok(SecurityPreferences {
        prefs: prefs(ctx)?,
        events: events(ctx),
        #[cfg(feature = "core")]
        server: ctx.data_opt::<crate::assembly::server::ServerState>(),
    })
}

pub(crate) async fn set_pref(ctx: &Context<'_>, key: &str, value: Value) -> Result<bool> {
    super::public::authorize(ctx, key)?;
    if key == "ai_image_search_model" && !matches!(value.as_str(), Some("default" | "imported")) {
        return Err(async_graphql::Error::new("Unknown image search model"));
    }
    if security_key(key) {
        Ok(security_preferences(ctx)?.set(key, value).await?)
    } else {
        Ok(preferences(ctx)?.set(key, value).await?)
    }
}

fn security_key(key: &str) -> bool {
    matches!(key, "password" | "password_type" | "login_require_approval" | "rotate_url_token_on_restart" | "api_permissions" | "mdns_hostname" | "dev_token" | "log_level" | "auto_check_update" | "skip_version")
}
pub(crate) async fn remove_pref(ctx: &Context<'_>, key: &str) -> Result<bool> {
    if security_key(key) {
        Ok(security_preferences(ctx)?.remove(key).await?)
    } else {
        Ok(preferences(ctx)?.remove(key).await?)
    }
}
