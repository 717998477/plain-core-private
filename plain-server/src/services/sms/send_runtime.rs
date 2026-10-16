use plain_rs::{
    events::WsEvent,
    prefs::{Prefs, sms_sends},
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Notify, broadcast, watch};

pub(crate) struct Runtime {
    prefs: Arc<Prefs>,
    events: broadcast::Sender<WsEvent>,
    changed: Notify,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Report {
    request_id: String,
    part_index: usize,
    part_count: usize,
    success: bool,
    result_code: i32,
}
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl Runtime {
    pub fn new(prefs: Arc<Prefs>, events: broadcast::Sender<WsEvent>) -> Arc<Self> {
        Arc::new(Self {
            prefs,
            events,
            changed: Notify::new(),
        })
    }
    pub fn register(
        &self,
        id: String,
        client_request_id: Option<String>,
        parts: usize,
    ) -> anyhow::Result<()> {
        sms_sends::register(&self.prefs, id, client_request_id, parts, now())?;
        self.changed.notify_one();
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> anyhow::Result<()> {
        sms_sends::cancel(&self.prefs, id)?;
        self.changed.notify_one();
        Ok(())
    }
    pub fn report(&self, report: Report) -> anyhow::Result<()> {
        if let Some(result) = sms_sends::record(
            &self.prefs,
            &report.request_id,
            report.part_index,
            report.part_count,
            report.success,
            report.result_code,
            now(),
        )? {
            self.publish(result)?;
            self.changed.notify_one();
        }
        Ok(())
    }
    fn publish(&self, outcome: sms_sends::Outcome) -> anyhow::Result<()> {
        let _ = self.events.send(WsEvent::broadcast(
            "SMS_SEND_RESULT",
            serde_json::to_string(&outcome)?,
        ));
        Ok(())
    }
    pub fn replay(&self) -> anyhow::Result<()> {
        sms_sends::tick(&self.prefs, now())?;
        for value in sms_sends::pending(&self.prefs)? {
            if let Some(result) = value.result() {
                self.publish(result)?;
            }
        }
        Ok(())
    }
    fn advance(&self) -> anyhow::Result<()> {
        for result in sms_sends::tick(&self.prefs, now())? {
            self.publish(result)?;
        }
        Ok(())
    }
    pub fn follow(self: &Arc<Self>, mut stop: watch::Receiver<bool>) {
        let owner = self.clone();
        tokio::spawn(async move {
            while !*stop.borrow() {
                if let Err(error) = owner.advance() {
                    log::error!("SMS send tracking: {error}");
                }
                let deadline = sms_sends::pending(&owner.prefs)
                    .ok()
                    .and_then(|rows| rows.into_iter().map(|row| row.deadline()).min());
                let wait = async {
                    if let Some(deadline) = deadline {
                        tokio::time::sleep(Duration::from_millis(deadline.saturating_sub(now())))
                            .await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                };
                tokio::select! { _ = stop.changed() => break, _ = owner.changed.notified() => {}, _ = wait => {} }
            }
        });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/services/sms/send_runtime.rs"]
mod tests;
