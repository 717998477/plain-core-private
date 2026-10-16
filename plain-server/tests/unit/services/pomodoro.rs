use super::*;
use crate::assembly::server::ContentServer;
use crate::graphql::executor::content_changes;
use async_graphql::Request;
use serde_json::{Value, json};

struct Fixture {
    _directory: tempfile::TempDir,
    service: Arc<Service>,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&directory.path().join("plain.db")).unwrap());
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let mut settings = plain_rs::prefs::user::UserSettings::default().pomodoro_settings;
    for (key, value) in [
        ("workDurationMin", 1),
        ("shortBreakDurationMin", 2),
        ("longBreakDurationMin", 3),
        ("pomodorosBeforeLongBreak", 2),
    ] {
        settings[key] = json!(value);
    }
    prefs.set_user("pomodoro_settings", settings).unwrap();
    let (events, _) = broadcast::channel(128);
    Fixture {
        _directory: directory,
        service: Service::new(db, prefs, events),
    }
}

fn noon(day: u32) -> DateTime<Utc> {
    chrono::NaiveDate::from_ymd_opt(2026, 10, day)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap()
        .and_local_timezone(chrono::Local)
        .earliest()
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn completion_is_atomic_and_shared_phase_rules_match_the_app_baseline() {
    let fixture = fixture();
    let service = fixture.service;
    let now = noon(3);
    service.command_at("start", Some(1), now).unwrap();
    let results = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..16)
            .map(|_| {
                scope.spawn(|| {
                    service
                        .tick_at(false, now + chrono::Duration::seconds(1))
                        .unwrap()
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|result| result.completed_state.is_some())
            .count(),
        1
    );
    let row = service
        .db
        .pomodoro_get_by_date("2026-10-03")
        .unwrap()
        .unwrap();
    assert_eq!((row.completed_count, row.total_work_secs), (1, 60));
    assert!(
        results
            .iter()
            .all(|result| result.today.state == PomodoroState::ShortBreak
                && !result.today.is_running)
    );
    service.command_at("start", Some(1), now).unwrap();
    let rest = service
        .tick_at(false, now + chrono::Duration::seconds(1))
        .unwrap();
    assert_eq!(rest.today.state, PomodoroState::Work);
    assert_eq!(rest.today.current_round, 2);
    service.command_at("start", Some(1), now).unwrap();
    let work = service
        .tick_at(false, now + chrono::Duration::seconds(1))
        .unwrap();
    assert_eq!(work.today.state, PomodoroState::LongBreak);
    assert_eq!(work.today.time_left_sec, 180);
    service.tick_at(true, now).unwrap();
    let row = service
        .db
        .pomodoro_get_by_date("2026-10-03")
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            row.completed_count,
            row.total_work_secs,
            row.total_break_secs
        ),
        (2, 120, 120)
    );
}

#[test]
fn progress_does_not_write_statistics_and_pause_adjust_and_settings_preserve_state() {
    let fixture = fixture();
    let service = fixture.service;
    let now = noon(3);
    for invalid in [0, 86401] {
        assert!(service.command_at("start", Some(invalid), now).is_err());
    }
    service.command_at("start", Some(30), now).unwrap();
    let before = service
        .db
        .with_conn(|c| Ok::<_, anyhow::Error>(c.total_changes()))
        .unwrap();
    for second in 1..10 {
        let progress = service
            .advance_at(false, now + chrono::Duration::seconds(second), true)
            .unwrap();
        assert_eq!(progress.today.time_left_sec, 30 - second as i32);
        assert!(progress.completed_state.is_none());
    }
    assert_eq!(
        service
            .db
            .with_conn(|c| Ok::<_, anyhow::Error>(c.total_changes()))
            .unwrap(),
        before
    );
    let paused = service
        .command_at("pause", None, now + chrono::Duration::seconds(10))
        .unwrap();
    assert_eq!(paused.time_left_sec, 20);
    assert!(paused.is_paused && !paused.is_running);
    assert_eq!(
        service
            .tick_at(false, now + chrono::Duration::hours(1))
            .unwrap()
            .today
            .time_left_sec,
        20
    );
    service.command_at("stop", None, now).unwrap();
    service.command_at("adjust", Some(15), now).unwrap();
    assert_eq!(service.tick_at(false, now).unwrap().today.time_left_sec, 15);
    let mut settings = serde_json::to_value(service.settings().unwrap()).unwrap();
    settings["workDurationMin"] = json!(3);
    service
        .prefs
        .set_user("pomodoro_settings", settings)
        .unwrap();
    assert_eq!(
        service.tick_at(false, now).unwrap().today.time_left_sec,
        180
    );
    assert!(service.command_at("adjust", Some(-1), now).is_err());
    assert!(service.command_at("adjust", Some(181), now).is_err());
}

