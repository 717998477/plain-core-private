use crate::graphql::types::Instant;
use async_graphql::{Enum, SimpleObject};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use plain_rs::db::Db;
use plain_rs::events::WS_POMODORO_ACTION;
use plain_rs::events::WsEvent;
use plain_rs::prefs::Prefs;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone, Deserialize, Serialize, SimpleObject)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroSettings {
    pub work_duration_min: i32,
    pub short_break_duration_min: i32,
    pub long_break_duration_min: i32,
    pub pomodoros_before_long_break: i32,
    pub show_notification: bool,
    pub play_sound_on_complete: bool,
    pub sound_path: String,
    pub original_sound_name: String,
}
impl Default for PomodoroSettings {
    fn default() -> Self {
        serde_json::from_value(plain_rs::prefs::user::UserSettings::default().pomodoro_settings)
            .expect("Default pomodoro settings")
    }
}
#[derive(Enum, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Debug)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PomodoroState {
    Work,
    ShortBreak,
    LongBreak,
}
#[derive(Clone, SimpleObject)]
pub struct PomodoroToday {
    pub date: Instant,
    pub completed_count: i32,
    pub current_round: i32,
    pub time_left_sec: i32,
    pub total_time_sec: i32,
    pub is_running: bool,
    pub is_paused: bool,
    pub state: PomodoroState,
}
#[derive(SimpleObject)]
pub struct PomodoroTickResult {
    pub today: PomodoroToday,
    pub completed_state: Option<PomodoroState>,
}
#[derive(Clone)]
struct Runtime {
    calendar_date: String,
    day_start: DateTime<Utc>,
    round: i32,
    remaining: i32,
    total: i32,
    deadline: Option<DateTime<Utc>>,
    paused: bool,
    state: PomodoroState,
}
impl Runtime {
    fn remaining(&self, now: DateTime<Utc>) -> i32 {
        self.deadline
            .map(|d| ((d - now).num_milliseconds().max(0) + 999) / 1000)
            .map(|n| n.min(i32::MAX as i64) as i32)
            .unwrap_or(self.remaining)
    }
}
pub struct Service {
    db: Arc<Db>,
    prefs: Arc<Prefs>,
    events: broadcast::Sender<WsEvent>,
    runtime: Mutex<Option<Runtime>>,
}
impl Service {
    pub fn new(db: Arc<Db>, prefs: Arc<Prefs>, events: broadcast::Sender<WsEvent>) -> Arc<Self> {
        Arc::new(Self {
            db,
            prefs,
            events,
            runtime: Mutex::new(None),
        })
    }
    pub fn settings(&self) -> anyhow::Result<PomodoroSettings> {
        let settings: PomodoroSettings = serde_json::from_value(
            plain_rs::prefs::user::snapshot(&self.prefs)?.pomodoro_settings,
        )?;
        anyhow::ensure!(
            (1..=1440).contains(&settings.work_duration_min)
                && (1..=1440).contains(&settings.short_break_duration_min)
                && (1..=1440).contains(&settings.long_break_duration_min)
                && (1..=100).contains(&settings.pomodoros_before_long_break),
            "invalid pomodoro settings"
        );
        Ok(settings)
    }
    fn duration(settings: &PomodoroSettings, state: PomodoroState) -> i32 {
        (match state {
            PomodoroState::Work => settings.work_duration_min,
            PomodoroState::ShortBreak => settings.short_break_duration_min,
            PomodoroState::LongBreak => settings.long_break_duration_min,
        }) * 60
    }
    fn initial_runtime(settings: &PomodoroSettings) -> anyhow::Result<Runtime> {
        let today = chrono::Local::now().date_naive();
        let day_start = today
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .ok_or_else(|| anyhow::anyhow!("invalid local day"))?
            .with_timezone(&Utc);
        Ok(Runtime {
            calendar_date: today.to_string(),
            day_start,
            round: 1,
            remaining: settings.work_duration_min * 60,
            total: settings.work_duration_min * 60,
            deadline: None,
            paused: false,
            state: PomodoroState::Work,
        })
    }
    fn runtime(
        &self,
        settings: &PomodoroSettings,
    ) -> anyhow::Result<parking_lot::MutexGuard<'_, Option<Runtime>>> {
        let mut runtime = self.runtime.lock();
        if runtime.is_none() {
            *runtime = Some(Self::initial_runtime(settings)?);
        }
        Ok(runtime)
    }
    fn snapshot(
        conn: &rusqlite::Connection,
        runtime: &Runtime,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PomodoroToday> {
        let count: Option<i32> = conn
            .query_row(
                "SELECT completed_count FROM pomodoro_items WHERE date=?1",
                [&runtime.calendar_date],
                |r| r.get(0),
            )
            .optional()?;
        Ok(PomodoroToday {
            date: Instant(runtime.day_start),
            completed_count: count.unwrap_or(0),
            current_round: runtime.round,
            time_left_sec: runtime.remaining(now),
            total_time_sec: runtime.total,
            is_running: runtime.deadline.is_some(),
            is_paused: runtime.paused,
            state: runtime.state,
        })
    }
    pub fn today(&self) -> anyhow::Result<PomodoroToday> {
        let settings = self.settings()?;
        let runtime = self.runtime(&settings)?;
        self.db
            .with_conn(|c| Self::snapshot(c, runtime.as_ref().unwrap(), Utc::now()))
    }
    pub fn configure_day(&self, date: &str, start: DateTime<Utc>) -> anyhow::Result<PomodoroToday> {
        let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
        anyhow::ensure!(parsed.to_string() == date, "invalid calendar date");
        let settings = self.settings()?;
        let mut runtime = self.runtime(&settings)?;
        let mut r = runtime.as_ref().unwrap().clone();
        let today = self.db.with_conn(|c| {
            if r.calendar_date != date {
                r.round = 1;
            }
            r.calendar_date = date.into();
            r.day_start = start;
            if r.deadline.is_none() && !r.paused {
                r.remaining = Self::duration(&settings, r.state);
                r.total = r.remaining;
            }
            Self::snapshot(c, &r, Utc::now())
        })?;
        *runtime = Some(r);
        Ok(today)
    }
    pub fn command(&self, action: &str, duration: Option<i32>) -> anyhow::Result<PomodoroToday> {
        let settings = self.settings()?;
        let now = Utc::now();
        let mut runtime = self.runtime(&settings)?;
        let mut r = runtime.as_ref().unwrap().clone();
        let today = self.db.with_conn(|c| {
            match action {
                "start" => {
                    let seconds = duration.unwrap_or(r.remaining(now));
                    anyhow::ensure!((1..=86400).contains(&seconds), "invalid duration");
                    r.remaining = seconds;
                    r.deadline = Some(now + chrono::Duration::seconds(seconds as i64));
                    r.paused = false;
                }
                "pause" => {
                    r.remaining = r.remaining(now);
                    r.deadline = None;
                    r.paused = true;
                }
                "stop" => {
                    r.state = PomodoroState::Work;
                    r.remaining = Self::duration(&settings, r.state);
                    r.total = r.remaining;
                    r.deadline = None;
                    r.paused = false;
                }
                "adjust" => {
                    let seconds = duration.ok_or_else(|| anyhow::anyhow!("duration required"))?;
                    anyhow::ensure!((0..=r.total).contains(&seconds), "invalid duration");
                    r.remaining = seconds;
                    if r.deadline.is_some() {
                        r.deadline = Some(now + chrono::Duration::seconds(seconds as i64));
                    }
                }
                _ => anyhow::bail!("unknown pomodoro action"),
            }
            Self::snapshot(c, &r, now)
        })?;
        *runtime = Some(r);
        self.publish(action, &today, None);
        Ok(today)
    }
    pub fn tick(&self, skip: bool) -> anyhow::Result<PomodoroTickResult> {
        let settings = self.settings()?;
        let now = Utc::now();
        let mut runtime = self.runtime(&settings)?;
        let mut r = runtime.as_ref().unwrap().clone();
        let result=self.db.with_conn(|c| {let tx=c.unchecked_transaction()?;
            let completed= if skip || (r.deadline.is_some() && r.remaining(now)==0) {Some(r.state)} else {None};
            if let Some(state)=completed {
                let count: Option<i32>=tx.query_row("SELECT completed_count FROM pomodoro_items WHERE date=?1",[&r.calendar_date],|row|row.get(0)).optional()?;
                let mut count=count.unwrap_or(0);
                if !skip {
                    let work=if state==PomodoroState::Work {r.total} else {0};
                    let rest=if state!=PomodoroState::Work {r.total} else {0};
                    let increment=i32::from(state==PomodoroState::Work);
                    let existing:Option<String>=tx.query_row("SELECT id FROM pomodoro_items WHERE date=?1",[&r.calendar_date],|row|row.get(0)).optional()?;
                    if let Some(id)=existing {tx.execute("UPDATE pomodoro_items SET completed_count=completed_count+?1,total_work_secs=total_work_secs+?2,total_break_secs=total_break_secs+?3,updated_at=?4 WHERE id=?5",params![increment,work,rest,now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),id])?;}
                    else {tx.execute("INSERT INTO pomodoro_items(id,date,completed_count,total_work_secs,total_break_secs,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![uuid::Uuid::new_v4().to_string(),r.calendar_date,increment,work,rest,now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)])?;}
                    count+=increment;
                }
                r.state=if state==PomodoroState::Work {if count>0 && count%settings.pomodoros_before_long_break==0 {PomodoroState::LongBreak} else {PomodoroState::ShortBreak}} else {r.round+=1;PomodoroState::Work};
                r.deadline=None;r.paused=false;r.remaining=Self::duration(&settings,r.state);r.total=r.remaining;
            }
            let today=Self::snapshot(&tx,&r,now)?;tx.commit()?;Ok::<_,anyhow::Error>(PomodoroTickResult {today,completed_state:completed}) })?;
        *runtime = Some(r);
        if result.completed_state.is_some() {
            self.publish(
                if skip { "skip" } else { "complete" },
                &result.today,
                if skip { None } else { result.completed_state },
            );
        }
        Ok(result)
    }
    fn publish(&self, action: &str, today: &PomodoroToday, completed_state: Option<PomodoroState>) {
        let payload = serde_json::json!({"action":action,"timeLeftSec":today.time_left_sec,"totalTimeSec":today.total_time_sec,"completedCount":today.completed_count,"round":today.current_round,"state":today.state,"completedState":completed_state});
        let _ = self
            .events
            .send(WsEvent::broadcast(WS_POMODORO_ACTION, payload.to_string()));
    }
}

#[cfg(all(test, feature = "core", feature = "http_transport"))]
#[path = "../../tests/unit/services/pomodoro.rs"]
mod tests;
