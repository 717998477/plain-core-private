use plain_rs::{
    events::WsEvent,
    prefs::Prefs,
    prefs::system::{self, Command},
};
use serde_json::Value;
use tokio::sync::broadcast;

pub(crate) struct SystemPreferences<'a> {
    pub(crate) prefs: &'a Prefs,
    pub(crate) events: Option<&'a broadcast::Sender<WsEvent>>,
    pub(crate) server: Option<&'a crate::assembly::server::ServerState>,
}

impl SystemPreferences<'_> {
    pub(crate) async fn set(&self, key: &str, value: Value) -> anyhow::Result<bool> {
        let changed = if key == system::keys::UPDATE_INFO {
            if let Some(state) = self.server {
                crate::services::settings::updates::apply_settings(
                    state,
                    Command::PatchUpdate {
                        patch: serde_json::from_value(value)?,
                    },
                )
                .await?
            } else {
                system::set_entry(self.prefs, key, value)?
            }
        } else {
            system::set_entry(self.prefs, key, value)?
        };
        self.notify(changed);
        if key == system::keys::MDNS_HOSTNAME {
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
