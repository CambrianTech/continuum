//! Unix-specific PATH checking for [`super::exists`].

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub fn exists(binary: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if is_executable_at(&dir, binary) {
            return true;
        }
    }
    // Fallback: check common system dirs even without PATH set.
    for fallback in ["/usr/bin", "/bin", "/usr/local/bin"] {
        if is_executable_at(Path::new(fallback), binary) {
            return true;
        }
    }
    false
}

fn is_executable_at(dir: &Path, name: &str) -> bool {
    let candidate = dir.join(name);
    match std::fs::metadata(&candidate) {
        Ok(meta) => {
            if !meta.is_file() {
                return false;
            }
            let mode = meta.permissions().mode();
            // Check execute bit for owner, group, or other.
            (mode & 0o111) != 0
        }
        Err(_) => false,
    }
}
