//! Read pins: open a file to read it while the path that reached it, and the bytes it names, stay
//! the ones that were verified. A pin never writes. What differs by OS is whether a held handle can
//! forbid others to replace what it holds: Windows can (a share mode that omits WRITE and DELETE),
//! so a pin there refuses a rewrite, a delete, or a rename of or onto the input or any directory on
//! its path; Unix has no such share mode, so a pin there is an ordinary read handle and the caller
//! re-verifies identity instead. The walk over a path's components is here once.

use std::fs::File;
use std::io;
use std::path::Path;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Whether a pin on this host forbids replacement (Windows) or is only a read handle (Unix).
pub const FORBIDS_REPLACEMENT: bool = os::FORBIDS_REPLACEMENT;

/// What a path component is to its pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// The input itself: its bytes cannot be rewritten while held.
    Input,
    /// A directory on its path: it cannot be renamed or deleted, while files INSIDE it can still
    /// be created, renamed and deleted (a read-only share on a directory froze every rename
    /// beneath it: the 5090, 2026-10-10, os error 32 across ~/.continuum for the lane's life).
    Ancestor,
}

/// Open `path` to read, pinned: on a host that forbids replacement, no other handle may write,
/// delete or rename it while the returned file is held.
pub fn file(path: &Path) -> io::Result<File> {
    os::open_file(path)
}

/// Hold every component of `path` (the input, then each ancestor directory), so the path stays
/// the path that was verified while the handles are held: links and directories are held
/// themselves, never followed. Empty on a host that cannot forbid replacement. `cancelled` is
/// checked before each component; a cancelled walk is `Interrupted` and holds nothing.
pub fn mapping(path: &Path, cancelled: &dyn Fn() -> bool) -> io::Result<Vec<File>> {
    let mut held = Vec::new();
    if !FORBIDS_REPLACEMENT {
        return Ok(held);
    }
    for (depth, component) in path.ancestors().enumerate() {
        if cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "read pin cancelled"));
        }
        let role = if depth == 0 { Component::Input } else { Component::Ancestor };
        held.push(os::hold_component(component, role)?);
    }
    Ok(held)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    // what this catches: a pin reads the verified bytes, holds one handle per path component
    // exactly where the host can forbid replacement (none where it cannot), and a cancelled walk
    // holds nothing, on whichever platform this runs. The Windows share semantics (renames inside
    // a held directory succeed; a rename of or onto the input fails) are pinned by the engine's
    // launch_inputs test, the consumer that measured them.
    #[test]
    fn a_pin_reads_its_bytes_and_holds_each_component_where_the_host_can() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("adapter.gguf");
        std::fs::write(&input, b"gene").unwrap();
        let mut text = Vec::new();
        file(&input).unwrap().read_to_end(&mut text).unwrap();
        assert_eq!(text, b"gene");
        let held = mapping(&input, &|| false).unwrap();
        let components = if FORBIDS_REPLACEMENT { input.ancestors().count() } else { 0 };
        assert_eq!(held.len(), components);
        drop(held);
        if FORBIDS_REPLACEMENT {
            assert_eq!(mapping(&input, &|| true).unwrap_err().kind(), io::ErrorKind::Interrupted);
        }
    }
}
