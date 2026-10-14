use super::*;
use crate::assembly::server::ContentServer;
use crate::graphql::executor::content_changes;
use async_graphql::Request;
use serde_json::{Value, json};

async fn call(port: u16, token: &str, query: &str) -> Value {
    let text = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query": query}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

async fn expire(server: &ContentServer, token: &str) {
    let result = call(
        server.port,
        token,
        "mutation {adjustPomodoro(durationSec:0) {timeLeftSec}}",
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
}

#[tokio::test]
async fn pomodoro_host_ticks_statistics_atomic_completion_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = Arc::new(Db::open(&path).unwrap());
    db.with_conn(|conn| {
        let count: i32 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='pomodoro_runtime'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    });
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
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
    let token = plain_rs::utils::base64::base64_encode(&[12; 32]);
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let config = r#"mutation {configurePomodoroDay(date:"2026-10-03",dayStart:"2026-10-02T16:00:00.000Z") {date timeLeftSec}}"#;
    let result = call(server.port, &token, config).await;
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(result["data"]["configurePomodoroDay"]["timeLeftSec"], 60);
    for bad in [
        "mutation {startPomodoro(durationSec:0)}",
        "mutation {adjustPomodoro(durationSec:-1)}",
    ] {
        assert!(call(server.port, &token, bad).await.get("errors").is_some());
    }
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:30)}",
    )
    .await;
    call(server.port, &token, "mutation {pausePomodoro}").await;
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {isRunning isPaused timeLeftSec}}"
        )
        .await["data"]["pomodoroToday"],
        json!({"isRunning":false,"isPaused":true,"timeLeftSec":30})
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:30)}",
    )
    .await;
    assert_eq!(
        call(server.port, &token, "{pomodoroToday {isRunning isPaused}}").await["data"]["pomodoroToday"],
        json!({"isRunning":true,"isPaused":false})
    );
    call(server.port, &token, "mutation {stopPomodoro}").await;
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {isRunning isPaused timeLeftSec state}}"
        )
        .await["data"]["pomodoroToday"],
        json!({"isRunning":false,"isPaused":false,"timeLeftSec":60,"state":"WORK"})
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&server, &token).await;
    let query = "mutation {tickPomodoro {today {completedCount state timeLeftSec} completedState}}";
    let results =
        futures_util::future::join_all((0..16).map(|_| call(server.port, &token, query))).await;
    assert_eq!(
        results
            .iter()
            .filter(|r| r["data"]["tickPomodoro"]["completedState"] == "WORK")
            .count(),
        1
    );
    let row = db.pomodoro_get_by_date("2026-10-03").unwrap().unwrap();
    assert_eq!(row.completed_count, 1);
    assert_eq!(row.total_work_secs, 60);
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&server, &token).await;
    call(server.port, &token, query).await;
    let row = db.pomodoro_get_by_date("2026-10-03").unwrap().unwrap();
    assert_eq!(row.total_break_secs, 120);
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&server, &token).await;
    let done = call(server.port, &token, query).await;
    assert_eq!(done["data"]["tickPomodoro"]["today"]["state"], "LONG_BREAK");
    assert_eq!(done["data"]["tickPomodoro"]["today"]["timeLeftSec"], 180);
    call(
        server.port,
        &token,
        "mutation {skipPomodoro {completedState}}",
    )
    .await;
    assert_eq!(
        db.pomodoro_get_by_date("2026-10-03")
            .unwrap()
            .unwrap()
            .total_break_secs,
        120
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:30)}",
    )
    .await;
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {isRunning completedCount}}"
        )
        .await["data"]["pomodoroToday"]["isRunning"],
        false
    );
    let restored = call(server.port, &token, config).await;
    assert_eq!(restored["data"]["configurePomodoroDay"]["timeLeftSec"], 60);
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {completedCount currentRound isPaused state}}"
        )
        .await["data"]["pomodoroToday"],
        json!({"completedCount":2,"currentRound":1,"isPaused":false,"state":"WORK"})
    );
    call(server.port, &token, "mutation {pausePomodoro}").await;
    assert!(
        call(server.port, &token, query).await["data"]["tickPomodoro"]["completedState"].is_null()
    );
    call(server.port,&token,r#"mutation {configurePomodoroDay(date:"2026-10-04",dayStart:"2026-10-03T16:00:00.000Z") {completedCount}}"#).await;
    assert_eq!(
        call(server.port, &token, "{pomodoroToday {completedCount}}").await["data"]["pomodoroToday"]
            ["completedCount"],
        0
    );
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:1)}",
    )
    .await;
    expire(&server, &token).await;
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER reject_statistics BEFORE INSERT ON pomodoro_items BEGIN SELECT RAISE(ABORT, 'test write failure'); END;").unwrap());
    assert!(
        call(server.port, &token, query)
            .await
            .get("errors")
            .is_some()
    );
    assert!(db.pomodoro_get_by_date("2026-10-04").unwrap().is_none());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_statistics;").unwrap());
    assert_eq!(
        call(server.port, &token, query).await["data"]["tickPomodoro"]["today"]["completedCount"],
        1
    );
    let history=call(server.port,&token,r#"{pomodoroRecords(startDate:"2026-10-03") {date totalWorkSec totalBreakSec createdAt} pomodoroTotalCompleted pomodoroRecord(date:"2026-10-04") {completedCount}}"#).await;
    assert!(history.get("errors").is_none(), "{history}");
    assert_eq!(history["data"]["pomodoroTotalCompleted"], 3);
    assert_eq!(
        history["data"]["pomodoroRecords"].as_array().unwrap().len(),
        2
    );
    assert_eq!(history["data"]["pomodoroRecord"]["completedCount"], 1);
    call(
        server.port,
        &token,
        "mutation {startPomodoro(durationSec:30)}",
    )
    .await;
    call(server.port, &token, "mutation {pausePomodoro}").await;
    server.shutdown().await;
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    call(server.port, &token, config).await;
    assert_eq!(
        call(
            server.port,
            &token,
            "{pomodoroToday {isRunning isPaused timeLeftSec completedCount}}"
        )
        .await["data"]["pomodoroToday"],
        json!({"isRunning":false,"isPaused":false,"timeLeftSec":60,"completedCount":2})
    );
    server.shutdown().await;
}
#[test]
fn pomodoro_ticks_do_not_invalidate_other_domains() {
    for query in [
        "mutation { tickPomodoro { completedState } }",
        "mutation { named: tickPomodoro {completedState} }",
        "query {pomodoroToday {state}}",
    ] {
        assert!(!content_changes(&Request::new(query)));
    }
    assert!(content_changes(&Request::new(
        "mutation {tickPomodoro {completedState} deleteBookmarks(ids:[]) {affectedCount}}"
    )));
}
