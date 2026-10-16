use super::{Prefs, PrefsError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const PREFIX: &str = "sms_pending_";
pub const RETENTION_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub request_id: String,
    pub client_request_id: Option<String>,
    pub part_count: usize,
    pub completed_parts: BTreeSet<usize>,
    pub created_at_millis: u64,
    pub terminal_success: Option<bool>,
    pub terminal_result_code: Option<i32>,
    pub terminal_at_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub request_id: Option<String>,
    pub success: bool,
    pub result_code: i32,
}
impl Pending {
    pub fn result(&self) -> Option<Outcome> {
        Some(Outcome {
            request_id: self.client_request_id.clone(),
            success: self.terminal_success?,
            result_code: self.terminal_result_code?,
        })
    }
    pub fn deadline(&self) -> u64 {
        self.terminal_at_millis
            .unwrap_or(self.created_at_millis)
            .saturating_add(RETENTION_MS)
    }
}
fn key(id: &str) -> String {
    format!("{PREFIX}{id}")
}
fn decode(value: Value) -> super::Result<Pending> {
    let value: String = serde_json::from_value(value).map_err(PrefsError::Decode)?;
    serde_json::from_str(&value).map_err(PrefsError::Decode)
}
fn encode(value: &Pending) -> super::Result<Value> {
    serde_json::to_string(value)
        .map(Value::String)
        .map_err(PrefsError::Encode)
}

pub fn pending(prefs: &Prefs) -> super::Result<Vec<Pending>> {
    prefs
        .entries()
        .into_iter()
        .filter(|(name, _)| name.starts_with(PREFIX))
        .map(|(_, value)| decode(value))
        .collect()
}

pub fn register(
    prefs: &Prefs,
    request_id: String,
    client_request_id: Option<String>,
    part_count: usize,
    now: u64,
) -> super::Result<()> {
    let value = Pending {
        request_id,
        client_request_id,
        part_count: part_count.max(1),
        completed_parts: BTreeSet::new(),
        created_at_millis: now,
        terminal_success: None,
        terminal_result_code: None,
        terminal_at_millis: None,
    };
    prefs.set(
        &key(&value.request_id),
        serde_json::to_string(&value).map_err(PrefsError::Encode)?,
    )?;
    Ok(())
}
pub fn cancel(prefs: &Prefs, id: &str) -> super::Result<()> {
    prefs.remove(&key(id))?;
    Ok(())
}

fn finish(value: &mut Pending, success: bool, result_code: i32, now: u64) -> Outcome {
    let result = Outcome {
        request_id: value.client_request_id.clone(),
        success,
        result_code,
    };
    value.terminal_success = Some(success);
    value.terminal_result_code = Some(result_code);
    value.terminal_at_millis = Some(now);
    result
}
pub fn record(
    prefs: &Prefs,
    id: &str,
    index: usize,
    count: usize,
    success: bool,
    code: i32,
    now: u64,
) -> super::Result<Option<Outcome>> {
    prefs.update_system(|map| {
        let name = key(id);
        let Some(raw) = map.get(&name).cloned() else {
            return Ok(None);
        };
        let mut value = decode(raw)?;
        if value.terminal_result_code.is_some()
            || count.max(1) != value.part_count
            || index >= value.part_count
        {
            return Ok(None);
        }
        if success && !value.completed_parts.insert(index) {
            return Ok(None);
        }
        let outcome = if !success || value.completed_parts.len() == value.part_count {
            Some(finish(&mut value, success, code, now))
        } else {
            None
        };
        map.insert(name, encode(&value)?);
        Ok(outcome)
    })
}
fn tick_map(map: &mut Map<String, Value>, now: u64) -> super::Result<Vec<Outcome>> {
    let names = map
        .keys()
        .filter(|name| name.starts_with(PREFIX))
        .cloned()
        .collect::<Vec<_>>();
    let mut outcomes = Vec::new();
    for name in names {
        let mut value = decode(map[&name].clone())?;
        if now < value.deadline() {
            continue;
        }
        if value.terminal_result_code.is_some() {
            map.remove(&name);
        } else {
            outcomes.push(finish(&mut value, false, -1000, now));
            map.insert(name, encode(&value)?);
        }
    }
    Ok(outcomes)
}
pub fn tick(prefs: &Prefs, now: u64) -> super::Result<Vec<Outcome>> {
    prefs.update_system(|map| tick_map(map, now))
}

#[cfg(test)]
#[path = "../../tests/unit/prefs/sms_sends.rs"]
mod tests;
