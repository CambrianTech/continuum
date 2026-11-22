//! The system volume and how a path is spelled for comparing it with mount points. What differs by
//! OS is where the system lives (`/`; `%SystemRoot%`) and whether a path carries a verbatim prefix
//! (`\\?\C:` reads as `C:` beside a mount point); callers compare once, here's the spelling.

use std::path::{Path, PathBuf};

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Where the operating system lives: the volume a drive inventory calls System. `None` when the
/// OS does not say (a Windows host without `SystemRoot`).
pub fn system_root() -> Option<PathBuf> {
    os::system_root()
}

/// `path` spelled the way mount points are, for comparison only: filesystem operations keep the
/// path's native spelling (and Windows long-path support).
pub fn mount_comparison_path(path: &Path) -> PathBuf {
    os::mount_comparison_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a host whose system volume the inventory cannot name (every drive would
    // then compete for the cold tier, the system drive included)
    #[test]
    fn the_system_root_is_named() {
        assert!(system_root().is_some());
    }

    // what this catches: a comparison spelling that changes an ordinary absolute path
    #[test]
    fn an_ordinary_path_compares_as_itself() {
        let p = std::env::temp_dir();
        assert_eq!(mount_comparison_path(&p).components().count(), p.components().count());
    }
}
