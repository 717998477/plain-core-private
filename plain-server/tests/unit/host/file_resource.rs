use super::*;
use serde_json::json;

#[test]
fn read_authorization_checks_every_real_path_without_modifying_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("document.txt");
    std::fs::write(&path, b"original bytes").unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[path,directory.path()]})).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
    assert!(
        authorize_read(json!({"operation":"read","paths":[path,directory.path().join("missing")]}))
            .is_err()
    );
}

#[test]
fn read_authorization_never_grants_ambiguous_or_write_requests() {
    let directory = tempfile::tempdir().unwrap();
    for request in [
        json!({"paths":[directory.path()]}),
        json!({"type":"COPY","paths":[directory.path()]}),
        json!({"operation":"write","paths":[directory.path()]}),
        json!({"operation":"read","paths":[]}),
        json!({"operation":"read","paths":["relative.txt"]}),
        json!({"operation":"read","paths":[directory.path()],"extra":true}),
    ] {
        assert!(authorize_read(request).is_err());
    }
}

#[cfg(unix)]
#[test]
fn read_authorization_follows_existing_symlinks_and_rejects_broken_links() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.txt");
    let link = directory.path().join("link.txt");
    std::fs::write(&path, b"bytes").unwrap();
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[link]})).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert!(authorize_read(json!({"operation":"read","paths":[link]})).is_err());
}
