use super::*;

struct Stub(&'static [(&'static str, &'static [u8])]);

impl WebBundle for Stub {
    fn read(&self, relative: &str) -> Option<&'static [u8]> {
        self.0
            .iter()
            .find(|(key, _)| *key == relative)
            .map(|(_, bytes)| *bytes)
    }
}

#[test]
fn a_bundle_is_reached_through_a_shared_handle() {
    let bundle: SharedWebBundle = std::sync::Arc::new(Stub(&[("index.html", b"shell")]));
    assert_eq!(bundle.read("index.html"), Some(&b"shell"[..]));
    assert_eq!(bundle.read("missing.js"), None);
}
