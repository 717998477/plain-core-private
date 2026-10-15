use super::*;

async fn open() -> (
    Arc<NativeResources>,
    NativeResourceReader,
    NativeResourceStream,
    watch::Sender<bool>,
    u64,
) {
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let resources = Arc::new(NativeResources::new(host.clone()));
    let (stop, stopped) = watch::channel(false);
    let opening = {
        let resources = resources.clone();
        tokio::spawn(async move {
            resources
                .read_native("content://fixture/document", stopped)
                .await
        })
    };
    let request = requests.recv().await.unwrap();
    assert_eq!(request["method"], "fileResourceStream");
    let id = request["params"]["id"].as_str().unwrap();
    assert!(resources.attach_native(id, generation + 1).is_err());
    let stream = resources.attach_native(id, generation).unwrap();
    assert!(resources.attach_native(id, generation).is_err());
    host.reply(generation, json!({"id":request["id"],"result":null}))
        .unwrap();
    let reader = opening.await.unwrap().unwrap();
    let metadata: Value =
        serde_json::from_str(&stream.next_request().await.unwrap().unwrap()).unwrap();
    assert_eq!(metadata, json!({"path":"content://fixture/document"}));
    (resources, reader, stream, stop, generation)
}

#[tokio::test]
async fn native_stream_preserves_bytes_and_has_one_terminal_receipt() {
    let (_resources, mut reader, stream, _stop, _generation) = open().await;
    assert!(stream.write(Vec::new()).await.is_err());
    assert!(stream.write(vec![0; 64 * 1024 + 1]).await.is_err());
    let bytes = (0..64 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    stream.write(bytes.clone()).await.unwrap();
    assert_eq!(reader.read().await.unwrap(), Some(bytes));
    stream.finish().await.unwrap();
    assert!(stream.finish().await.is_err());
    assert!(stream.write(vec![1]).await.is_err());
    assert_eq!(reader.read().await.unwrap(), None);
    assert_eq!(reader.read().await.unwrap(), None);
}

#[tokio::test]
async fn native_stream_failure_reaches_the_reader() {
    let (_resources, mut reader, stream, _stop, _generation) = open().await;
    stream.fail().await.unwrap();
    assert!(reader.read().await.is_err());
    assert_eq!(reader.read().await.unwrap(), None);
    assert!(stream.finish().await.is_err());
}

#[tokio::test]
async fn cancellation_closes_both_directions() {
    let (_resources, mut reader, stream, stop, _generation) = open().await;
    stop.send_replace(true);
    assert!(reader.read().await.is_err());
    assert!(stream.write(vec![1]).await.is_err());
    assert!(stream.next_request().await.is_err());
}

#[tokio::test]
async fn replacing_the_host_rejects_old_resource_handles() {
    let (resources, mut reader, stream, _stop, generation) = open().await;
    let (replacement, _requests) = resources.host.connect();
    assert_ne!(generation, replacement);
    assert!(reader.read().await.is_err());
    assert!(stream.write(vec![1]).await.is_err());
    assert!(stream.next_request().await.is_err());
}

#[tokio::test]
async fn cancelling_before_attachment_releases_registration_and_capacity() {
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let resources = Arc::new(NativeResources::new(host.clone()));
    let (_stop, stopped) = watch::channel(false);
    let opening = {
        let resources = resources.clone();
        tokio::spawn(async move {
            resources
                .read_native("content://fixture/document", stopped)
                .await
        })
    };
    let request = requests.recv().await.unwrap();
    let id = request["params"]["id"].as_str().unwrap().to_owned();
    host.reply(generation, json!({"id":request["id"],"result":null}))
        .unwrap();
    opening.abort();
    let Err(error) = opening.await else {
        panic!("cancelled resource open completed")
    };
    assert!(error.is_cancelled());
    assert!(resources.attach_native(&id, generation).is_err());
    assert!(resources.pending.lock().unwrap().is_empty());
    assert_eq!(resources.capacity.available_permits(), 32);
}
