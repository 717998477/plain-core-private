use super::*;

#[test]
fn browser_vector_binds_the_same_transcript_keys_and_ciphertext_in_rust() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../plain-rs/testdata/crypto/web-login-channel-vector.json"
    ))
    .unwrap();
    let hello: ServerHello = serde_json::from_value(vector["hello"].clone()).unwrap();
    let client = Client {
        client_id: hello.client_id.clone(),
        key: EcdhSession::from_private_key(&base64_decode(
            vector["clientPrivate"].as_str().unwrap(),
        ))
        .unwrap(),
        nonce: hello.client_nonce.clone(),
    };
    assert_eq!(
        hello.transcript().unwrap(),
        vector["transcript"].as_str().unwrap().as_bytes()
    );
    let channel = client
        .accept(
            &hello,
            vector["signingPublicKey"].as_str().unwrap(),
            "fixture-device",
            1000,
        )
        .unwrap();
    assert_eq!(channel.context, vector["context"].as_str().unwrap());
    assert_eq!(
        base64_encode(&channel.request_key),
        vector["requestKey"].as_str().unwrap()
    );
    assert_eq!(
        base64_encode(&channel.response_key),
        vector["responseKey"].as_str().unwrap()
    );
    assert_eq!(
        channel
            .decrypt_request(
                &base64_decode(vector["requestFrame"].as_str().unwrap()),
                1000
            )
            .unwrap(),
        vector["request"].as_str().unwrap().as_bytes()
    );
}

fn pair() -> (Channel, Channel) {
    let (private, public) = plain_rs::crypto::ed25519_generate();
    let (client, hello) = Client::start("browser").unwrap();
    let (server, response) = Channel::accept("browser", "device", &hello, &private, 1000).unwrap();
    let client = client
        .accept(&response, &base64_encode(&public), "device", 1000)
        .unwrap();
    (client, server)
}

#[test]
fn derivation_matches_rfc5869_sha256_case_one() {
    let key = derive(
        &[0x0b; 22],
        &(0_u8..=12).collect::<Vec<_>>(),
        &(0xf0_u8..=0xf9).collect::<Vec<_>>(),
    );
    assert_eq!(
        plain_rs::utils::hex::bytes_to_hex(&key),
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf"
    );
}

#[test]
fn actual_ephemeral_exchange_agrees_and_separates_directions() {
    let (client, server) = pair();
    assert_eq!(client.context, server.context);
    assert_eq!(client.verification_code, server.verification_code);
    assert_eq!(client.verification_code.len(), 35);
    let request = client.encrypt_request(b"private metadata", 1000).unwrap();
    assert_eq!(
        server.decrypt_request(&request, 1000).unwrap(),
        b"private metadata"
    );
    assert!(client.decrypt_response(&request, 1000).is_err());
    let response = server.encrypt_response(b"approved", 1000).unwrap();
    assert_eq!(
        client.decrypt_response(&response, 1000).unwrap(),
        b"approved"
    );
    assert!(server.decrypt_request(&response, 1000).is_err());
    let (_, other) = pair();
    assert!(other.decrypt_request(&request, 1000).is_err());
    assert!(other.decrypt_response(&response, 1000).is_err());
}

#[test]
fn encrypted_frames_and_hello_expire_at_the_advertised_boundary() {
    let (client, server) = pair();
    let request = client.encrypt_request(b"request", 300_999).unwrap();
    assert!(server.decrypt_request(&request, 300_999).is_ok());
    assert!(server.decrypt_request(&request, 301_000).is_err());
    assert!(client.encrypt_request(b"request", 301_000).is_err());
    assert!(server.encrypt_response(b"response", 301_000).is_err());
    let response = server.encrypt_response(b"response", 1000).unwrap();
    assert!(client.decrypt_response(&response, 301_000).is_err());
}

#[test]
fn every_signed_hello_field_and_signature_reject_substitution() {
    for field in [
        "protocol",
        "clientId",
        "deviceId",
        "clientPublicKey",
        "serverPublicKey",
        "clientNonce",
        "serverNonce",
        "expiresAt",
        "signature",
    ] {
        let (private, public) = plain_rs::crypto::ed25519_generate();
        let (client, hello) = Client::start("browser").unwrap();
        let (_, response) = Channel::accept("browser", "device", &hello, &private, 1000).unwrap();
        let mut value = serde_json::to_value(response).unwrap();
        value[field] = match field {
            "expiresAt" => serde_json::json!(301_001),
            "serverPublicKey" | "clientPublicKey" => {
                serde_json::json!(base64_encode(&EcdhSession::generate().public_key_bytes))
            }
            "serverNonce" | "clientNonce" => serde_json::json!(nonce()),
            _ => serde_json::json!("substituted"),
        };
        let response = serde_json::from_value(value).unwrap();
        assert!(
            client
                .accept(&response, &base64_encode(&public), "", 1000)
                .is_err(),
            "{field}"
        );
    }
}

#[test]
fn trusted_identity_and_fresh_client_nonce_cannot_be_replaced() {
    let (private, public) = plain_rs::crypto::ed25519_generate();
    let (client, hello) = Client::start("browser").unwrap();
    let (_, response) = Channel::accept("browser", "device", &hello, &private, 1000).unwrap();
    let (other, _) = Client::start("browser").unwrap();
    assert!(
        other
            .accept(&response, &base64_encode(&public), "device", 1000)
            .is_err()
    );
    assert!(
        client
            .accept(&response, &base64_encode(&public), "other-device", 1000)
            .is_err()
    );
    let (client, hello) = Client::start("browser").unwrap();
    let (_, response) = Channel::accept("browser", "device", &hello, &private, 1000).unwrap();
    let (_, replacement) = plain_rs::crypto::ed25519_generate();
    assert!(
        client
            .accept(&response, &base64_encode(&replacement), "device", 1000)
            .is_err()
    );
}

#[test]
fn server_rejects_malformed_keys_nonces_and_unusable_identities() {
    let (private, _) = plain_rs::crypto::ed25519_generate();
    let (_, mut hello) = Client::start("browser").unwrap();
    let original = hello.public_key.clone();
    for key in ["".into(), base64_encode(&[0; 65]), format!("{original}\n")] {
        hello.public_key = key;
        assert!(Channel::accept("browser", "device", &hello, &private, 1000).is_err());
    }
    hello.public_key = original;
    hello.nonce = base64_encode(&[0; 31]);
    assert!(Channel::accept("browser", "device", &hello, &private, 1000).is_err());
    hello.nonce = nonce();
    assert!(Channel::accept("", "device", &hello, &private, 1000).is_err());
    assert!(Channel::accept("browser", "", &hello, &private, 1000).is_err());
    assert!(Channel::accept("browser", "device", &hello, &[0; 4], 1000).is_err());
}

#[test]
fn a_trailing_client_clock_does_not_reject_a_fresh_signed_hello_or_extend_its_lifetime() {
    let (private, public) = plain_rs::crypto::ed25519_generate();
    let (client, hello) = Client::start("browser").unwrap();
    let (server, response) = Channel::accept("browser", "device", &hello, &private, 1000).unwrap();
    let client = client
        .accept(&response, &base64_encode(&public), "device", 900)
        .unwrap();
    let request = client.encrypt_request(b"request", 300_899).unwrap();
    assert_eq!(
        server.decrypt_request(&request, 300_899).unwrap(),
        b"request"
    );
    assert!(client.encrypt_request(b"request", 300_900).is_err());
    let response = server.encrypt_response(b"response", 300_900).unwrap();
    assert!(client.decrypt_response(&response, 300_900).is_err());
}
