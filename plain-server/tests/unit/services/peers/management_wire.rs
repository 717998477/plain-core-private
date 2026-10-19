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
fn authenticated_notification_events_are_hints_and_clipboard_payloads_keep_the_existing_contract() {
    let key = [7; 32];
    for kind in [
        "NOTIFICATION_CREATED",
        "NOTIFICATION_UPDATED",
        "NOTIFICATION_DELETED",
        "NOTIFICATION_REFRESHED",
        "PERMISSIONS_UPDATED",
        "DEVICE_NAME_UPDATED",
        "CLIPBOARD_CHANGED",
    ] {
        let data = serde_json::json!({"id":"fixture","text":"text","sensitive":false});
        let frame = encoded(kind, &serde_json::to_vec(&data).unwrap(), &key).unwrap();
        let decoded = decoded(&frame, &key).unwrap().unwrap();
        assert_eq!(
            decoded,
            (
                kind.into(),
                if kind == "CLIPBOARD_CHANGED" {
                    data
                } else {
                    Value::Null
                }
            )
        );
    }
}

#[test]
fn corrupt_wrong_key_and_invalid_json_frames_cannot_be_projected() {
    let key = [7; 32];
    let frame = encoded("CLIPBOARD_CHANGED", b"{}", &key).unwrap();
    assert!(decoded(&frame, &[8; 32]).is_err());
    let mut changed = frame.clone();
    *changed.last_mut().unwrap() ^= 1;
    assert!(decoded(&changed, &key).is_err());
    assert!(decoded(b"missing separator", &key).is_err());
    assert!(decoded(&frame[..frame.len() - 1], &key).is_err());
    let malformed = encoded("CLIPBOARD_CHANGED", b"not json", &key).unwrap();
    assert!(decoded(&malformed, &key).is_err());
}

#[test]
fn unrelated_binary_events_and_null_payloads_do_not_enter_management_views() {
    let frame = encoded("SCREEN_MIRROR_VIDEO", &[0, 255, 1], &[7; 32]).unwrap();
    assert!(decoded(&frame, &[7; 32]).unwrap().is_none());
    let frame = encoded("CLIPBOARD_CHANGED", b"null", &[7; 32]).unwrap();
    assert!(decoded(&frame, &[7; 32]).unwrap().is_none());
}

#[test]
fn notification_refresh_hints_do_not_require_json_payloads() {
    for payload in [b"".as_slice(), b"null", b"not json"] {
        let frame = encoded("NOTIFICATION_REFRESHED", payload, &[7; 32]).unwrap();
        assert_eq!(
            decoded(&frame, &[7; 32]).unwrap(),
            Some(("NOTIFICATION_REFRESHED".into(), Value::Null))
        );
        assert!(decoded(&frame, &[8; 32]).is_err());
    }
}

fn encoded(kind: &str, payload: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    plain_rs::protocols::websocket::Stream::new(b"registration").encode(kind, payload, key)
}

fn decoded(frame: &[u8], key: &[u8]) -> Result<Option<(String, Value)>> {
    event(
        frame,
        key,
        &mut plain_rs::protocols::websocket::Stream::new(b"registration"),
    )
}

#[test]
fn ignored_events_are_authenticated_and_advance_the_same_connection_sequence() {
    let key = [7; 32];
    let mut sender = plain_rs::protocols::websocket::Stream::new(b"registration");
    let mut receiver = plain_rs::protocols::websocket::Stream::new(b"registration");
    let ignored = sender.encode("MESSAGE_CREATED", b"[]", &key).unwrap();
    let hint = sender.encode("DEVICE_NAME_UPDATED", b"", &key).unwrap();
    assert!(event(&ignored, &key, &mut receiver).unwrap().is_none());
    assert_eq!(
        event(&hint, &key, &mut receiver).unwrap(),
        Some(("DEVICE_NAME_UPDATED".into(), Value::Null))
    );
    assert!(event(&hint, &key, &mut receiver).is_err());
    let (_, body) = plain_rs::protocols::websocket::decode_raw(&hint).unwrap();
    let renamed = plain_rs::protocols::websocket::encode_raw("CLIPBOARD_CHANGED", body).unwrap();
    assert!(
        event(
            &renamed,
            &key,
            &mut plain_rs::protocols::websocket::Stream::new(b"registration")
        )
        .is_err()
    );
}
