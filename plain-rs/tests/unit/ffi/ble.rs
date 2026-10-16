use super::*;
#[test]
fn encoder_retains_message_and_roundtrips_native_handle() {
    let body = (0..8192).map(|v| v as u8).collect::<Vec<_>>();
    let message = call(0, 0, 0, 0, 0, false, &body).unwrap().unwrap();
    let encode = encoder(17, true, 512, &message).unwrap();
    let assembly = plain_ble_new();
    let mut complete = None;
    while let Some(frame) = call(4, encode, 0, 0, 0, false, &[]).unwrap() {
        assert!(frame.len() <= 512);
        if let Some(data) = call(3, assembly, 0, 0, 0, false, &frame).unwrap() {
            complete = Some(data);
        }
    }
    assert_eq!(plain_ble_info(assembly), 17 | (1 << 32));
    assert_eq!(
        call(1, 0, 0, 0, 0, false, &complete.unwrap())
            .unwrap()
            .unwrap(),
        body
    );
    plain_ble_free(encode);
    plain_ble_free(assembly);
    assert!(call(4, encode, 0, 0, 0, false, &[]).is_err());
    assert!(call(3, assembly, 0, 0, 0, false, &[]).is_err());
}
#[test]
fn c_buffer_reports_error_and_is_released() {
    let buffer = unsafe { plain_ble_call(99, 0, 0, 0, 0, false, std::ptr::null(), 0) };
    assert_eq!(buffer.status, -1);
    assert!(buffer.len > 0);
    unsafe {
        plain_ble_buffer_free(buffer);
    }
}

#[test]
fn removed_gateway_actions_cannot_be_called_through_native_codecs() {
    for action in 5..=8 {
        for data in [
            &[][..],
            br#"{"clientId":"peer","token":"key","body":"{}"}"#.as_slice(),
        ] {
            assert_eq!(
                call(action, 0, 0, 0, 0, false, data).unwrap_err(),
                "Invalid BLE codec action",
            );
            let buffer =
                unsafe { plain_ble_call(action, 0, 0, 0, 0, false, data.as_ptr(), data.len()) };
            assert_eq!(buffer.status, -1);
            unsafe { plain_ble_buffer_free(buffer) };
        }
    }
}

#[test]
fn discovery_codec_uses_shared_bytes_and_rejects_invalid_inputs() {
    let payload = crate::discovery::advertisement::ble("fixture", true, false).unwrap();
    let decoded = call(9, 0, 0, 0, 0, false, &payload).unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"shortId":"f16d05ec6b29248d", "awareSupported":true, "awareRunning":false})
    );
    for length in 0..9 {
        assert_eq!(
            call(9, 0, 0, 0, 0, false, &payload[..length])
                .unwrap()
                .unwrap(),
            b"null"
        );
    }
    assert!(call(9, 0, 0, 0, 0, false, &[0; 1651]).is_err());
    assert_eq!(
        call(10, 0, 0, 0, 0, false, b"fixture").unwrap().unwrap(),
        b"f16d05ec6b29248d"
    );
    for value in [vec![], vec![b'a'; 1025], vec![255]] {
        assert!(call(10, 0, 0, 0, 0, false, &value).is_err());
    }
}

#[test]
fn nearby_codec_roundtrips_shared_wire_without_a_runtime() {
    let discover = br#"{"kind":"DISCOVER"}"#;
    assert_eq!(
        call(11, 0, 0, 0, 0, false, discover).unwrap().unwrap(),
        b"DISCOVER:"
    );
    let input = br#"{"kind":"PAIR_CANCEL","payload":{"fromId":"sender","toId":"receiver"}}"#;
    let wire = call(11, 0, 0, 0, 0, false, input).unwrap().unwrap();
    let output = call(12, 0, 0, 0, 0, false, &wire).unwrap().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
        serde_json::from_slice::<serde_json::Value>(input).unwrap()
    );
    assert!(call(11, 0, 0, 0, 0, false, br#"{"kind":"INVALID"}"#).is_err());
    for bytes in [
        b"missing-prefix".to_vec(),
        b"UNKNOWN:{}".to_vec(),
        vec![255],
    ] {
        assert!(call(12, 0, 0, 0, 0, false, &bytes).is_err());
    }
}
