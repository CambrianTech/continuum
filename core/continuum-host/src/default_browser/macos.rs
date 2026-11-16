//! macOS: `open` hands a URL to the default browser.

pub(super) fn opener(url: &str) -> (&'static str, Vec<&str>) {
    ("open", vec![url])
}
