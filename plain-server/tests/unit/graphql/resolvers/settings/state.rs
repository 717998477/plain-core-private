use super::*;
use async_graphql::{EmptySubscription, MergedObject, Schema};
#[derive(MergedObject)]
struct Mutations(
    PreferencesMutation,
    crate::graphql::resolvers::settings::public::PrefsMutation,
);
fn fixture() -> (
    tempfile::TempDir,
    Schema<PreferencesQuery, Mutations, EmptySubscription>,
    tokio::sync::broadcast::Receiver<WsEvent>,
) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    let (events, receiver) = broadcast::channel(32);
    let schema = Schema::build(
        PreferencesQuery,
        Mutations(
            PreferencesMutation,
            crate::graphql::resolvers::settings::public::PrefsMutation,
        ),
        EmptySubscription,
    )
    .data(prefs)
    .data(events)
    .finish();
    (dir, schema, receiver)
}

#[tokio::test]
async fn recent_searches_clear_through_the_boolean_preference_write_with_shared_events() {
    let (_directory, schema, mut events) = fixture();
    let prefs = schema.data::<Arc<Prefs>>().unwrap();
    prefs
        .set_user("recent_searches", serde_json::json!(["first", "second"]))
        .unwrap();
    for changed in [true, false] {
        let response = schema
            .execute("mutation{setUserPref(key:\"recent_searches\",value:[])}")
            .await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(response.data.into_json().unwrap()["setUserPref"], true);
        assert_eq!(
            plain_rs::prefs::user::snapshot(prefs)
                .unwrap()
                .recent_searches,
            serde_json::json!([])
        );
        assert_eq!(events.try_recv().is_ok(), changed);
        assert!(events.try_recv().is_err());
    }
}

