use crate::assembly::server::ServerState;
use anyhow::{Result, anyhow, ensure};
use plain_rs::{
    events::WsEvent,
    prefs::system::UpdateInfo,
    updates::{Release, Source},
};
use std::{sync::OnceLock, time::Duration};

#[derive(async_graphql::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppUpdateStatus {
    Available,
    UpToDate,
    RateLimited,
    NotDue,
}

fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .use_rustls_tls()
                .user_agent("PlainHub")
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!(error.clone()))
}

pub(super) fn update(state: &ServerState, patch: serde_json::Value) -> Result<()> {
    let changed = plain_rs::prefs::system::Command::PatchUpdate {
        patch: serde_json::from_value(patch)?,
    }
    .apply(&state.prefs)?;
    if changed {
        let _ = state
            .events
            .send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct Cache {
    checked_at: Option<tokio::time::Instant>,
    outcome: Option<Result<AppUpdateStatus, String>>,
}
#[derive(Default)]
pub(crate) struct Runtime {
    pub(super) cache: tokio::sync::Mutex<Cache>,
    pub(crate) download: std::sync::Arc<super::update_download::Runtime>,
}

pub(crate) async fn check(state: &ServerState, automatic: bool) -> Result<AppUpdateStatus> {
    ensure!(!*state.stop.borrow(), "Rust core stopped");
    crate::services::capabilities::require(&state.host, "APP_UPDATES").await?;
    let mut cache = state.update_checks.cache.lock().await;
    let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis())?;
    let info = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    if automatic && !plain_rs::updates::due(&info, now_ms) {
        return Ok(AppUpdateStatus::NotDue);
    }
    attempt(state, &mut cache).await
}

pub(crate) async fn snapshot(
    state: &ServerState,
) -> Result<crate::graphql::resolvers::app::capability_types::AppUpdate> {
    ensure!(!*state.stop.borrow(), "Rust core stopped");
    crate::services::capabilities::require(&state.host, "APP_UPDATES").await?;
    let mut cache = state.update_checks.cache.lock().await;
    let status = if cache
        .checked_at
        .is_some_and(|at| at.elapsed() < Duration::from_secs(600))
    {
        cache
            .outcome
            .clone()
            .ok_or_else(|| anyhow!("Update check unavailable"))?
            .map_err(anyhow::Error::msg)?
    } else {
        attempt(state, &mut cache).await?
    };
    ensure!(
        status != AppUpdateStatus::RateLimited,
        "Update source rate limited"
    );
    project(state)
}

fn project(
    state: &ServerState,
) -> Result<crate::graphql::resolvers::app::capability_types::AppUpdate> {
    let info = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    let version = state
        .host
        .startup_property("appVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("Missing startup app version"))?;
    let latest = info.latest_release.as_ref();
    Ok(
        crate::graphql::resolvers::app::capability_types::AppUpdate {
            current_version: plain_rs::updates::normalized_version(version),
            latest_version: latest.map(|release| release.version.clone()),
            has_update: latest.is_some_and(|release| {
                plain_rs::updates::available_version(&release.version, version, &info.skip_version)
            }),
            url: latest
                .map(|release| release.url.clone())
                .filter(|url| !url.is_empty()),
            release_name: latest
                .map(|release| release.name.clone())
                .unwrap_or_default(),
            published_at: latest
                .and_then(|release| {
                    chrono::DateTime::parse_from_rfc3339(&release.published_at).ok()
                })
                .map(|time| crate::graphql::types::Instant(time.with_timezone(&chrono::Utc))),
        },
    )
}

async fn attempt(state: &ServerState, cache: &mut Cache) -> Result<AppUpdateStatus> {
    let mut stop = state.stop.clone();
    ensure!(!*stop.borrow(), "Rust core stopped");
    let result = tokio::select! {
        biased;
        _=stop.changed()=>Err(anyhow!("Rust core stopped")),
        result=fetch(state)=>result,
    };
    cache.checked_at = Some(tokio::time::Instant::now());
    cache.outcome = Some(result.as_ref().copied().map_err(ToString::to_string));
    result
}

async fn fetch(state: &ServerState) -> Result<AppUpdateStatus> {
    let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis())?;
    let source: Source = serde_json::from_value(
        state
            .host
            .startup_property("updateSource")
            .cloned()
            .ok_or_else(|| anyhow!("Missing update source"))?,
    )?;
    let abis: Vec<String> = serde_json::from_value(
        state
            .host
            .startup_property("supportedAbis")
            .cloned()
            .ok_or_else(|| anyhow!("Missing supported ABIs"))?,
    )?;
    let version = state
        .host
        .startup_property("appVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("Missing startup app version"))?;
    let url = reqwest::Url::parse(&source.url)?;
    ensure!(
        url.scheme() == "https" && url.host_str().is_some(),
        "Invalid update source"
    );
    let response = client()?.get(url).send().await?;
    update(state, serde_json::json!({"checkUpdateTime":now_ms}))?;
    if response.status() == reqwest::StatusCode::FORBIDDEN
        || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Ok(AppUpdateStatus::RateLimited);
    }
    let response = response.error_for_status()?;
    ensure!(
        response.content_length().unwrap_or(0) <= 4 * 1024 * 1024,
        "Update response too large"
    );
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            bytes.len() + chunk.len() <= 4 * 1024 * 1024,
            "Update response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&bytes)?;
    let latest = plain_rs::updates::release_info(&release, &source)?;
    let mut info: UpdateInfo = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    let available = plain_rs::updates::apply_release(&mut info, version, &abis, &source, &release);
    let mut patch = serde_json::json!({"latestRelease":latest});
    if available {
        patch.as_object_mut().unwrap().extend(serde_json::json!({"newVersion":info.new_version,"log":info.log,"publishDate":info.publish_date,"size":info.size,"downloadUrl":info.download_url}).as_object().unwrap().clone());
    }
    let previous = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    if previous.new_version != info.new_version
        || previous.download_url != info.download_url
        || previous.size != info.size
    {
        state.update_checks.download.invalidate();
        patch["downloadedApkPath"] = serde_json::json!("");
    }
    update(state, patch)?;
    if !available {
        return Ok(AppUpdateStatus::UpToDate);
    }

    Ok(AppUpdateStatus::Available)
}

pub(crate) async fn apply_settings(
    state: &ServerState,
    command: plain_rs::prefs::system::Command,
) -> Result<bool> {
    let _cache = state.update_checks.cache.lock().await;
    let before = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    let mut changed = command.apply(&state.prefs)?;
    let after = plain_rs::prefs::system::snapshot(&state.prefs)?.update_info;
    if before.new_version != after.new_version
        || before.download_url != after.download_url
        || before.size != after.size
        || before.skip_version != after.skip_version
    {
        state.update_checks.download.invalidate();
        changed |= plain_rs::prefs::system::Command::PatchUpdate {
            patch: serde_json::from_value(serde_json::json!({"downloadedApkPath":""}))?,
        }
        .apply(&state.prefs)?;
    }
    Ok(changed)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/settings/updates.rs"]
mod tests;
