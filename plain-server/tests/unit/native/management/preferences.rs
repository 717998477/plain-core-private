use crate::assembly::server::ContentServer;

#[tokio::test]
async fn native_resets_keep_identity_and_publish_once_per_change() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let management = core.native_management().unwrap();
    let before = plain_rs::prefs::security::snapshot(&state.prefs).unwrap();
    let mut events = state.events.subscribe();
    assert!(management.reset_password().unwrap());
    assert!(management.reset_url_token().unwrap());
    assert!(management.reset_adb_token().unwrap());
    let after = plain_rs::prefs::security::snapshot(&state.prefs).unwrap();
    assert_ne!(before.password, after.password);
    assert_ne!(before.url_token, after.url_token);
    assert_ne!(before.adb_token, after.adb_token);
    assert_eq!(before.client_id, after.client_id);
    assert_eq!(before.signature_public_key, after.signature_public_key);
    assert_eq!(after.password.len(), 6);
    assert_eq!(after.adb_token.len(), 32);
    assert_eq!(
        plain_rs::utils::base64::base64_decode_checked(&after.url_token)
            .unwrap()
            .len(),
        32
    );
    for _ in 0..3 {
        assert_eq!(events.try_recv().unwrap().event_type, "PREFS_UPDATED");
    }
    assert!(events.try_recv().is_err());
    core.stop_core().await;
}

#[tokio::test]
async fn native_notification_actions_validate_and_preserve_change_events() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let management = core.native_management().unwrap();
    let mut events = state.events.subscribe();
    assert!(
        management
            .set_notification_filter_mode("allowlist".into())
            .await
            .unwrap()
    );
    assert_eq!(preference_updates(&mut events), 1);
    assert!(
        !management
            .set_notification_filter_mode("allowlist".into())
            .await
            .unwrap()
    );
    assert!(
        management
            .set_notification_filter_mode("invalid".into())
            .await
            .is_err()
    );
    assert_eq!(preference_updates(&mut events), 0);
    management
        .toggle_notification_app("fixture.app".into())
        .await
        .unwrap();
    assert_eq!(preference_updates(&mut events), 1);
    assert!(management.clear_notification_filter_apps().await.unwrap());
    assert_eq!(preference_updates(&mut events), 1);
    assert!(!management.clear_notification_filter_apps().await.unwrap());
    assert_eq!(preference_updates(&mut events), 0);
    core.stop_core().await;
}

#[tokio::test]
async fn native_scan_history_preserves_order_and_scoped_deletion() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let management = core.native_management().unwrap();
    assert!(management.scan_history().unwrap().is_empty());
    for text in ["duplicate", "neighbor", "duplicate", "quote'\nvalue"] {
        assert!(management.add_scan_history(text.into()).unwrap());
    }
    assert_eq!(
        management.scan_history().unwrap(),
        ["quote'\nvalue", "duplicate", "neighbor", "duplicate"]
    );
    assert!(management.remove_scan_history("duplicate".into()).unwrap());
    assert_eq!(
        management.scan_history().unwrap(),
        ["quote'\nvalue", "neighbor"]
    );
    core.stop_core().await;
}

fn preference_updates(events: &mut tokio::sync::broadcast::Receiver<plain_rs::events::WsEvent>) -> usize {
    std::iter::from_fn(|| events.try_recv().ok())
        .filter(|event| event.event_type == "PREFS_UPDATED")
        .count()
}

#[tokio::test]
async fn native_preference_writes_share_validation_and_commit_notifications() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let management = core.native_management().unwrap();
    let mut events = state.events.subscribe();
    assert!(
        management
            .set_pref("http_port", serde_json::json!(8181))
            .await
            .unwrap()
    );
    assert_eq!(preference_updates(&mut events), 1);
    assert_eq!(
        serde_json::to_value(management.prefs_snapshot().unwrap()).unwrap()["httpPort"],
        8181
    );
    assert!(
        !management
            .set_pref("http_port", serde_json::json!(8181))
            .await
            .unwrap()
    );
    assert_eq!(preference_updates(&mut events), 0);
    let before = state.prefs.entries_sorted();
    for (key, value) in [
        ("http_port", serde_json::json!(70000)),
        ("url_token", serde_json::json!("invalid")),
        ("unknown_pref", serde_json::json!(true)),
        ("ai_image_search_model", serde_json::json!("unknown")),
    ] {
        assert!(management.set_pref(key, value).await.is_err());
    }
    assert_eq!(state.prefs.entries_sorted(), before);
    assert_eq!(preference_updates(&mut events), 0);
    assert!(
        management
            .set_pref("login_require_approval", serde_json::json!(false))
            .await
            .unwrap()
    );
    assert_eq!(preference_updates(&mut events), 1);
    assert_eq!(
        serde_json::to_value(management.prefs_snapshot().unwrap()).unwrap()["loginRequireApproval"],
        false
    );
    core.stop_core().await;
    assert!(
        management
            .set_pref("http_port", serde_json::json!(8282))
            .await
            .is_err()
    );
}
