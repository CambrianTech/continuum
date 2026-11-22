//! The hot model tier: the model a node serves is read from its fast drive, never from the cold
//! archive (docs/architecture/STORAGE-SERVING-TIER-GOVERNOR.md, "ensure_hot_resident").
//!
//! The 5090 served a 27B from a 16 TB SATA hard disk for months: an install step pointed the
//! Hugging Face cache at the big drive, so the one path held both the archive and the model being
//! served (Codex, 2026-11-22: model and core exe on `D:` HDD, home and KV pages on `C:` NVMe). A
//! user installs with no agent and no programming knowledge, so the core decides: before an engine
//! loads a model stored on another drive than the continuum home, its files are copied up into
//! `<home>/models/hot/`, and the resolver reads that copy from then on. The archive keeps its
//! copy, so a hot copy is always a duplicate and safe for the served-weights store's owner
//! (`<home>/models`, a tracked class) to drop when it is not being served.
//!
//! Room is the disk-pressure monitor's own rule: a copy that would push the home's volume out of
//! `Normal` is not made, and the model is served from where it is stored, said loudly.

use std::path::{Path, PathBuf};
use std::time::Instant;

/// What promoting a stored model did; [`Promotion::serving_path`] is the file to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Promotion {
    /// It is already read from the fast drive: stored there, or copied up earlier.
    AlreadyHot(PathBuf),
    /// Copied up now.
    Promoted { path: PathBuf, bytes: u64, millis: u128 },
    /// The fast drive has no room without entering disk pressure: served from the archive.
    NoRoom { stored: PathBuf, need: u64, available: u64 },
    /// The drives or the home could not be read, or the copy failed: served from the archive.
    Failed { stored: PathBuf, why: String },
}

impl Promotion {
    pub fn serving_path(&self) -> &Path {
        match self {
            Promotion::AlreadyHot(path) | Promotion::Promoted { path, .. } => path,
            Promotion::NoRoom { stored, .. } | Promotion::Failed { stored, .. } => stored,
        }
    }
}

/// Whether the fast drive can take `need` more bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Room {
    /// The model is stored on the fast drive already.
    SameDrive,
    Fits,
    Short { available: u64 },
}

/// The complete hot copy of the stored GGUF `stored`, when every file of it is there at its size.
pub fn hot_copy(stored: &Path) -> Option<PathBuf> {
    let root = hot_root()?;
    complete_copy(stored, &root)
}

/// Copy the stored model up to the fast drive when it lives elsewhere and fits; the caller loads
/// [`Promotion::serving_path`]. Blocking (a whole model's bytes): call it off the async runtime.
pub fn promote(stored: &Path) -> Promotion {
    let Some(root) = hot_root() else {
        return Promotion::Failed { stored: stored.to_path_buf(), why: "the continuum home is unresolved".into() };
    };
    let promotion = promote_into(stored, &root, |need| room_on_home(stored, &root, need));
    report(&promotion);
    promotion
}

/// [`promote`] against an explicit hot root and room rule (the whole decision, testable on temp dirs).
pub fn promote_into(stored: &Path, root: &Path, room: impl FnOnce(u64) -> Room) -> Promotion {
    if let Some(path) = complete_copy(stored, root) {
        return Promotion::AlreadyHot(path);
    }
    let files = files_of(stored);
    let need: u64 = files.iter().map(|file| size(file)).sum();
    match room(need) {
        Room::SameDrive => Promotion::AlreadyHot(stored.to_path_buf()),
        Room::Short { available } => Promotion::NoRoom { stored: stored.to_path_buf(), need, available },
        Room::Fits => {
            let began = Instant::now();
            match copy_up(&files, &hot_dir(stored, root)) {
                Ok(()) => match complete_copy(stored, root) {
                    Some(path) => Promotion::Promoted { path, bytes: need, millis: began.elapsed().as_millis() },
                    None => Promotion::Failed { stored: stored.to_path_buf(), why: "the copy did not verify by size".into() },
                },
                Err(error) => Promotion::Failed { stored: stored.to_path_buf(), why: error.to_string() },
            }
        }
    }
}

fn hot_root() -> Option<PathBuf> {
    crate::paths::continuum_home().ok().map(|home| home.join("models").join("hot"))
}

/// The room on the home's drive for `need` bytes, by the disk-pressure monitor's thresholds.
fn room_on_home(stored: &Path, root: &Path, need: u64) -> Room {
    use crate::system_resources::disk_pressure::DiskPressureLevel;
    use crate::system_resources::drives::{detect_drives, drive_holding};
    let drives = detect_drives();
    let home = root.ancestors().find(|dir| dir.exists()).unwrap_or(root); // unwrap_or: the root itself names the drive when nothing exists yet
    let (Some(fast), Some(cold)) = (drive_holding(&drives, home), drive_holding(&drives, stored)) else {
        return Room::Short { available: 0 };
    };
    if fast.mount == cold.mount {
        return Room::SameDrive;
    }
    let used_after = fast.total_bytes.saturating_sub(fast.available_bytes).saturating_add(need);
    let ratio = used_after as f64 / fast.total_bytes.max(1) as f64;
    match DiskPressureLevel::from_pressure(ratio) {
        DiskPressureLevel::Normal => Room::Fits,
        _ => Room::Short { available: fast.available_bytes },
    }
}

