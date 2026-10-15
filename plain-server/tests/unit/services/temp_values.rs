use super::*;

#[test]
fn values_are_consumed_once_overwritten_and_shared_with_clones() {
    let values = TempValues::default();
    assert!(values.take("selection").is_none());
    values.set("selection".into(), "old".into());
    values.clone().set("selection".into(), "current".into());
    assert_eq!(values.clone().take("selection"), Some("current".into()));
    assert!(values.take("selection").is_none());
}

#[tokio::test]
async fn main_mutation_and_resource_store_share_one_runtime_and_keep_other_runtimes_isolated() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first = crate::assembly::server::ContentServer::open(first.path()).unwrap();
    let second = crate::assembly::server::ContentServer::open(second.path()).unwrap();
    let response = first
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new(
            r#"mutation { setTempValue(key:"selection",value:"[]") { key value } }"#,
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(
        second
            .runtime_state()
            .temp_values
            .take("selection")
            .is_none()
    );
    assert_eq!(
        first.runtime_state().temp_values.take("selection"),
        Some("[]".into())
    );
    assert!(
        first
            .runtime_state()
            .temp_values
            .take("selection")
            .is_none()
    );
    first.stop_core().await;
    second.stop_core().await;
}
