use super::*;
use std::io::Write;

fn archive(path: &Path, files: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

fn source(paths: &[&Path]) -> Source {
    Source {
        archives: paths
            .iter()
            .map(|path| path.to_str().unwrap().to_owned())
            .collect(),
        prefix: "assets/web/".into(),
        ..Default::default()
    }
}

#[test]
fn archive_assets_include_extensionless_files_and_dotted_directories_with_split_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.apk");
    let split = dir.path().join("split.apk");
    archive(
        &base,
        &[
            ("assets/web/index.html", b"base"),
            ("assets/web/v1.2/LICENSE", b"license"),
            ("assets/web/robots", b"robots"),
            ("unrelated/payload", b"skip"),
        ],
    );
    archive(
        &split,
        &[
            ("assets/web/index.html", b"overlay"),
            ("assets/web/app.js", b"script"),
        ],
    );
    let cache = CachePaths::new(&dir.path().join("cache"));
    let source = source(&[&base, &split]);
    let mut opens = Vec::new();
    let root = prepare_with(&cache, &source, |path| opens.push(path.to_owned()))
        .unwrap()
        .unwrap();
    assert!(root.starts_with(cache.web_assets()));
    assert_eq!(fs::read(root.join("index.html")).unwrap(), b"overlay");
    assert_eq!(fs::read(root.join("v1.2/LICENSE")).unwrap(), b"license");
    assert_eq!(fs::read(root.join("robots")).unwrap(), b"robots");
    assert!(!root.join("unrelated").exists());
    assert_eq!(opens.len(), 2);
    assert_eq!(
        prepare_with(&cache, &source, |_| panic!(
            "cached assets must not reopen ZIPs"
        ))
        .unwrap(),
        Some(root.clone())
    );
    fs::remove_file(root.join(".complete")).unwrap();
    assert_eq!(
        prepare_with(&cache, &source, |path| opens.push(path.to_owned())).unwrap(),
        Some(root)
    );
    assert_eq!(opens.len(), 4);
}

#[test]
fn replacing_the_packaged_source_invalidates_the_cached_projection() {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    let cache = CachePaths::new(&dir.path().join("cache"));
    let source = source(&[&apk]);
    archive(&apk, &[("assets/web/index.html", b"first")]);
    let first = prepare(&cache, &source).unwrap().unwrap();
    archive(
        &apk,
        &[(
            "assets/web/index.html",
            b"replacement with different length",
        )],
    );
    let second = prepare(&cache, &source).unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(
        fs::read(second.join("index.html")).unwrap(),
        b"replacement with different length"
    );
}

#[test]
fn absent_shell_and_failed_extraction_never_publish_a_complete_cache() {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    let cache = CachePaths::new(&dir.path().join("cache"));
    let source = source(&[&apk]);
    archive(&apk, &[("assets/web/app.js", b"script")]);
    assert!(prepare(&cache, &source).unwrap().is_none());
    assert_eq!(fs::read_dir(cache.web_assets()).unwrap().count(), 0);
    for bad in [
        "assets/web/../../escape",
        "assets/web/a\\b",
        "assets/web/.complete",
    ] {
        archive(&apk, &[("assets/web/index.html", b"shell"), (bad, b"bad")]);
        assert!(prepare(&cache, &source).is_err(), "{bad}");
        assert_eq!(fs::read_dir(cache.web_assets()).unwrap().count(), 0);
        assert!(!dir.path().join("escape").exists());
    }
}

#[test]
fn empty_source_is_optional_but_ambiguous_or_relative_sources_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let cache = CachePaths::new(&dir.path().join("cache"));
    assert!(prepare(&cache, &Source::default()).unwrap().is_none());
    assert!(!cache.root().exists());
    for source in [
        Source {
            directory: "relative".into(),
            ..Default::default()
        },
        Source {
            directory: dir.path().to_str().unwrap().into(),
            archives: vec!["package.apk".into()],
            ..Default::default()
        },
        Source {
            prefix: "../".into(),
            ..Default::default()
        },
        Source {
            archives: vec!["relative.apk".into()],
            prefix: "assets/web/".into(),
            ..Default::default()
        },
    ] {
        assert!(prepare(&cache, &source).is_err());
    }
}

#[cfg(unix)]
#[test]
fn packaged_directories_are_used_directly_without_cache_writes_or_archive_reads() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("web");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("index.html"), b"shell").unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();
    let cache = CachePaths::new(&dir.path().join("cache"));
    let result = prepare_with(
        &cache,
        &Source {
            directory: root.to_str().unwrap().into(),
            ..Default::default()
        },
        |_| panic!("directory source cannot open archives"),
    );
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result.unwrap(), Some(root.canonicalize().unwrap()));
    assert!(!cache.root().exists());
}

