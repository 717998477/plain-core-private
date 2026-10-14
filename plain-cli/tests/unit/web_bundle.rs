use super::*;

#[test]
fn the_embedded_table_is_sorted_for_lookup() {
    let keys = FILES.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}

#[test]
fn a_built_bundle_is_readable_by_relative_path() {
    let bundle = EmbeddedBundle;
    if FILES.is_empty() {
        return;
    }
    assert!(bundle.read("index.html").is_some());
    assert!(bundle.read("assets").is_none());
    assert!(bundle.read("missing-file.js").is_none());
}