#[test]
fn rust_rolls_over_calendar_days_without_client_configuration() {
    let fixture = fixture();
    let service = fixture.service;
    let now = noon(3);
    service.command_at("start", Some(1), now).unwrap();
    service
        .tick_at(false, now + chrono::Duration::seconds(1))
        .unwrap();
    service.tick_at(true, now).unwrap();
    assert_eq!(service.tick_at(false, now).unwrap().today.current_round, 2);
    let midnight = Service::initial_runtime(&service.settings().unwrap(), noon(4))
        .unwrap()
        .day_start;
    service
        .command_at("start", Some(1), midnight - chrono::Duration::seconds(1))
        .unwrap();
    let next = service.tick_at(false, midnight).unwrap();
    assert_eq!(next.today.current_round, 1);
    assert_eq!(next.today.completed_count, 1);
    assert_eq!(
        service
            .db
            .pomodoro_get_by_date("2026-10-03")
            .unwrap()
            .unwrap()
            .completed_count,
        1
    );
    assert_eq!(
        service
            .db
            .pomodoro_get_by_date("2026-10-04")
            .unwrap()
            .unwrap()
            .completed_count,
        1
    );
}

#[test]
fn failed_completion_keeps_deadline_and_emits_no_success_until_commit() {
    let fixture = fixture();
    let service = fixture.service;
    let mut events = service.events.subscribe();
    let now = noon(3);
    service.command_at("start", Some(1), now).unwrap();
    while events.try_recv().is_ok() {}
    service.db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_pomodoro BEFORE INSERT ON pomodoro_items BEGIN SELECT RAISE(FAIL, 'test failure'); END")).unwrap();
    assert!(
        service
            .tick_at(false, now + chrono::Duration::seconds(1))
            .is_err()
    );
    assert!(events.try_recv().is_err());
    assert!(service.runtime.lock().as_ref().unwrap().deadline.is_some());
    service
        .db
        .with_conn(|c| c.execute_batch("DROP TRIGGER fail_pomodoro"))
        .unwrap();
    let done = service
        .tick_at(false, now + chrono::Duration::seconds(1))
        .unwrap();
    assert_eq!(done.completed_state, Some(PomodoroState::Work));
    assert_eq!(done.today.completed_count, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&events.try_recv().unwrap().payload).unwrap()["action"],
        "complete"
    );
}

