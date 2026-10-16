use super::*;
use async_graphql::{EmptySubscription, Schema};
fn fixture() -> (
    tempfile::TempDir,
    Schema<PreferencesQuery, PreferencesMutation, EmptySubscription>,
    tokio::sync::broadcast::Receiver<WsEvent>,
) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    let (events, receiver) = broadcast::channel(32);
    let schema = Schema::build(PreferencesQuery, PreferencesMutation, EmptySubscription)
        .data(prefs)
        .data(events)
        .finish();
    (dir, schema, receiver)
}
#[tokio::test]
async fn graphql_settings_are_isolated_between_devices() {
    let (_a, first, mut events) = fixture();
    let (_b, second, _) = fixture();
    let response=first.execute("mutation { updateUserSettings(input:{editorFontSize:20}) { result { affectedCount } } }").await;
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
    let invalid=schema.execute("mutation {updateSystemSettings(input:{action:SET_PASSWORD_TYPE,value:4}){result {affectedCount}}}").await;
    assert!(!invalid.errors.is_empty());
    let updated=schema.execute("mutation {updateSystemSettings(input:{action:SET_TWO_FACTOR,enabled:false}){result {affectedCount}}}").await;
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
        "clearRecentSearches",
        r#"recordRecentSaveDirectory(path:"/selected directory")"#,
        r#"toggleNotificationApp(packageName:"example.app")"#,
        r#"setNotificationFilterMode(mode:"allowlist")"#,
        "clearNotificationFilterApps",
        "deletePlaylistVideos(paths:[])",
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
    let schema = Schema::build(PreferencesQuery, PreferencesMutation, EmptySubscription)
        .data(prefs.clone())
        .finish();
    let response = schema.execute("mutation{updateUserSettings(input:{editorFontSize:21}){result{affectedCount} userSettings{editorFontSize}}}").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["updateUserSettings"]["result"]["affectedCount"], 1);
    assert_eq!(
        data["updateUserSettings"]["userSettings"]["editorFontSize"],
        21
    );
    let response = schema
        .execute(
            "mutation{updateUserSettings(input:{editorFontSize:22}){systemSettings{clientId}}}",
        )
        .await;
    assert!(
        !response.errors.is_empty(),
        "selected system settings must still validate their data"
    );
    assert_eq!(
        plain_rs::prefs::user::snapshot(&prefs)
            .unwrap()
            .editor_font_size,
        22
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
