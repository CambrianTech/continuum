//! Windows: `cmd /C start` opens a URL in the default browser; `start` takes an (empty) window
//! title before the URL.

pub(super) fn opener(url: &str) -> (&'static str, Vec<&str>) {
    ("cmd", vec!["/C", "start", "", url])
}
