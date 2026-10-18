use super::models::{SystemSettings, UserSettings};
use crate::graphql::types::ActionResult;
use crate::services::settings::system_preferences::SystemPreferences;
use crate::services::settings::user_preferences::UserPreferences;
use async_graphql::{Context, Json, Object, Result};
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
fn notify(ctx: &Context<'_>, changed: bool) {
    if changed {
        if let Some(events) = events(ctx) {
            let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
        }
    }
}
#[derive(Default)]
pub struct PreferencesQuery;
#[Object]
impl PreferencesQuery {
    async fn system_settings(&self, ctx: &Context<'_>) -> Result<SystemSettings> {
        Ok(plain_rs::prefs::system::snapshot(prefs(ctx)?)?.try_into()?)
    }
    async fn user_settings(&self, ctx: &Context<'_>) -> Result<UserSettings> {
        Ok(plain_rs::prefs::user::snapshot(prefs(ctx)?)?.into())
    }
    #[graphql(guard = "crate::http::auth::public_gate::LocalOwner")]
    async fn verify_adb_token(&self, ctx: &Context<'_>, token: String) -> Result<bool> {
        Ok(plain_rs::prefs::system::verify_adb_token(
            prefs(ctx)?,
            &token,
        )?)
    }
}
#[derive(Default)]
pub struct PreferencesMutation;
#[Object]
impl PreferencesMutation {
    async fn set_system_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: Json<Value>,
    ) -> Result<ActionResult> {
        validate_key(&key)?;
        let changed = system_preferences(ctx)?.set(&key, value.0).await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_system_pref(&self, ctx: &Context<'_>, key: String) -> Result<ActionResult> {
        validate_dynamic_key(&key)?;
        let changed = plain_rs::prefs::system::remove_entry(prefs(ctx)?, &key)?;
        notify(ctx, changed);
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn reset_password(&self, ctx: &Context<'_>) -> Result<bool> {
        system_preferences(ctx)?.reset_password()?;
        Ok(true)
    }
    async fn reset_url_token(&self, ctx: &Context<'_>) -> Result<bool> {
        system_preferences(ctx)?.reset_url_token()?;
        Ok(true)
    }
    async fn reset_adb_token(&self, ctx: &Context<'_>) -> Result<bool> {
        system_preferences(ctx)?.reset_adb_token()?;
        Ok(true)
    }
    async fn record_recent_search(&self, ctx: &Context<'_>, query: String) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::RecordSearch(query))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_recent_search(&self, ctx: &Context<'_>, query: String) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::RemoveSearch(query))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn record_recent_save_directory(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::RecordSaveDirectory(path))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn toggle_notification_app(
        &self,
        ctx: &Context<'_>,
        package_name: String,
    ) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::ToggleNotificationApp(
                package_name,
            ))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn set_notification_filter_mode(
        &self,
        ctx: &Context<'_>,
        mode: String,
    ) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::SetNotificationMode(mode))
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_notification_filter_apps(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        let changed = user_preferences(ctx)?
            .action(plain_rs::prefs::user::Action::ClearNotificationApps)
            .await?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
}
#[cfg(all(test, feature = "core"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/state.rs"]
mod tests;

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
    {
        return Err(async_graphql::Error::new("invalid_pref_key"));
    }
    Ok(())
}

fn validate_dynamic_key(key: &str) -> Result<()> {
    validate_key(key)?;
    if [
        plain_rs::prefs::system::keys::PASSWORD,
        plain_rs::prefs::system::keys::PASSWORD_TYPE,
        plain_rs::prefs::system::keys::AUTH_TWO_FACTOR,
        plain_rs::prefs::system::keys::ROTATE_URL_TOKEN_ON_RESTART,
        plain_rs::prefs::system::keys::ADB_TOKEN,
        plain_rs::prefs::system::keys::URL_TOKEN,
        plain_rs::prefs::system::keys::MASTER_SECRET,
        plain_rs::prefs::system::keys::UPDATE_INFO,
        plain_rs::prefs::system::keys::API_PERMISSIONS,
        plain_rs::prefs::system::keys::CLIENT_ID,
        plain_rs::prefs::system::keys::MDNS_HOSTNAME,
        plain_rs::prefs::system::keys::SIGNATURE_KEY_PAIR,
        plain_rs::prefs::system::keys::ONBOARDING_COMPLETED,
    ]
    .contains(&key)
    {
        return Err(async_graphql::Error::new(
            "Use the system settings operation",
        ));
    }
    Ok(())
}

#[cfg(all(test, feature = "core", feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/http.rs"]
mod http_tests;

pub(crate) fn user_preferences<'a>(ctx: &'a Context<'_>) -> Result<UserPreferences<'a>> {
    Ok(UserPreferences {
        prefs: prefs(ctx)?,
        events: events(ctx),
        #[cfg(feature = "core")]
        server: ctx.data_opt::<crate::assembly::server::ServerState>(),
    })
}

pub(crate) async fn update_user(
    ctx: &Context<'_>,
    patch: plain_rs::prefs::user::UserSettingsPatch,
) -> Result<bool> {
    Ok(user_preferences(ctx)?.patch(patch).await?)
}

fn system_preferences<'a>(ctx: &'a Context<'_>) -> Result<SystemPreferences<'a>> {
    Ok(SystemPreferences {
        prefs: prefs(ctx)?,
        events: events(ctx),
        #[cfg(feature = "core")]
        server: ctx.data_opt::<crate::assembly::server::ServerState>(),
    })
}
