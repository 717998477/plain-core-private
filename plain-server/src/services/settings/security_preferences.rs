use plain_rs::{
    events::WsEvent,
    prefs::Prefs,
    prefs::security::{self, Command},
};
use serde_json::Value;
use tokio::sync::broadcast;

pub(crate) struct SecurityPreferences<'a> {
    pub(crate) prefs: &'a Prefs,
    pub(crate) events: Option<&'a broadcast::Sender<WsEvent>>,
    pub(crate) server: Option<&'a crate::assembly::server::ServerState>,
}

impl SecurityPreferences<'_> {
    pub(crate) async fn set(&self, key: &str, value: Value) -> anyhow::Result<bool> {
        let changed = if matches!(key, "auto_check_update" | "skip_version") {
            if let Some(state) = self.server {
                crate::services::settings::updates::set_setting(state, key, value).await?
            } else {
                security::set_entry(self.prefs, key, value)?
            }
        } else {
            security::set_entry(self.prefs, key, value)?
        };
        self.notify(changed);
        if key == plain_rs::prefs::pref_keys::MDNS_HOSTNAME {
            if let Some(state) = self.server {
                state.mdns.update_hostname(state).await?;
            }
        }
        Ok(changed)
    }

    pub(crate) async fn remove(&self, key: &str) -> anyhow::Result<bool> {
        let changed = plain_rs::prefs::security::remove_entry(self.prefs, key)?;
        self.notify(changed);
        if key == plain_rs::prefs::pref_keys::MDNS_HOSTNAME {
            if let Some(state) = self.server {
                state.mdns.update_hostname(state).await?;
            }
        }
        Ok(changed)
    }

    pub(crate) fn reset_password(&self) -> anyhow::Result<bool> {
        self.apply(Command::ResetPassword)
    }
    pub(crate) fn reset_url_token(&self) -> anyhow::Result<bool> {
        self.apply(Command::ResetUrlToken)
    }
    pub(crate) fn reset_adb_token(&self) -> anyhow::Result<bool> {
        self.apply(Command::ResetAdbToken)
    }

    fn apply(&self, command: Command) -> anyhow::Result<bool> {
        let changed = command.apply(self.prefs)?;
        self.notify(changed);
        Ok(changed)
    }

    fn notify(&self, changed: bool) {
        if changed {
            if let Some(events) = self.events {
                let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
            }
        }
    }
}
