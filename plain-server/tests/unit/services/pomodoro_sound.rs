use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;

#[tokio::test]
async fn native_import_replaces_sound_in_the_owned_layout_without_host_calls() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let (_generation, mut requests) = state.host.connect();
    let source = directory.path().join("selected+audio.wav");
    let destination = plain_rs::data_paths::DataPaths::new(directory.path()).pomodoro_sound();
    for bytes in [b"first sound".as_slice(), b"replacement sound".as_slice()] {
        std::fs::write(&source, bytes).unwrap();
        let reference = core.native_management().unwrap().import_pomodoro_sound(format!("file://{}", source.display())).await.unwrap();
        assert_eq!(reference, "app://audio/pomodoro_sound.mp3");
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert!(source.exists());
        assert_eq!(
            std::fs::read_dir(destination.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
    assert!(requests.try_recv().is_err());
    let response = crate::graphql::executor::execute_graphql(
        &state.public,
        async_graphql::Request::new(r#"mutation{importPomodoroSound(source:"/missing")}"#),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!response.errors.is_empty());
    assert_eq!(std::fs::read(&destination).unwrap(), b"replacement sound");
    core.stop_core().await;
}

#[tokio::test]
async fn interrupted_platform_read_preserves_the_previous_sound_and_removes_temporary_files() {
    for outcome in ["failure", "cancel", "finish"] {
        let directory = tempfile::tempdir().unwrap();
        let core = ContentServer::open(directory.path()).unwrap();
        let state = core.runtime_state();
        let destination = plain_rs::data_paths::DataPaths::new(directory.path()).pomodoro_sound();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"old sound").unwrap();
        let (generation, mut requests) = state.host.connect();
        let task = {
            let state = state.clone();
            tokio::spawn(async move { import(&state, "content://fixture/sound").await })
        };
        let request = requests.recv().await.unwrap();
        assert_eq!(request["method"], "fileResourceStream");
        let stream = state
            .bridge
            .attach_native(request["params"]["id"].as_str().unwrap(), generation)
            .unwrap();
        state
            .host
            .reply(generation, json!({"id":request["id"],"result":true}))
            .unwrap();
        stream.write(b"new sound".to_vec()).await.unwrap();
        match outcome {
            "failure" => stream.fail().await.unwrap(),
            "finish" => stream.finish().await.unwrap(),
            _ => core.stop_core().await,
        }
        let result = task.await.unwrap();
        let expected = if outcome == "finish" {
            assert!(result.is_ok());
            b"new sound"
        } else {
            assert!(result.is_err());
            b"old sound"
        };
        assert_eq!(std::fs::read(&destination).unwrap(), expected);
        assert_eq!(
            std::fs::read_dir(destination.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
        core.stop_core().await;
    }
}

#[tokio::test]
async fn invalid_or_stopped_sources_do_not_replace_sound() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let source = directory.path().join("selected");
    assert!(import(&state, source.to_str().unwrap()).await.is_err());
    assert!(
        import(&state, directory.path().to_str().unwrap())
            .await
            .is_err()
    );
    core.stop_core().await;
    std::fs::write(&source, b"stopped").unwrap();
    assert!(import(&state, source.to_str().unwrap()).await.is_err());
    assert!(
        !plain_rs::data_paths::DataPaths::new(directory.path())
            .pomodoro_sound()
            .exists()
    );
}
