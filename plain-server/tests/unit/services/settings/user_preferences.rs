use super::*;
use plain_rs::prefs::user::{self, UserSettingsPatch};

#[tokio::test]
async fn generic_writes_typed_patches_and_removal_share_derived_events() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let (events, mut receiver) = broadcast::channel(16);
    let settings = UserPreferences {
        prefs: &prefs,
        events: Some(&events),
        #[cfg(feature = "core")]
        server: None,
    };
    assert!(
        settings
            .set(user::keys::DEVICE_NAME, serde_json::json!("Synthetic"))
            .await
            .unwrap()
    );
    assert_eq!(receiver.try_recv().unwrap().event_type, "PREFS_UPDATED");
    let event = receiver.try_recv().unwrap();
    assert_eq!(event.event_type, "DEVICE_NAME_UPDATED");
    assert_eq!(event.payload, "\"Synthetic\"");
    assert!(receiver.try_recv().is_err());
    assert!(
        !settings
            .patch(UserSettingsPatch {
                device_name: Some("Synthetic".into()),
                ..Default::default()
            })
            .await
            .unwrap()
    );
    assert!(receiver.try_recv().is_err());
    assert!(settings.remove(user::keys::DEVICE_NAME).await.unwrap());
    assert_eq!(receiver.try_recv().unwrap().event_type, "PREFS_UPDATED");
    let event = receiver.try_recv().unwrap();
    assert_eq!(event.event_type, "DEVICE_NAME_UPDATED");
    assert_eq!(
        event.payload,
        serde_json::to_string(&user::UserSettings::default().device_name).unwrap()
    );
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn invalid_generic_values_leave_state_and_events_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let (events, mut receiver) = broadcast::channel(16);
    let settings = UserPreferences {
        prefs: &prefs,
        events: Some(&events),
        #[cfg(feature = "core")]
        server: None,
    };
    let before = prefs.user_entries();
    assert!(
        settings
            .set(user::keys::HTTP_PORT, serde_json::json!(70000))
            .await
            .is_err()
    );
    assert!(
        settings
            .set(user::keys::DESKTOP_ACCESS, serde_json::json!("false"))
            .await
            .is_err()
    );
    assert_eq!(prefs.user_entries(), before);
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn atomic_actions_and_raw_dynamic_entries_use_the_same_store() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let settings = UserPreferences {
        prefs: &prefs,
        events: None,
        #[cfg(feature = "core")]
        server: None,
    };
    for index in 0..12 {
        settings
            .action(user::Action::RecordSearch(format!("term{index}")))
            .await
            .unwrap();
    }
    settings
        .action(user::Action::RecordSearch("TERM5".into()))
        .await
        .unwrap();
    let snapshot = user::snapshot(&prefs).unwrap();
    assert_eq!(snapshot.recent_searches.as_array().unwrap().len(), 10);
    assert_eq!(snapshot.recent_searches[0], "TERM5");
    settings
        .set("homeFeatures", serde_json::json!(["IMAGES", "FILES"]))
        .await
        .unwrap();
    assert_eq!(
        prefs.get_user::<Value>("homeFeatures").unwrap().unwrap(),
        serde_json::json!(["IMAGES", "FILES"])
    );
    assert_eq!(
        user::snapshot(&prefs).unwrap().home_features,
        user::UserSettings::default().home_features
    );
}
