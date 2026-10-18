use crate::prefs::Prefs;
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

const DIRECTIONS: &str = "clipboard_directions";
const APPLIED: &str = "clipboard_applied";

#[derive(Deserialize)]
pub struct Item {
    pub id: String,
    pub text: String,
    pub sensitive: bool,
}

pub fn receives(prefs: &Prefs, peer_id: &str) -> Result<bool> {
    let directions = prefs
        .get::<serde_json::Map<String, Value>>(DIRECTIONS)?
        .unwrap_or_default();
    match directions.get(peer_id) {
        None => Ok(true),
        Some(Value::String(direction)) => match direction.as_str() {
            "pull" | "both" => Ok(true),
            "off" | "push" => Ok(false),
            _ => anyhow::bail!("Invalid clipboard direction"),
        },
        Some(_) => anyhow::bail!("Invalid clipboard direction"),
    }
}

pub fn prepare(prefs: &Prefs, peer_id: &str, item: &Item) -> Result<bool> {
    if !receives(prefs, peer_id)? || item.sensitive {
        return Ok(false);
    }
    ensure!(!item.id.is_empty(), "Missing clipboard item identity");
    super::validate_text(&item.text).map_err(anyhow::Error::msg)?;
    Ok(prefs.update_system(|map| {
        let mut applied = applied_ids(map)?;
        match applied.get(peer_id) {
            Some(id) => Ok(id != &item.id),
            None => {
                applied.insert(peer_id.into(), item.id.clone());
                map.insert(
                    APPLIED.into(),
                    serde_json::to_value(applied).map_err(crate::prefs::PrefsError::Encode)?,
                );
                Ok(false)
            }
        }
    })?)
}

pub fn applied(prefs: &Prefs, peer_id: &str, item: &Item) -> Result<()> {
    Ok(prefs.update_system(|map| {
        let mut applied = applied_ids(map)?;
        applied.insert(peer_id.into(), item.id.clone());
        map.insert(
            APPLIED.into(),
            serde_json::to_value(applied).map_err(crate::prefs::PrefsError::Encode)?,
        );
        Ok(())
    })?)
}

fn applied_ids(
    map: &serde_json::Map<String, Value>,
) -> crate::prefs::Result<std::collections::BTreeMap<String, String>> {
    serde_json::from_value(map.get(APPLIED).cloned().unwrap_or_else(|| json!({})))
        .map_err(crate::prefs::PrefsError::Decode)
}

#[cfg(test)]
#[path = "../../tests/unit/clipboard/sync.rs"]
mod tests;