#[tokio::test]
async fn lifecycle_completes_without_any_client_tick_or_query_and_stops() {
    let fixture = fixture();
    let service = fixture.service;
    let mut events = service.events.subscribe();
    let (stop, receiver) = watch::channel(false);
    let task = service.follow(receiver);
    service.command("start", Some(30)).unwrap();
    service.command("adjust", Some(0)).unwrap();
    let completed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            let payload: Value = serde_json::from_str(&event.payload).unwrap();
            if payload["action"] == "complete" {
                break payload;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(completed["today"]["completedCount"], 1);
    assert_eq!(completed["today"]["state"], "SHORT_BREAK");
    assert_eq!(completed["today"]["isRunning"], false);
    assert_eq!(completed["completedState"], "WORK");
    stop.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    while events.try_recv().is_ok() {}
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn stopped_lifecycle_performs_no_initial_work() {
    let fixture = fixture();
    let service = fixture.service;
    let mut events = service.events.subscribe();
    let (_stop, receiver) = watch::channel(true);
    service.follow(receiver).await.unwrap();
    assert!(service.runtime.lock().is_none());
    assert!(events.try_recv().is_err());
}

#[test]
fn core_restart_restores_statistics_but_does_not_restart_a_timer() {
    let fixture = fixture();
    let service = fixture.service;
    let now = noon(3);
    service.command_at("start", Some(1), now).unwrap();
    service
        .tick_at(false, now + chrono::Duration::seconds(1))
        .unwrap();
    service.command_at("start", Some(30), now).unwrap();
    let next = Service::new(
        service.db.clone(),
        service.prefs.clone(),
        service.events.clone(),
    );
    let today = next.tick_at(false, now).unwrap().today;
    assert_eq!(today.completed_count, 1);
    assert_eq!(today.state, PomodoroState::Work);
    assert!(!today.is_running && !today.is_paused);
    assert_eq!(today.time_left_sec, 60);
    let count = next
        .db
        .with_conn(|c| {
            c.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='pomodoro_runtime'",
                [],
                |r| r.get::<_, i32>(0),
            )
        })
        .unwrap();
    assert_eq!(count, 0);
}

async fn call(server: &ContentServer, token: &str, query: &str) -> Value {
    let text = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/graphql", server.port))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query":query}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn server_assembly_runs_the_timer_and_main_controls_share_its_statistics() {
    let fixture = fixture();
    let token = plain_rs::utils::base64::base64_encode(&[12; 32]);
    let server = ContentServer::start(
        fixture._directory.path(),
        &token,
        fixture.service.prefs.clone(),
    )
    .unwrap();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    for query in [
        "mutation {startPomodoro(durationSec:0)}",
        "mutation {adjustPomodoro(durationSec:-1) {state}}",
    ] {
        assert!(call(&server, &token, query).await.get("errors").is_some());
    }
    for query in [
        "mutation {startPomodoro(durationSec:30)}",
        "mutation {pausePomodoro}",
    ] {
        assert!(call(&server, &token, query).await.get("errors").is_none());
    }
    assert_eq!(
        call(&server, &token, "{pomodoroToday {isRunning isPaused}} ").await["data"]["pomodoroToday"],
        json!({"isRunning":false,"isPaused":true})
    );
    call(&server, &token, "mutation {startPomodoro(durationSec:30)}").await;
    call(
        &server,
        &token,
        "mutation {adjustPomodoro(durationSec:0) {state}}",
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event_type == WS_POMODORO_ACTION {
                let payload: Value = serde_json::from_str(&event.payload).unwrap();
                if payload["action"] == "complete" {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        call(
            &server,
            &token,
            "{pomodoroTotalCompleted pomodoroToday {completedCount state isRunning}} "
        )
        .await["data"],
        json!({"pomodoroTotalCompleted":1,"pomodoroToday":{"completedCount":1,"state":"SHORT_BREAK","isRunning":false}})
    );
    let date = Utc::now()
        .with_timezone(&chrono::Local)
        .date_naive()
        .to_string();
    let history = call(&server, &token, &format!("{{pomodoroRecord(date:\"{date}\") {{completedCount totalWorkSec}} pomodoroRecords(startDate:\"{date}\") {{id}}}}")).await;
    assert_eq!(
        history["data"]["pomodoroRecord"],
        json!({"completedCount":1,"totalWorkSec":60})
    );
    assert_eq!(
        history["data"]["pomodoroRecords"].as_array().unwrap().len(),
        1
    );
    call(
        &server,
        &token,
        "mutation {skipPomodoro {today {state} completedState}}",
    )
    .await;
    assert_eq!(
        call(&server, &token, "mutation {stopPomodoro}").await["data"]["stopPomodoro"],
        true
    );
    server.shutdown().await;
}

#[tokio::test]
async fn main_schema_rejects_retired_client_scheduler_controls() {
    let fixture = fixture();
    let token = plain_rs::utils::base64::base64_encode(&[12; 32]);
    let server = ContentServer::start(
        fixture._directory.path(),
        &token,
        fixture.service.prefs.clone(),
    )
    .unwrap();
    for query in [
        "mutation { tickPomodoro {completedState} }",
        "mutation { configurePomodoroDay(date:\"2026-10-03\", dayStart:\"2026-10-03T00:00:00Z\") {date} }",
    ] {
        let result: Value = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/graphql", server.port))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"query":query}).to_string())
            .send()
            .await
            .unwrap()
            .text()
            .await
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap();
        assert!(result.get("errors").is_some(), "{result}");
    }
    server.shutdown().await;
}

#[test]
fn timer_controls_do_not_invalidate_unrelated_domains() {
    for query in [
        "mutation { startPomodoro(durationSec:30) }",
        "mutation { named: skipPomodoro {completedState} }",
        "query {pomodoroToday {state}}",
    ] {
        assert!(!content_changes(&Request::new(query)));
    }
    assert!(content_changes(&Request::new(
        "mutation {skipPomodoro {completedState} deleteBookmarks(ids:[]) {affectedCount}}"
    )));
}

#[tokio::test]
async fn idle_lifecycle_initialization_does_not_emit_a_false_progress_change() {
    let fixture = fixture();
    let service = fixture.service;
    let mut events = service.events.subscribe();
    let (stop, receiver) = watch::channel(false);
    let task = service.follow(receiver);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while service.runtime.lock().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
    task.await.unwrap();
    assert!(events.try_recv().is_err());
}
