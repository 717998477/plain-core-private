use super::*;
fn fixture() -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    (dir, prefs)
}
#[test]
fn defaults_are_complete_and_partial_patch_does_not_reset_other_settings() {
    let (_dir, prefs) = fixture();
    let settings = snapshot(&prefs).unwrap();
    assert_eq!(settings.http_port, 8080);
    assert!(settings.keep_awake);
    assert!(settings.desktop_access);
    assert_eq!(
        settings.app_home_features,
        "IMAGES|VIDEOS|AUDIO|DOCS|FILES|NOTES|FEEDS"
    );
    patch(
        &prefs,
        SettingsPatch {
            editor_font_size: Some(20),
            ..Default::default()
        },
    )
    .unwrap();
    patch(
        &prefs,
        SettingsPatch {
            keep_awake: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let result = snapshot(&prefs).unwrap();
    assert_eq!(result.editor_font_size, 20);
    assert!(!result.keep_awake);
}

#[test]
fn invalid_patch_and_failed_write_leave_store_unchanged() {
    let (dir, prefs) = fixture();
    let before = prefs.entries();
    assert!(
        patch(
            &prefs,
            SettingsPatch {
                http_port: Some(70000),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, prefs.entries());
    std::fs::create_dir(dir.path().join("prefs.json.tmp")).unwrap();
    assert!(
        patch(
            &prefs,
            SettingsPatch {
                keep_awake: Some(false),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, prefs.entries());
}
#[test]
fn notification_actions_are_atomic_and_reject_invalid_modes() {
    let (_dir, prefs) = fixture();
    std::thread::scope(|scope| {
        for i in 0..12 {
            let prefs = &prefs;
            scope.spawn(move || {
                Action::ToggleNotificationApp(format!("app{i}"))
                    .apply(prefs)
                    .unwrap();
            });
        }
    });
    assert_eq!(
        snapshot(&prefs).unwrap().notification_filter["apps"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    Action::SetNotificationMode("allowlist".into())
        .apply(&prefs)
        .unwrap();
    Action::ClearNotificationApps.apply(&prefs).unwrap();
    assert_eq!(
        snapshot(&prefs).unwrap().notification_filter,
        serde_json::json!({"mode":"allowlist","apps":[]})
    );
    assert!(
        Action::SetNotificationMode("other".into())
            .apply(&prefs)
            .is_err()
    );
}

#[test]
fn structured_values_reject_unusable_shapes_without_writing() {
    let (_dir, prefs) = fixture();
    let before = prefs.entries();
    for input in [
        SettingsPatch {
            notification_filter: Some(serde_json::json!({"mode":"other","apps":[]})),
            ..Default::default()
        },
        SettingsPatch {
            screen_mirror_quality: Some(serde_json::json!({"mode":"HD","resolution":0})),
            ..Default::default()
        },
        SettingsPatch {
            pomodoro_settings: Some(serde_json::json!({})),
            ..Default::default()
        },
    ] {
        assert!(patch(&prefs, input).is_err());
        assert_eq!(before, prefs.entries());
    }
}


#[test]
fn listener_ports_use_the_single_preferences_store() {
    let (_directory, prefs) = fixture();
    prefs.set("http_port", 9090).unwrap();
    prefs.set("https_port", 9443).unwrap();
    let initial = snapshot(&prefs).unwrap();
    assert_eq!((initial.http_port, initial.https_port), (9090, 9443));
    patch(
        &prefs,
        SettingsPatch {
            http_port: Some(8180),
            https_port: Some(8543),
            ..Default::default()
        },
    )
    .unwrap();
    let configured = snapshot(&prefs).unwrap();
    assert_eq!((configured.http_port, configured.https_port), (8180, 8543));
    assert_eq!(prefs.get::<i32>("http_port").unwrap(), Some(8180));
    assert_eq!(prefs.get::<i32>("https_port").unwrap(), Some(8543));
}