/// The weight files and the sidecars an engine reads beside them (chat template, projector, json).
fn files_of(stored: &Path) -> Vec<PathBuf> {
    let mut files = super::shard_set(stored);
    if let Some(dir) = stored.parent() {
        let sidecars = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default(); // unwrap_or_default: a non-utf8 name is not a sidecar
            path.is_file() && (!name.ends_with(".gguf") || name.starts_with("mmproj"))
        });
        files.extend(sidecars);
    }
    files.sort();
    files.dedup();
    files
}

/// Where `stored`'s copy lives: one directory per stored directory, named by it and a hash of its
/// full path (two snapshots or two roots never share one).
fn hot_dir(stored: &Path, root: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let parent = stored.parent().unwrap_or(stored); // unwrap_or: a bare file name is its own directory key
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parent.hash(&mut hasher);
    let name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("model"); // unwrap_or: a non-utf8 directory still gets a hashed name
    root.join(format!("{name}-{:016x}", hasher.finish()))
}

/// The copy of `stored` under `root` when every file of it is there at its stored size.
fn complete_copy(stored: &Path, root: &Path) -> Option<PathBuf> {
    let dir = hot_dir(stored, root);
    let complete = files_of(stored).iter().all(|file| {
        let copy = dir.join(file.file_name().unwrap_or_default()); // unwrap_or_default: every listed file has a name
        copy.is_file() && size(&copy) == size(file)
    });
    complete.then(|| dir.join(stored.file_name().unwrap_or_default())) // unwrap_or_default: a stored GGUF has a name
}

/// Copy every file into `dir`, each to a `.part` name first and renamed only once all are written,
/// so a crash mid-copy never leaves a file the resolver reads as complete.
fn copy_up(files: &[PathBuf], dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::with_capacity(files.len());
    for file in files {
        let name = file.file_name().unwrap_or_default(); // unwrap_or_default: every listed file has a name
        let part = dir.join(format!("{}.part", name.to_string_lossy()));
        std::fs::copy(file, &part)?;
        written.push((part, dir.join(name)));
    }
    for (part, done) in written {
        std::fs::rename(part, done)?;
    }
    Ok(())
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) // unwrap_or: an unreadable file sizes 0, so its copy never verifies against it
}

fn report(promotion: &Promotion) {
    match promotion {
        Promotion::AlreadyHot(path) => continuum_base::probe!(class = "models.hot.resident", path = %path.display(), "the served model is read from the fast drive"),
        Promotion::Promoted { path, bytes, millis } => continuum_base::probe!(class = "models.hot.promoted", path = %path.display(), bytes = *bytes, millis = *millis as u64,
            "the served model was copied up from the archive drive to the fast drive"),
        Promotion::NoRoom { stored, need, available } => continuum_base::probe!(class = "models.hot.no_room", stored = %stored.display(), need = *need, available = *available,
            "the served model stays on its archive drive: the fast drive has no room without disk pressure; it loads and relaunches at that drive's speed"),
        Promotion::Failed { stored, why } => continuum_base::probe!(class = "models.hot.failed", stored = %stored.display(), why = %why,
            "the served model could not be copied up and is read from its archive drive"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).expect("test dir");
        for (name, body) in [("m-00001-of-00002.gguf", "aaaa"), ("m-00002-of-00002.gguf", "bb"), ("chat_template.jinja", "t"), ("mmproj-f16.gguf", "p"), ("other-q8.gguf", "zzzzzzzz")] {
            std::fs::write(dir.join(name), body).expect("test file");
        }
        dir.join("m-00001-of-00002.gguf")
    }

    // what this catches: the served model read off the archive drive (the 5090's 27B on a 16 TB HDD,
    // 2026-11-22). A model stored elsewhere is copied up with its shards and the sidecars the engine
    // reads beside it, never the other quantizations in its folder, and the resolver reads the copy;
    // a second launch copies nothing; no room serves the stored file; a half-written copy is not a copy.
    #[test]
    fn the_served_model_is_copied_up_once_and_read_from_the_fast_drive() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let stored = model(&tmp.path().join("cold/snap"));
        let root = tmp.path().join("hot");

        let short = promote_into(&stored, &root, |_| Room::Short { available: 3 });
        assert_eq!(short, Promotion::NoRoom { stored: stored.clone(), need: 8, available: 3 }, "shards 6 + template 1 + projector 1");
        assert_eq!(short.serving_path(), stored);

        let Promotion::Promoted { path, bytes, .. } = promote_into(&stored, &root, |_| Room::Fits) else { panic!("test: it fits") };
        assert_eq!(bytes, 8);
        let dir = path.parent().expect("a copy has a directory");
        for name in ["m-00001-of-00002.gguf", "m-00002-of-00002.gguf", "chat_template.jinja", "mmproj-f16.gguf"] {
            assert!(dir.join(name).is_file(), "{name} copied up");
        }
        assert!(!dir.join("other-q8.gguf").exists(), "another quantization in the folder is not this model");
        assert_eq!(complete_copy(&stored, &root), Some(path.clone()));
        assert_eq!(promote_into(&stored, &root, |_| panic!("test: a hot copy asks for no room")), Promotion::AlreadyHot(path.clone()));

        std::fs::write(dir.join("m-00002-of-00002.gguf"), "b").expect("truncate");
        assert_eq!(complete_copy(&stored, &root), None, "a copy that does not match by size is not complete");
        assert_eq!(promote_into(&stored, &root, |_| Room::SameDrive), Promotion::AlreadyHot(stored.clone()));
    }
}