#[cfg(unix)]
#[test]
fn extraction_rejects_archive_links_and_cache_directory_links() {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    let mut zip = zip::ZipWriter::new(fs::File::create(&apk).unwrap());
    zip.add_symlink(
        "assets/web/index.html",
        "../../outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    let cache = CachePaths::new(&dir.path().join("cache"));
    assert!(prepare(&cache, &source(&[&apk])).is_err());
    assert_eq!(fs::read_dir(cache.web_assets()).unwrap().count(), 0);
    fs::remove_dir(cache.web_assets()).unwrap();
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, cache.web_assets()).unwrap();
    archive(&apk, &[("assets/web/index.html", b"shell")]);
    assert!(prepare(&cache, &source(&[&apk])).is_err());
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn application_cache_clear_keeps_served_assets_and_prunes_inactive_versions_without_reopening_archives()
 {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    archive(&apk, &[("assets/web/index.html", b"first")]);
    let server = crate::assembly::server::ContentServer::open_with_cache(
        &dir.path().join("data"),
        &dir.path().join("cache"),
    )
    .unwrap();
    server.set_web_assets(source(&[&apk])).await.unwrap();
    let state = server.runtime_state();
    let first = state.web_root.read().unwrap().clone().unwrap();
    archive(
        &apk,
        &[
            ("assets/web/index.html", b"replacement shell"),
            ("assets/web/assets/app.js", b"current script"),
        ],
    );
    server.set_web_assets(source(&[&apk])).await.unwrap();
    let active = state.web_root.read().unwrap().clone().unwrap();
    assert_ne!(first, active);
    let marker = fs::read(active.join(".complete")).unwrap();
    fs::write(state.cache.root().join("scratch"), b"remove").unwrap();
    fs::remove_file(&apk).unwrap();
    crate::services::cache::clear_application(&state)
        .await
        .unwrap();
    assert!(!first.exists());
    assert!(!state.cache.root().join("scratch").exists());
    assert_eq!(fs::read(active.join(".complete")).unwrap(), marker);
    assert_eq!(*state.web_root.read().unwrap(), Some(active.clone()));
    let response = crate::http::routes::static_host::fallback(
        axum::extract::State(state.clone()),
        axum::http::Request::builder()
            .uri("/assets/app.js")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap()
            .as_ref(),
        b"current script"
    );
    crate::services::cache::clear_application(&state)
        .await
        .unwrap();
    assert_eq!(
        fs::read(active.join("index.html")).unwrap(),
        b"replacement shell"
    );
    server.shutdown().await;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn application_cache_clear_recovers_a_missing_active_extraction() {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    archive(&apk, &[("assets/web/index.html", b"restore")]);
    let server = crate::assembly::server::ContentServer::open_with_cache(
        &dir.path().join("data"),
        &dir.path().join("cache"),
    )
    .unwrap();
    server.set_web_assets(source(&[&apk])).await.unwrap();
    let state = server.runtime_state();
    let active = state.web_root.read().unwrap().clone().unwrap();
    fs::remove_dir_all(&active).unwrap();
    crate::services::cache::clear_application(&state)
        .await
        .unwrap();
    assert_eq!(fs::read(active.join("index.html")).unwrap(), b"restore");
    assert_eq!(*state.web_root.read().unwrap(), Some(active));
    server.shutdown().await;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn cancelled_publication_waiter_keeps_asset_publication_serialized_with_cache_clear() {
    let dir = tempfile::tempdir().unwrap();
    let apk = dir.path().join("base.apk");
    archive(&apk, &[("assets/web/index.html", b"first")]);
    let server = crate::assembly::server::ContentServer::open_with_cache(
        &dir.path().join("data"),
        &dir.path().join("cache"),
    )
    .unwrap();
    server.set_web_assets(source(&[&apk])).await.unwrap();
    let state = server.runtime_state();
    let first = state.web_root.read().unwrap().clone().unwrap();
    archive(
        &apk,
        &[("assets/web/index.html", b"replacement while clearing")],
    );
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let preparation = tokio::task::spawn_blocking(move || {
        let _guard = PREPARATION.lock().unwrap();
        let _ = entered.send(());
        let _ = released.blocking_recv();
    });
    waiting.await.unwrap();
    let configuring_state = state.clone();
    let new_source = source(&[&apk]);
    let configuring = tokio::spawn(async move { configure(&configuring_state, new_source).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while state.web_asset_changes.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    configuring.abort();
    assert!(configuring.await.unwrap_err().is_cancelled());
    let mut clearing = Box::pin(clear_cache(&state));
    assert!(futures_util::poll!(&mut clearing).is_pending());
    assert_eq!(fs::read(first.join("index.html")).unwrap(), b"first");
    release.send(()).unwrap();
    preparation.await.unwrap();
    clearing.await.unwrap();
    let active = state.web_root.read().unwrap().clone().unwrap();
    assert_ne!(first, active);
    assert!(!first.exists());
    assert_eq!(
        fs::read(active.join("index.html")).unwrap(),
        b"replacement while clearing"
    );
    server.shutdown().await;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::test]
async fn directory_assets_inside_cache_are_retained_by_their_canonical_path() {
    let dir = tempfile::tempdir().unwrap();
    let server = crate::assembly::server::ContentServer::open_with_cache(
        &dir.path().join("data"),
        &dir.path().join("cache"),
    )
    .unwrap();
    let state = server.runtime_state();
    let assets = state.cache.root().join("packaged");
    fs::create_dir_all(assets.join("child")).unwrap();
    fs::write(assets.join("index.html"), b"directory shell").unwrap();
    fs::write(state.cache.root().join("scratch"), b"remove").unwrap();
    server
        .set_web_assets(Source {
            directory: assets.join("child/..").to_str().unwrap().into(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        *state.web_root.read().unwrap(),
        Some(assets.canonicalize().unwrap())
    );
    crate::services::cache::clear_application(&state)
        .await
        .unwrap();
    assert_eq!(
        fs::read(assets.join("index.html")).unwrap(),
        b"directory shell"
    );
    assert!(!state.cache.root().join("scratch").exists());
    server.shutdown().await;
}
