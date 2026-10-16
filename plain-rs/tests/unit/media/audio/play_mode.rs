use super::*;

#[test]
fn mode_cycle_is_shared_persisted_and_atomic_between_callers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs");
    let prefs = std::sync::Arc::new(crate::prefs::Prefs::load(&path).unwrap());
    assert_eq!(
        MediaPlayMode::cycle(&prefs).unwrap(),
        MediaPlayMode::RepeatOne
    );
    assert_eq!(
        MediaPlayMode::cycle(&prefs).unwrap(),
        MediaPlayMode::Shuffle
    );
    assert_eq!(MediaPlayMode::cycle(&prefs).unwrap(), MediaPlayMode::Repeat);
    let threads = (0..4)
        .map(|_| {
            let prefs = prefs.clone();
            std::thread::spawn(move || {
                (0..3)
                    .map(|_| MediaPlayMode::cycle(&prefs).unwrap().ordinal())
                    .collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let mut results = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    results.sort_unstable();
    assert_eq!(results, [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2]);
    assert_eq!(
        crate::media::audio::queue::play_mode(&prefs).unwrap(),
        MediaPlayMode::Repeat
    );
    let reopened = crate::prefs::Prefs::load(&path).unwrap();
    assert_eq!(
        crate::media::audio::queue::play_mode(&reopened).unwrap(),
        MediaPlayMode::Repeat
    );
}

#[test]
fn malformed_stored_modes_are_rejected_without_overwriting_them() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = crate::prefs::Prefs::load(&dir.path().join("prefs")).unwrap();
    prefs
        .set_user(crate::prefs::user::keys::AUDIO_PLAY_MODE, 99)
        .unwrap();
    assert!(MediaPlayMode::cycle(&prefs).is_err());
    assert_eq!(
        prefs
            .get_user::<i32>(crate::prefs::user::keys::AUDIO_PLAY_MODE)
            .unwrap(),
        Some(99)
    );
}
