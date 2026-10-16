use super::*;
const TEXT: &str = r#"{"type":"TEXT","value":{"text":"fixture ' 中文"}}"#;
fn db() -> Db {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.upsert_peer(&DPeer::new(
        "peer",
        "Fixture",
        "",
        Some(1),
        crate::chat::enums::DeviceType::Phone,
    ));
    db
}
#[test]
fn creation_and_sql_failure_return_actual_committed_state() {
    let db = db();
    assert_eq!(
        create(&db, "local", "", TEXT).unwrap().status,
        ChatStatus::Sent
    );
    assert_eq!(
        create(&db, "peer", "", TEXT).unwrap().status,
        ChatStatus::Pending
    );
    assert!(create(&db, "peer", "group", TEXT).is_err());
    assert!(create(&db, "local", "", "{}").is_err());
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_chat BEFORE INSERT ON chats BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")).unwrap();
    assert!(create(&db, "local", "", TEXT).is_err());
}
#[test]
fn reception_is_atomic_and_receipt_survives_restart_and_message_deletion() {
    let path = std::env::temp_dir().join(format!(
        "chat-receipt-{}-{}.db",
        std::process::id(),
        crate::db::short_id()
    ));
    let db = Db::open(&path).unwrap();
    db.upsert_peer(&DPeer::new(
        "peer",
        "Fixture",
        "",
        Some(1),
        crate::chat::enums::DeviceType::Phone,
    ));
    let signature = crate::base64_encode(&[7; 64]);
    let timestamp = crate::peers::pairing::now_ms();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_chat BEFORE INSERT ON chats BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")).unwrap();
    assert!(receive(&db, "peer", "", TEXT, &signature, timestamp).is_err());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_chat;"))
        .unwrap();
    let first = receive(&db, "peer", "", TEXT, &signature, timestamp)
        .unwrap()
        .unwrap();
    assert!(
        receive(&db, "peer", "", TEXT, &signature, timestamp)
            .unwrap()
            .is_none()
    );
    messages::delete(&db, &[first.chat.id]).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert!(
        receive(&db, "peer", "", TEXT, &signature, timestamp)
            .unwrap()
            .is_none()
    );
    assert!(receive(&db, "unknown", "", TEXT, &signature, timestamp).is_err());
    assert!(receive(&db, "peer", "", TEXT, &signature, i64::MIN).is_err());
    crate::db::chat_store::peers::delete(&db, &["peer".into()]).unwrap();
    assert_eq!(
        db.with_conn(
            |c| c.query_row("SELECT count(*) FROM chat_receipts", [], |r| r
                .get::<_, i64>(0))
        )
        .unwrap(),
        0
    );
    drop(db);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn retry_merges_current_recipient_results_and_protects_content_and_creation() {
    let db = db();
    let chat = create(&db, "peer", "", TEXT).unwrap();
    let result = |id: &str, error: Option<&str>| ChannelDeliveryResult {
        peer_id: id.into(),
        peer_name: id.into(),
        error: error.map(str::to_owned),
    };
    let first = delivery(
        &db,
        &chat.id,
        Some(vec![result("one", None), result("two", Some("offline"))]),
        false,
    )
    .unwrap()
    .unwrap();
    assert_eq!(first.status, ChatStatus::Partial);
    let retried = delivery(&db, &chat.id, Some(vec![result("two", None)]), true)
        .unwrap()
        .unwrap();
    assert_eq!(retried.status, ChatStatus::Sent);
    assert_eq!(retried.content, chat.content);
    assert_eq!(retried.created_at, chat.created_at);
    let data: StatusData = serde_json::from_str(retried.status_data.as_deref().unwrap()).unwrap();
    assert_eq!(data.results.unwrap().len(), 2);
    assert!(
        delivery(
            &db,
            &chat.id,
            Some(vec![result("two", None), result("two", None)]),
            true
        )
        .is_err()
    );
    messages::delete(&db, &[chat.id.clone()]).unwrap();
    assert!(
        delivery(&db, &chat.id, Some(vec![]), false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn forwarding_creates_one_atomic_batch_from_the_stored_message() {
    let db = db();
    let source = create(&db, "local", "", TEXT).unwrap();
    let targets = vec![
        ("local".into(), String::new()),
        ("peer".into(), String::new()),
    ];
    db.with_conn(|connection| connection.execute_batch("CREATE TRIGGER reject_forward BEFORE INSERT ON chats WHEN NEW.to_id='peer' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")).unwrap();
    assert!(forward_many(&db, &source.id, &targets).is_err());
    assert_eq!(db.get_all_chats().len(), 1);
    db.with_conn(|connection| connection.execute_batch("DROP TRIGGER reject_forward;"))
        .unwrap();
    let rows = forward_many(&db, &source.id, &targets).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.content == TEXT && row.from_id == "me")
    );
    assert_eq!(rows[0].status, ChatStatus::Sent);
    assert_eq!(rows[1].status, ChatStatus::Pending);
    assert_ne!(rows[0].id, rows[1].id);
    assert_ne!(rows[0].id, source.id);
    assert!(forward_many(&db, "missing", &targets).is_err());
    assert!(forward_many(&db, "missing", &[]).unwrap().is_empty());
    assert_eq!(db.get_all_chats().len(), 3);
}

#[test]
fn channel_reception_requires_current_joined_membership_before_consuming_receipt() {
    use crate::chat::channels::messages::ChannelMember;
    use crate::db::chat_store::{SaveMode, channels};
    let db = db();
    let mut channel = crate::chat::channels::state::create(&db, "owner", "Fixture").unwrap();
    let signature = crate::base64_encode(&[19; 64]);
    let timestamp = crate::peers::pairing::now_ms();
    for members in [
        serde_json::to_string(&[ChannelMember::new("owner")]).unwrap(),
        serde_json::to_string(&[ChannelMember::new("owner"), ChannelMember::pending("peer")])
            .unwrap(),
        "invalid roster".to_owned(),
    ] {
        db.with_conn(|connection| {
            connection.execute(
                "UPDATE chat_channels SET members=?1 WHERE id=?2",
                rusqlite::params![members, channel.id],
            )
        })
        .unwrap();
        assert!(receive(&db, "peer", &channel.id, TEXT, &signature, timestamp).is_err());
        db.with_conn(|connection| -> anyhow::Result<()> {
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM chats", [], |row| row.get::<_, i64>(0))?,
                0
            );
            assert_eq!(
                connection.query_row("SELECT COUNT(*) FROM chat_receipts", [], |row| row
                    .get::<_, i64>(0))?,
                0
            );
            Ok(())
        })
        .unwrap();
    }
    channel.members =
        serde_json::to_string(&[ChannelMember::new("owner"), ChannelMember::new("peer")]).unwrap();
    channels::save(&db, std::slice::from_ref(&channel), SaveMode::Update).unwrap();
    let received = receive(&db, "peer", &channel.id, TEXT, &signature, timestamp)
        .unwrap()
        .unwrap();
    assert_eq!(
        received.chat.channel_id.as_deref(),
        Some(channel.id.as_str())
    );
    assert!(
        receive(&db, "peer", &channel.id, TEXT, &signature, timestamp)
            .unwrap()
            .is_none()
    );
    channel.members = serde_json::to_string(&[ChannelMember::new("owner")]).unwrap();
    channels::save(&db, std::slice::from_ref(&channel), SaveMode::Update).unwrap();
    assert!(
        receive(
            &db,
            "peer",
            &channel.id,
            TEXT,
            &crate::base64_encode(&[20; 64]),
            timestamp
        )
        .is_err()
    );
    db.with_conn(|connection| -> anyhow::Result<()> {
        assert_eq!(
            connection.query_row("SELECT COUNT(*) FROM chats", [], |row| row.get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            connection.query_row("SELECT COUNT(*) FROM chat_receipts", [], |row| row
                .get::<_, i64>(0))?,
            1
        );
        Ok(())
    })
    .unwrap();
}