#[tokio::test]
async fn adb_token_verification_requires_transport_injected_local_origin() {
    use crate::graphql::executor::RequestOrigin;
    let (_directory, schema, _) = fixture();
    let prefs = schema.data::<Arc<Prefs>>().unwrap();
    prefs.set("adb_token", "fixture-token").unwrap();
    for (token, expected) in [("fixture-token", true), ("wrong", false), ("", false)] {
        let document = format!(
            "{{verifyAdbToken(token:{})}}",
            serde_json::to_string(token).unwrap()
        );
        let local = schema
            .execute(async_graphql::Request::new(&document).data(RequestOrigin::Local))
            .await;
        assert!(local.errors.is_empty(), "{:?}", local.errors);
        assert_eq!(local.data.into_json().unwrap()["verifyAdbToken"], expected);
        for origin in [None, Some(RequestOrigin::Remote)] {
            let mut request = async_graphql::Request::new(&document);
            if let Some(origin) = origin {
                request = request.data(origin);
            }
            let denied = schema.execute(request).await;
            assert_eq!(denied.errors.len(), 1);
            assert_eq!(denied.errors[0].message, "no_permission");
            assert!(!denied.data.to_string().contains("true"));
        }
    }
}
#[tokio::test]
async fn main_user_settings_snapshot_preserves_every_native_snapshot_value_without_host_calls() {
    let (_directory, schema, mut events) = fixture();
    let prefs = schema.data::<Arc<Prefs>>().unwrap();
    prefs
        .set_user("editor_font_size", serde_json::json!(20))
        .unwrap();
    prefs
        .set_user("feed_auto_refresh_interval", serde_json::json!(2345))
        .unwrap();
    let expected = serde_json::to_value(plain_rs::prefs::user::snapshot(prefs).unwrap()).unwrap();
    let fields = expected
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let response = schema
        .execute(format!("{{ userSettings {{ {fields} }} }}"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["userSettings"], expected);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn graphql_settings_are_isolated_between_devices() {
    let (_a, first, mut events) = fixture();
    let (_b, second, _) = fixture();
    let response = first
        .execute("mutation { setUserPref(key:\"editor_font_size\",value:20) }")
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(events.recv().await.unwrap().event_type, "PREFS_UPDATED");
    let first = first
        .execute("{userSettings {editorFontSize}} ")
        .await
        .data
        .into_json()
        .unwrap();
    let second = second
        .execute("{userSettings {editorFontSize}} ")
        .await
        .data
        .into_json()
        .unwrap();
    assert_eq!(first["userSettings"]["editorFontSize"], 20);
    assert_eq!(second["userSettings"]["editorFontSize"], 14);
}
#[tokio::test]
async fn system_commands_validate_and_do_not_return_private_keys() {
    let (_dir, schema, _) = fixture();
    let invalid = schema
        .execute("mutation {setSystemPref(key:\"password_type\",value:4){affectedCount}}")
        .await;
    assert!(!invalid.errors.is_empty());
    let updated = schema
        .execute("mutation {setSystemPref(key:\"auth_two_factor\",value:false){affectedCount}}")
        .await;
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    let result = schema
        .execute("{systemSettings {authTwoFactor clientId signaturePublicKey}}")
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        result.data.into_json().unwrap()["systemSettings"]["authTwoFactor"],
        false
    );
    assert!(
        !schema
            .execute("{systemSettings {masterSecret}}")
            .await
            .errors
            .is_empty()
    );
}

#[tokio::test]
async fn runtime_system_preferences_validate_before_publishing() {
    let (_dir, schema, mut events) = fixture();
    for (key, value) in [("log_level", "\"debug\""), ("dev_token", "\"development\"")] {
        let document = format!(
            "mutation {{ setSystemPref(key: \"{key}\", value: {value}) {{ affectedCount }} }}"
        );
        let response = schema.execute(document).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(events.try_recv().unwrap().event_type, "PREFS_UPDATED");
        assert!(events.try_recv().is_err());
    }
    for document in [
        "mutation { setSystemPref(key: \"log_level\", value: \"invalid\") { affectedCount } }",
        "mutation { setSystemPref(key: \"dev_token\", value: 1) { affectedCount } }",
    ] {
        assert!(!schema.execute(document).await.errors.is_empty());
        assert!(events.try_recv().is_err());
    }
}

#[tokio::test]
async fn app_preference_actions_execute_through_the_main_mutations() {
    let (_directory, schema, mut events) = fixture();
    for selection in [
        r#"recordRecentSearch(query:"quoted \"term\" αβγ")"#,
        r#"removeRecentSearch(query:"quoted \"term\" αβγ")"#,
        r#"recordRecentSaveDirectory(path:"/selected directory")"#,
        r#"toggleNotificationApp(packageName:"example.app")"#,
        r#"setNotificationFilterMode(mode:"allowlist")"#,
        "clearNotificationFilterApps",
    ] {
        let response = schema
            .execute(format!("mutation {{ {selection} {{ affectedCount }} }}"))
            .await;
        assert!(
            response.errors.is_empty(),
            "{selection}: {:?}",
            response.errors
        );
        let affected = response
            .data
            .into_json()
            .unwrap()
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["affectedCount"]
            .as_i64()
            .unwrap();
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            if event.event_type == "PREFS_UPDATED" {
                count += 1;
            }
        }
        assert_eq!(
            count, affected,
            "one changed action publishes once; unchanged actions publish nothing"
        );
    }
    let response = schema
        .execute("{userSettings{recentSearches recentSaveDirs notificationFilter}}")
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let settings = response.data.into_json().unwrap()["userSettings"].clone();
    assert_eq!(settings["recentSearches"], serde_json::json!([]));
    assert_eq!(
        settings["recentSaveDirs"],
        serde_json::json!(["/selected directory"])
    );
    assert_eq!(
        settings["notificationFilter"],
        serde_json::json!({"mode":"allowlist","apps":[]})
    );
}

