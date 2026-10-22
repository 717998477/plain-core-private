//! Public preference roots share the local Rust store and notify UI projections.

use async_graphql::{Context, Json, Object};
use plain_rs::prefs::Prefs;
use serde_json::Value;
use std::sync::Arc;

/// Measured in bytes, not characters: the limit is what the store has to
/// hold, and a multi-byte value would slip past a character count.
fn validate_value(value: &Value) -> async_graphql::Result<()> {
    if value.to_string().len() > 65536 {
        return Err(async_graphql::Error::new("invalid_pref_value"));
    }
    Ok(())
}

#[derive(Default)]
pub struct PrefsQuery;

#[Object]
impl PrefsQuery {
    async fn prefs(&self, ctx: &Context<'_>) -> Json<Value> {
        Json(Value::Object(
            prefs(ctx).entries().into_iter().filter(|(key, _)| allowed(ctx, key)).collect(),
        ))
    }
    async fn saved_value(&self, ctx: &Context<'_>, key: String) -> async_graphql::Result<Json<Value>> {
        saved_key(&key)?;
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        Ok(Json(state.store.get()?.get_json(&key)?.unwrap_or(Value::Null)))
    }
    async fn scan_history(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<String>> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        Ok(state.db.with_conn(|conn| -> rusqlite::Result<Vec<String>> {
            let mut query = conn.prepare("SELECT text FROM scan_history ORDER BY id DESC")?;
            query.query_map([], |row| row.get(0))?.collect()
        })?)
    }

}

#[derive(Default)]
pub struct PrefsMutation;

#[Object]
impl PrefsMutation {
    async fn set_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: Json<Value>,
    ) -> async_graphql::Result<bool> {
        validate_value(&value.0)?;
        super::state::set_pref(ctx, &key, value.0).await?;
        Ok(true)
    }

    async fn remove_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
    ) -> async_graphql::Result<bool> {
        authorize(ctx, &key)?;
        if matches!(key.as_str(), "skip_version" | "auto_check_update") {
            let default = if key == "skip_version" { Value::String(String::new()) } else { Value::Bool(true) };
            super::state::set_pref(ctx, &key, default).await?;
        } else {
            super::state::remove_pref(ctx, &key).await?;
        }
        Ok(true)
    }
    async fn set_saved_value(&self, ctx: &Context<'_>, key: String, value: Json<Value>) -> async_graphql::Result<bool> {
        saved_key(&key)?;
        validate_value(&value.0)?;
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        Ok(state.store.get()?.set_json(&key, value.0)?)
    }
    async fn add_scan_history(&self, ctx: &Context<'_>, text: String) -> async_graphql::Result<bool> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        state.db.with_conn(|conn| conn.execute("INSERT INTO scan_history(text) VALUES (?1)", [&text]))?;
        Ok(true)
    }
    async fn remove_scan_history(&self, ctx: &Context<'_>, text: String) -> async_graphql::Result<bool> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        state.db.with_conn(|conn| conn.execute("DELETE FROM scan_history WHERE text = ?1", [&text]))?;
        Ok(true)
    }

}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<Prefs> {
    ctx.data_unchecked::<Arc<Prefs>>()
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/settings/public.rs"]
mod tests;

pub(crate) fn allowed(ctx: &Context<'_>, key: &str) -> bool {
    plain_rs::prefs::pref_keys::accessible(key, ctx.data_opt::<crate::graphql::executor::RequestOrigin>() == Some(&crate::graphql::executor::RequestOrigin::Local))
}
pub(crate) fn authorize(ctx: &Context<'_>, key: &str) -> async_graphql::Result<()> {
    if !allowed(ctx, key) { return Err(async_graphql::Error::new("Preference is not accessible")); }
    Ok(())
}
pub(crate) fn require_local(ctx: &Context<'_>) -> async_graphql::Result<()> {
    if ctx.data_opt::<crate::graphql::executor::RequestOrigin>() != Some(&crate::graphql::executor::RequestOrigin::Local) {
        return Err(async_graphql::Error::new("Local access required"));
    }
    Ok(())
}

fn saved_key(key: &str) -> async_graphql::Result<()> {
    if matches!(key, "last_file_path" | "quick_note_draft" | "recent_searches" | "recent_save_dirs") ||
        key.strip_prefix("chat_input_text:").is_some_and(|target| {
            (target.starts_with("channel:") || target.starts_with("peer:")) && target.split_once(':').is_some_and(|(_, id)| !id.is_empty())
        }) {
        Ok(())
    } else { Err(async_graphql::Error::new("Unknown saved data key")) }
}
