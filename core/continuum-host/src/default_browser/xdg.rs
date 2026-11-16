//! Linux and the other Unixes: `xdg-open` hands a URL to the desktop's default browser.

pub(super) fn opener(url: &str) -> (&'static str, Vec<&str>) {
    ("xdg-open", vec![url])
}
