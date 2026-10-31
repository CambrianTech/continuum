//! A file's identity on its volume: the pair that names one file whatever path reaches it, so a
//! path that was replaced (rename or copy over it) reads as another file. What differs by OS is
//! the call that reads the pair (`dev`/`ino`; volume serial and file index); the comparison is
//! here once.

use std::fs::File;
use std::io;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// One file on one volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub volume: u64,
    pub index: u64,
}

/// The identity of an open file.
pub fn of(file: &File) -> io::Result<FileIdentity> {
    os::identity(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two handles on one file read the same identity, and a path replaced by
    // a rename reads as another file, on whichever platform this runs.
    #[test]
    fn a_replaced_path_is_another_file() {
        let dir = tempfile::tempdir().unwrap();
        let (path, other) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::write(&path, b"one").unwrap();
        std::fs::write(&other, b"two").unwrap();
        let first = of(&File::open(&path).unwrap()).unwrap();
        assert_eq!(first, of(&File::open(&path).unwrap()).unwrap());
        std::fs::rename(&other, &path).unwrap();
        assert_ne!(first, of(&File::open(&path).unwrap()).unwrap());
    }
}
