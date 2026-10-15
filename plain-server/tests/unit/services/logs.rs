use super::*;
use std::io::{Cursor, Write};

#[test]
fn filtering_precedes_paging_and_large_offsets_remain_bounded() {
    let bytes = b"MATCH-old\r\n\nnoise\nmatch-middle\nnoise\nMATCH-new";
    assert_eq!(
        read(
            &mut Cursor::new(bytes),
            bytes.len() as u64,
            Some("match"),
            1,
            1
        )
        .unwrap(),
        vec!["match-middle"]
    );
    assert!(
        read(
            &mut Cursor::new(bytes),
            bytes.len() as u64,
            None,
            usize::MAX,
            10
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        read(&mut Cursor::new(bytes), bytes.len() as u64, None, 0, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn multibyte_lines_survive_reverse_block_boundaries() {
    let line = "αβγ".repeat(30000);
    let bytes = format!("old\n{line}\r\nlast").into_bytes();
    assert_eq!(
        read(&mut Cursor::new(&bytes), bytes.len() as u64, None, 0, 3).unwrap(),
        vec!["last".to_owned(), line, "old".to_owned()]
    );
}

#[test]
fn newest_page_stops_after_one_block() {
    struct Reader {
        file: Cursor<Vec<u8>>,
        reads: usize,
        bytes: usize,
    }
    impl Read for Reader {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            let count = self.file.read(bytes)?;
            self.bytes += count;
            Ok(count)
        }
    }
    impl Seek for Reader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.file.seek(position)
        }
    }
    let mut bytes = vec![b'x'; 256 * 1024];
    bytes.extend_from_slice(b"\nlatest\n");
    let len = bytes.len() as u64;
    let mut reader = Reader {
        file: Cursor::new(bytes),
        reads: 0,
        bytes: 0,
    };
    assert_eq!(read(&mut reader, len, None, 0, 1).unwrap(), vec!["latest"]);
    assert_eq!(reader.reads, 1);
    assert_eq!(reader.bytes, 64 * 1024);
}

#[tokio::test]
async fn clear_preserves_an_append_handle_and_missing_file_stays_missing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plain.log");
    let logs = Logs::new(Arc::new(Host::default()));
    logs.set_path(path.clone()).unwrap();
    logs.clear().await.unwrap();
    assert!(!path.exists());
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .unwrap();
    writer.write_all(b"old\n").unwrap();
    logs.clear().await.unwrap();
    writer.write_all(b"new\n").unwrap();
    assert_eq!(logs.read(String::new(), 0, 10).await.unwrap(), vec!["new"]);
    assert!(logs.set_path(PathBuf::from("relative.log")).is_err());
}

#[test]
fn missing_log_is_empty_but_read_failures_are_reported() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        read_lines_newest_first(&directory.path().join("missing.log"), None, 0, 10)
            .unwrap()
            .is_empty()
    );
    assert!(read_lines_newest_first(directory.path(), None, 0, 10).is_err());
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn native_main_uses_the_injected_logger_path_without_host_requests() {
    let directory = tempfile::tempdir().unwrap();
    let core =
        crate::assembly::server::ContentServer::open(&directory.path().join("core")).unwrap();
    let path = directory.path().join("plain.log");
    std::fs::write(&path, b"INFO old\nERROR current\n").unwrap();
    core.set_log_file(&path).unwrap();
    let (_generation, mut requests) = core.runtime_state().host.connect();
    let native = core.native_graphql().unwrap();
    let response = native
        .execute(async_graphql::Request::new(
            "{ appLogPath appLogs(query: \"text:ERROR\", offset: 0, limit: 1) }",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["appLogPath"], path.to_string_lossy().as_ref());
    assert_eq!(data["appLogs"], serde_json::json!(["ERROR current"]));
    let response = native
        .execute(async_graphql::Request::new("mutation { clearAppLogs }"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(std::fs::read(path).unwrap().is_empty());
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}
