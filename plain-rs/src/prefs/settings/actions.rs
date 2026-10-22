use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "camelCase")]
pub enum Action {
    ToggleNotificationApp(String),
    SetNotificationMode(String),
    ClearNotificationApps,
}
impl Action {
    pub fn apply(self, prefs: &Prefs) -> anyhow::Result<bool> {
        let toggle = matches!(&self, Self::ToggleNotificationApp(_));
        if let Self::SetNotificationMode(ref mode) = self {
            anyhow::ensure!(
                ["allowlist", "blacklist"].contains(&mode.as_str()),
                "Invalid notification mode"
            );
        }
        let mut changed = false;
        prefs.update(|entries| {
            let defaults = Settings::default();
            let (key, value) = match self {
                Self::ToggleNotificationApp(app) | Self::SetNotificationMode(app) => {
                    let mut filter = entries
                        .get(keys::NOTIFICATION_FILTER)
                        .cloned()
                        .unwrap_or(defaults.notification_filter);
                    if toggle {
                        let mut apps: Vec<Value> = serde_json::from_value(filter["apps"].clone())
                            .map_err(PrefsError::Decode)?;
                        let value = Value::String(app);
                        if apps.contains(&value) {
                            apps.retain(|a| a != &value);
                        } else {
                            apps.push(value);
                        }
                        filter["apps"] = Value::Array(apps);
                    } else {
                        filter["mode"] = Value::String(app);
                    }
                    (keys::NOTIFICATION_FILTER, filter)
                }
                Self::ClearNotificationApps => {
                    let mut filter = entries
                        .get(keys::NOTIFICATION_FILTER)
                        .cloned()
                        .unwrap_or(defaults.notification_filter);
                    filter["apps"] = serde_json::json!([]);
                    (keys::NOTIFICATION_FILTER, filter)
                }
            };
            if entries.get(key) != Some(&value) {
                entries.insert(key.into(), value);
                changed = true;
            }
            Ok(())
        })?;
        Ok(changed)
    }
}

// Kotlin compares simple character case mappings, without expanding ligatures.
fn same_term(left: &str, right: &str) -> bool {
    fn upper(c: char) -> char {
        let mut mapping = c.to_uppercase();
        let first = mapping.next().unwrap();
        if mapping.next().is_none() { first } else { c }
    }
    left.chars().count() == right.chars().count()
        && left.chars().zip(right.chars()).all(|(a, b)| {
            a == b || upper(a) == upper(b) || a.to_lowercase().next() == b.to_lowercase().next()
        })
}
