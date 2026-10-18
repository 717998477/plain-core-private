use super::*;
use async_graphql::Request;
use std::sync::Arc;

#[tokio::test]
async fn native_certificate_rotation_uses_the_server_identity_and_invalid_import_preserves_it() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "fixture").unwrap();
    let server = crate::assembly::server::ContentServer::start(
        directory.path(),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let native = server.native_graphql().unwrap();
    let response = native
        .execute(Request::new("{ tlsCertificateSignature }"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let first = response.data.into_json().unwrap()["tlsCertificateSignature"].clone();
    assert!(!first.as_array().unwrap().is_empty());
    let management = server.native_management().unwrap();
    let rotated =
        serde_json::to_value(management.regenerate_tls_certificate().await.unwrap()).unwrap();
    assert_ne!(first, rotated);
    assert!(
        management
            .import_tls_certificate_pem("invalid".into(), "invalid".into())
            .await
            .is_err()
    );
    let response = native
        .execute(Request::new("{ tlsCertificateSignature }"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["tlsCertificateSignature"],
        rotated
    );
    let state = server.runtime_state();
    assert!(state.tls_identity.read().unwrap().identity().is_ok());
    server.shutdown().await;
}
