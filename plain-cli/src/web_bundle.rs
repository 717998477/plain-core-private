use plain_server::http::routes::web_bundle::WebBundle;

include!(concat!(env!("OUT_DIR"), "/web_bundle.rs"));

pub struct EmbeddedBundle;

impl WebBundle for EmbeddedBundle {
    fn read(&self, relative: &str) -> Option<&'static [u8]> {
        FILES
            .binary_search_by(|(key, _)| (*key).cmp(&relative))
            .ok()
            .map(|index| FILES[index].1)
    }
}

#[cfg(test)]
#[path = "../tests/unit/web_bundle.rs"]
mod tests;
