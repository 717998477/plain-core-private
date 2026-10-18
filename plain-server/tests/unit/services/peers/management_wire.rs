use super::*;

#[test]
fn registration_preserves_the_existing_encrypted_unix_millisecond_frame() {
    let key = [7; 32];
    let frame = registration(&key, 1_700_000_000_123).unwrap();
    assert_eq!(
        plain_rs::crypto::xchacha_decrypt_raw(&key, &frame).unwrap(),
        b"1700000000123"
    );
    assert!(plain_rs::crypto::xchacha_decrypt_raw(&[8; 32], &frame).is_none());
    assert!(registration(&[7; 31], 0).is_err());
}

#[test]
fn authenticated_json_events_keep_the_peer_protocol_payload() {
    let key = [7; 32];
    for kind in [
        "NOTIFICATION_CREATED",
        "NOTIFICATION_UPDATED",
        "NOTIFICATION_DELETED",
        "NOTIFICATION_REFRESHED",
        "PERMISSIONS_UPDATED",
        "CLIPBOARD_CHANGED",
    ] {
        let data = serde_json::json!({"id":"fixture","text":"text","sensitive":false});
        let frame =
            plain_rs::protocols::websocket::encode(kind, &serde_json::to_vec(&data).unwrap(), &key)
                .unwrap();
        let decoded = event(&frame, &key).unwrap().unwrap();
        assert_eq!(decoded, (kind.into(), data));
    }
}

#[test]
fn corrupt_wrong_key_and_invalid_json_frames_cannot_be_projected() {
    let key = [7; 32];
    let frame = plain_rs::protocols::websocket::encode("CLIPBOARD_CHANGED", b"{}", &key).unwrap();
    assert!(event(&frame, &[8; 32]).is_err());
    let mut changed = frame.clone();
    *changed.last_mut().unwrap() ^= 1;
    assert!(event(&changed, &key).is_err());
    assert!(event(b"missing separator", &key).is_err());
    assert!(event(&frame[..frame.len() - 1], &key).is_err());
    let malformed =
        plain_rs::protocols::websocket::encode("NOTIFICATION_CREATED", b"not json", &key).unwrap();
    assert!(event(&malformed, &key).is_err());
}

#[test]
fn unrelated_binary_events_and_null_payloads_do_not_enter_management_views() {
    let frame =
        plain_rs::protocols::websocket::encode_raw("SCREEN_MIRROR_VIDEO", &[0, 255, 1]).unwrap();
    assert!(event(&frame, &[7; 32]).unwrap().is_none());
    let frame =
        plain_rs::protocols::websocket::encode("CLIPBOARD_CHANGED", b"null", &[7; 32]).unwrap();
    assert!(event(&frame, &[7; 32]).unwrap().is_none());
}
