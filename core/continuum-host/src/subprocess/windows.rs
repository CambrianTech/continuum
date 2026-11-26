//! Windows-specific PATH checking for [`super::exists`].
//!
//! Checks each entry in `%PATH%` for a file matching `binary`, also considering PATHEXT extensions
//! (`.exe`, `.bat`, `.cmd`, etc.). Uses `std::fs::metadata` to verify the file exists and is not a
//! directory. No external `where.exe` call needed — pure stdlib.

use std::path::Path;

pub fn exists(binary: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if is_executable_at(&dir, binary) {
            return true;
        }
    }

    // Fallback: check common system dirs.
    let system_dirs = [
        "C:\\Windows\\System32",
        "C:\\Windows",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ];
    for dir in &system_dirs {
        if is_executable_at(Path::new(dir), binary) {
            return true;
        }
    }

    false
}

fn is_executable_at(dir: &Path, name: &str) -> bool {
    // If the name already has an extension, check it directly.
    if name.contains('.') {
        return file_exists_not_dir(&dir.join(name));
    }

    // Otherwise, try each PATHEXT extension.
    let exts = std::env::var_os("PATHEXT")
        .map(|ext| {
            std::env::split_paths(&ext)
                .filter_map(|e| e.to_string_lossy().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".EXE".into(), ".CMD".into(), ".BAT".into()]);

    for ext in exts {
        let candidate = dir.join(format!("{name}{ext}"));
        if file_exists_not_dir(&candidate) {
            return true;
        }
    }

    // Also try the bare name (some Windows setups allow direct execution).
    file_exists_not_dir(&dir.join(name))
}

fn file_exists_not_dir(path: &Path) -> bool {
    std::fs::metadata(path).map_or(false, |m| m.is_file())
}
