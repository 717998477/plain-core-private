use super::*;

fn item(id: &str) -> Item {
    Item {
        id: id.into(),
        text: "Fixture 🌍".into(),
        sensitive: false,
    }
}

#[test]
fn first_observation_seeds_history_without_writing_and_reload_preserves_deduplication() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("system_prefs.json");
    let prefs = Prefs::load(&path).unwrap();
    assert!(!prepare(&prefs, "peer", &item("old")).unwrap());
    assert!(!prepare(&prefs, "peer", &item("old")).unwrap());
    assert!(prepare(&prefs, "peer", &item("new")).unwrap());
    assert!(prepare(&prefs, "peer", &item("new")).unwrap());
    applied(&prefs, "peer", &item("new")).unwrap();
    let reloaded = Prefs::load(&path).unwrap();
    assert!(!prepare(&reloaded, "peer", &item("new")).unwrap());
    assert!(prepare(&reloaded, "peer", &item("next")).unwrap());
    assert!(!prepare(&reloaded, "another-peer", &item("next")).unwrap());
}

#[test]
fn directions_and_sensitive_items_do_not_advance_the_applied_marker() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    assert!(!prepare(&prefs, "peer", &item("baseline")).unwrap());
    for (direction, receives) in [
        ("off", false),
        ("push", false),
        ("pull", true),
        ("both", true),
    ] {
        prefs.set(DIRECTIONS, json!({"peer":direction})).unwrap();
        assert_eq!(super::receives(&prefs, "peer").unwrap(), receives);
        assert_eq!(prepare(&prefs, "peer", &item("next")).unwrap(), receives);
    }
    let sensitive = Item {
        sensitive: true,
        ..item("secret")
    };
    assert!(!prepare(&prefs, "peer", &sensitive).unwrap());
    assert_eq!(
        prefs.get::<Value>(APPLIED).unwrap().unwrap(),
        json!({"peer":"baseline"})
    );
    for direction in [json!("invalid"), json!(1), Value::Null] {
        prefs.set(DIRECTIONS, json!({"peer":direction})).unwrap();
        assert!(super::receives(&prefs, "peer").is_err());
    }
}

#[test]
fn invalid_text_and_identity_never_change_persisted_state() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    for invalid in [
        item(""),
        Item {
            text: " ".into(),
            ..item("id")
        },
        Item {
            text: "🌍".repeat(128 * 1024 + 1),
            ..item("id")
        },
    ] {
        assert!(prepare(&prefs, "peer", &invalid).is_err());
        assert!(prefs.get::<Value>(APPLIED).unwrap().is_none());
    }
}

#[test]
fn failed_marker_persistence_does_not_claim_success_or_lose_other_peers() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    assert!(!prepare(&prefs, "peer", &item("old")).unwrap());
    assert!(!prepare(&prefs, "other", &item("other-old")).unwrap());
    let before = prefs.entries();
    std::fs::create_dir(directory.path().join("prefs.json.tmp")).unwrap();
    assert!(applied(&prefs, "peer", &item("new")).is_err());
    assert_eq!(prefs.entries(), before);
}

#[test]
fn sending_is_explicit_and_does_not_inherit_the_default_receive_direction() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    assert!(!sends(&prefs, "peer").unwrap());
    assert!(receives(&prefs, "peer").unwrap());
    for (direction, send, receive) in [
        ("off", false, false),
        ("pull", false, true),
        ("push", true, false),
        ("both", true, true),
    ] {
        prefs.set(DIRECTIONS, json!({"peer":direction})).unwrap();
        assert_eq!(sends(&prefs, "peer").unwrap(), send);
        assert_eq!(receives(&prefs, "peer").unwrap(), receive);
        assert!(!sends(&prefs, "another").unwrap());
    }
    for value in [json!("invalid"), json!(1), Value::Null] {
        prefs.set(DIRECTIONS, json!({"peer":value})).unwrap();
        assert!(sends(&prefs, "peer").is_err());
        assert!(receives(&prefs, "peer").is_err());
    }
}
