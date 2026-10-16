use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::io::{Cursor, Read};

#[tokio::test]
async fn archive_authorization_is_deduplicated_and_bounded_without_changing_zip_bytes() {
    for count in [2, 129] {
        let directory = tempfile::tempdir().unwrap();
        let core = ContentServer::open(directory.path()).unwrap();
        let state = core.runtime_state();
        state.file_platform.use_sdk_for_test();
        let mut entries = Vec::new();
        for index in 0..count {
            let path = directory.path().join(format!("source-{index}.txt"));
            std::fs::write(&path, format!("data-{index}")).unwrap();
            entries.push(ArchiveEntry {
                path,
                name: format!("entry-{index}.txt"),
            });
        }
        entries.push(ArchiveEntry {
            path: entries[0].path.clone(),
            name: "duplicate-source.txt".into(),
        });
        let host = state.host.clone();
        let (generation, mut requests) = host.connect();
        let responder = host.clone();
        let replies = tokio::spawn(async move {
            let mut sizes = Vec::new();
            let mut paths = std::collections::HashSet::new();
            while let Some(request) = requests.recv().await {
                assert_eq!(request["method"], "fileTaskAuthorize");
                assert_eq!(request["params"]["operation"], "read");
                let values = request["params"]["paths"].as_array().unwrap();
                sizes.push(values.len());
                for value in values {
                    assert!(paths.insert(value.as_str().unwrap().to_owned()));
                }
                responder
                    .reply(generation, json!({"id":request["id"],"result":true}))
                    .unwrap();
            }
            sizes
        });
        let mut reader = core
            .native_archives()
            .unwrap()
            .files(entries)
            .await
            .unwrap();
        let mut bytes = Vec::new();
        while let Some(chunk) = reader.read().await.unwrap() {
            bytes.extend(chunk);
        }
        assert_eq!(reader.size as usize, bytes.len());
        drop(reader);
        host.disconnect(generation);
        let sizes = replies.await.unwrap();
        assert_eq!(sizes, if count == 2 { vec![2] } else { vec![128, 1] });
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), count + 1);
        for index in 0..count {
            let mut bytes = String::new();
            archive
                .by_name(&format!("entry-{index}.txt"))
                .unwrap()
                .read_to_string(&mut bytes)
                .unwrap();
            assert_eq!(bytes, format!("data-{index}"));
        }
        let mut duplicate = String::new();
        archive
            .by_name("duplicate-source.txt")
            .unwrap()
            .read_to_string(&mut duplicate)
            .unwrap();
        assert_eq!(duplicate, "data-0");
        core.stop_core().await;
    }
}

#[tokio::test]
async fn malformed_archive_selection_is_rejected_before_any_platform_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    state.file_platform.use_sdk_for_test();
    let path = directory.path().join("source");
    std::fs::write(&path, b"bytes").unwrap();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let responder = host.clone();
    let replies = tokio::spawn(async move {
        let mut count = 0;
        while let Some(request) = requests.recv().await {
            count += 1;
            responder
                .reply(generation, json!({"id":request["id"],"result":true}))
                .unwrap();
        }
        count
    });
    let result = core
        .native_archives()
        .unwrap()
        .files(vec![
            ArchiveEntry {
                path: path.clone(),
                name: "valid.txt".into(),
            },
            ArchiveEntry {
                path,
                name: "../outside.txt".into(),
            },
        ])
        .await;
    assert!(result.is_err());
    host.disconnect(generation);
    assert_eq!(replies.await.unwrap(), 0);
    core.stop_core().await;
}