#[tokio::test]
async fn result_only_user_patch_does_not_read_unselected_system_settings() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&directory.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    prefs
        .set(
            plain_rs::prefs::system::keys::UPDATE_INFO,
            serde_json::json!("malformed unrelated system data"),
        )
        .unwrap();
    let schema = Schema::build(
        PreferencesQuery,
        Mutations(
            PreferencesMutation,
            crate::graphql::resolvers::settings::public::PrefsMutation,
        ),
        EmptySubscription,
    )
    .data(prefs.clone())
    .finish();
    let response = schema
        .execute(r#"mutation{setUserPref(key:"editor_font_size",value:21)}"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["setUserPref"], true);
    assert_eq!(
        plain_rs::prefs::user::snapshot(&prefs)
            .unwrap()
            .editor_font_size,
        21
    );
    let response = schema.execute("{systemSettings{clientId}}").await;
    assert!(
        !response.errors.is_empty(),
        "Selected system settings must validate malformed data"
    );
}

#[tokio::test]
async fn desktop_namespaced_preferences_use_the_main_settings_mutations() {
    let (directory, schema, mut events) = fixture();
    let request = async_graphql::Request::new(
        "mutation($key:String!,$value:JSON!){setSystemPref(key:$key,value:$value){affectedCount}}",
    )
    .variables(async_graphql::Variables::from_json(serde_json::json!({
        "key":"main_state:peer", "value":{"selected":"files","query":"quoted \"value\""}
    })));
    let response = schema.execute(request).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(events.try_recv().unwrap().event_type, "PREFS_UPDATED");
    let prefs = Prefs::load(&directory.path().join("system_prefs.json")).unwrap();
    let value = prefs.get::<Value>("main_state:peer").unwrap().unwrap();
    assert_eq!(value["selected"], "files");
    assert_eq!(value["query"], "quoted \"value\"");
    let response = schema
        .execute(r#"mutation{removeSystemPref(key:"main_state:peer"){affectedCount}}"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["removeSystemPref"]["affectedCount"],
        1
    );
    assert_eq!(events.try_recv().unwrap().event_type, "PREFS_UPDATED");
    let prefs = Prefs::load(&directory.path().join("system_prefs.json")).unwrap();
    assert!(prefs.get::<Value>("main_state:peer").unwrap().is_none());
    for key in [
        "client_id",
        "signature_key_pair",
        "../preferences",
        "key with spaces",
    ] {
        let request = async_graphql::Request::new(
            "mutation($key:String!){setSystemPref(key:$key,value:\"changed\"){affectedCount}}",
        )
        .variables(async_graphql::Variables::from_json(
            serde_json::json!({"key":key}),
        ));
        assert!(!schema.execute(request).await.errors.is_empty(), "{key}");
        assert!(events.try_recv().is_err());
    }
}

#[tokio::test]
async fn explicit_resets_generate_rust_owned_values_without_returning_secrets() {
    let (directory, schema, mut events) = fixture();
    let prefs = Prefs::load(&directory.path().join("system_prefs.json")).unwrap();
    let before = plain_rs::prefs::system::snapshot(&prefs).unwrap();
    let response = schema
        .execute("mutation{resetPassword resetUrlToken resetAdbToken}")
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap(),
        serde_json::json!({"resetPassword":true,"resetUrlToken":true,"resetAdbToken":true})
    );
    let persisted = Prefs::load(&directory.path().join("system_prefs.json")).unwrap();
    let after = plain_rs::prefs::system::snapshot(&persisted).unwrap();
    assert_ne!(after.password, before.password);
    assert_ne!(after.url_token, before.url_token);
    assert_ne!(after.adb_token, before.adb_token);
    assert_eq!(after.client_id, before.client_id);
    assert_eq!(after.signature_public_key, before.signature_public_key);
    assert_eq!(
        plain_rs::utils::base64::base64_decode_checked(&after.url_token)
            .unwrap()
            .len(),
        32
    );
    assert_eq!(after.password.len(), 6);
    assert_eq!(after.adb_token.len(), 32);
    assert_eq!(
        (0..3)
            .map(|_| events.try_recv().unwrap().event_type)
            .collect::<Vec<_>>(),
        ["PREFS_UPDATED", "PREFS_UPDATED", "PREFS_UPDATED"]
    );
    assert!(events.try_recv().is_err());
}
