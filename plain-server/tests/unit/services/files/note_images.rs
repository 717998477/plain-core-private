use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;

#[tokio::test]
async fn main_import_preserves_image_bytes_and_resolves_duplicate_names_in_rust() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let source = directory.path().join("source.png");
    std::fs::write(&source, b"image bytes").unwrap();
    let (_generation, mut requests) = state.host.connect();
    let schema = core.native_graphql().unwrap();
    for name in ["picture.png", "picture (1).png"] {
        let request = async_graphql::Request::new(
            r#"mutation($source:String!){importNoteImage(source:$source,fileName:"picture.png")}"#,
        )
        .variables(async_graphql::Variables::from_json(
            json!({"source":source}),
        ));
        let response = schema.execute(request).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let data = response.data.into_json().unwrap();
        assert_eq!(data["importNoteImage"], format!("app://note-images/{name}"));
        assert_eq!(
            std::fs::read(
                plain_rs::data_paths::DataPaths::new(directory.path())
                    .note_images()
                    .join(name)
            )
            .unwrap(),
            b"image bytes"
        );
    }
    assert!(source.exists());
    assert!(requests.try_recv().is_err());
    let remote = crate::graphql::executor::execute_graphql(
        &state.public,
        async_graphql::Request::new(
            r#"mutation{importNoteImage(source:"/missing",fileName:"remote.png")}"#,
        ),
        crate::graphql::executor::RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(!remote.errors.is_empty());
    core.stop_core().await;
}

#[tokio::test]
async fn concurrent_imports_do_not_overwrite_each_other_or_leave_temporary_files() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let mut tasks = Vec::new();
    for index in 0_u8..8 {
        let source = directory.path().join(format!("source-{index}"));
        std::fs::write(&source, [index; 32]).unwrap();
        let state = state.clone();
        tasks.push(tokio::spawn(async move {
            let reference = import(&state, source.to_str().unwrap(), "picture.png")
                .await
                .unwrap();
            let path = crate::services::files::references::resolve(
                state.db.clone(),
                state.directory.clone(),
                &reference,
            )
            .unwrap();
            assert_eq!(std::fs::read(path).unwrap(), [index; 32]);
            reference
        }));
    }
    let mut references = std::collections::BTreeSet::new();
    for task in tasks {
        assert!(references.insert(task.await.unwrap()));
    }
    assert_eq!(references.len(), 8);
    for index in 0..8 {
        let name = if index == 0 {
            "picture.png".into()
        } else {
            format!("picture ({index}).png")
        };
        assert!(references.contains(&format!("app://note-images/{name}")));
    }
    assert_eq!(
        std::fs::read_dir(plain_rs::data_paths::DataPaths::new(directory.path()).note_images())
            .unwrap()
            .count(),
        8
    );
    core.stop_core().await;
}

#[tokio::test]
async fn picked_names_remain_inside_the_store_and_failed_input_cleans_staging() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let source = directory.path().join("input");
    std::fs::write(&source, b"bytes").unwrap();
    let reference = import(&state, source.to_str().unwrap(), "../outside.png")
        .await
        .unwrap();
    assert_eq!(reference, "app://note-images/.._outside.png");
    assert!(!directory.path().join("outside.png").exists());
    assert_eq!(
        import(&state, source.to_str().unwrap(), "C:picture?.png ")
            .await
            .unwrap(),
        "app://note-images/C_picture_.png"
    );
    for name in ["CON.png", "nul", "LPT1.jpg", "com9.txt"] {
        assert!(
            import(&state, source.to_str().unwrap(), name)
                .await
                .is_err()
        );
    }
    assert!(
        import(&state, source.to_str().unwrap(), "bad\0name")
            .await
            .is_err()
    );
    assert!(
        import(&state, "/missing-input", "picture.png")
            .await
            .is_err()
    );
    let root = plain_rs::data_paths::DataPaths::new(directory.path()).note_images();
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    assert_eq!(
        std::fs::read(root.join(".._outside.png")).unwrap(),
        b"bytes"
    );
    core.stop_core().await;
    assert!(
        import(&state, source.to_str().unwrap(), "stopped.png")
            .await
            .is_err()
    );
}
