//! Lexical path equality under the host's spelling rules, without opening a file.
//! This does not establish file identity or resolve symlinks.
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(not(windows), path = "other.rs")]
mod os;

pub fn equal(left: &std::path::Path, right: &std::path::Path) -> bool {
    let mut left = left.components();
    let mut right = right.components();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if os::equal(a.as_os_str(), b.as_os_str()) => {}
            _ => return false,
        }
    }
}
