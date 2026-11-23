//! Which saved results a persona's checkpoint still names, as a small file beside the checkpoint.
//!
//! Eviction must keep a saved result while any persona's checkpoint names it, including a persona
//! that is not resident in this process (Astra on card 9197cd01). Spill used to learn that by asking
//! the workspace to decode the whole checkpoint (her working-memory snapshot), which made the result
//! store depend on the mind's memory layer above it, a cycle that kept continuum-cognition from being
//! cut into crates. Now the checkpoint's writer keeps a list of its handles beside it (one per line),
//! and the store reads that list: a file contract, not a code dependency, and no snapshot decoded per
//! eviction.
//!
//! The list never names fewer handles than the checkpoint on disk: before a checkpoint is replaced the
//! list holds the old checkpoint's handles and the new one's, and after it lands, the new one's alone.
//! A checkpoint with no list beside it (written before this, or mid-adoption) protects everything of
//! that persona's: the store fails closed, exactly as an unreadable checkpoint did.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

use uuid::Uuid;

/// The checkpoint file's name in her persona directory.
const CHECKPOINT: &str = "volatile.json";
/// The list of handles beside it.
const HANDLES: &str = "volatile.handles";

/// Where a persona's checkpoint lives: the one owner of that path.
pub(crate) fn checkpoint_path(persona_id: Uuid) -> io::Result<PathBuf> {
    let home = crate::paths::continuum_home()
        .map_err(|why| io::Error::new(io::ErrorKind::NotFound, format!("cannot resolve the continuum home: {why}")))?;
    Ok(home.join("personas").join(persona_id.to_string()).join(CHECKPOINT))
}

fn handles_path(checkpoint: &Path) -> PathBuf {
    checkpoint.with_file_name(HANDLES)
}

/// Write the list beside `checkpoint`: staged, synced and renamed, so a reader sees a whole list.
pub(crate) fn write(checkpoint: &Path, handles: &HashSet<String>) -> io::Result<()> {
    let dir = checkpoint
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{} names no directory", checkpoint.display())))?;
    let mut sorted: Vec<&str> = handles.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut body = sorted.join("\n");
    body.push('\n');
    let mut staging = continuum_host::durable::Staging::new(&continuum_host::confine::open_root(dir)?)?;
    staging.write_all(body.as_bytes())?;
    staging.replace(Path::new(HANDLES))
}

/// Remove the list (a checkpoint about to be replaced by one whose handles the writer does not
/// know): until a list is written again, the checkpoint protects everything of that persona's.
pub(crate) fn clear(checkpoint: &Path) -> io::Result<()> {
    match std::fs::remove_file(handles_path(checkpoint)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The handles the list beside `checkpoint` names, when there is a list.
pub(crate) fn listed(checkpoint: &Path) -> io::Result<Option<HashSet<String>>> {
    match std::fs::read_to_string(handles_path(checkpoint)) {
        Ok(body) => Ok(Some(body.lines().filter(|line| !line.is_empty()).map(str::to_owned).collect())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// What the persona's checkpoint protects: the handles its list names, cached by the list's
/// modification time. No checkpoint protects nothing; a checkpoint with no list beside it is an
/// `Err`, so the caller fails closed and evicts nothing of that persona's.
pub(crate) fn protected(persona_id: Uuid) -> io::Result<Arc<HashSet<String>>> {
    protected_at(&checkpoint_path(persona_id)?)
}

pub(crate) fn protected_at(checkpoint: &Path) -> io::Result<Arc<HashSet<String>>> {
    type Cached = (SystemTime, Arc<HashSet<String>>);
    static CACHE: OnceLock<parking_lot::Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    let list = handles_path(checkpoint);
    let modified = match std::fs::metadata(&list) {
        Ok(meta) => meta.modified()?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return match std::fs::metadata(checkpoint) {
                Ok(_) => Err(io::Error::new(io::ErrorKind::NotFound, format!("{} has no handle list beside it", checkpoint.display()))),
                Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(Arc::default()),
                Err(other) => Err(other),
            };
        }
        Err(error) => return Err(error),
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, handles)) = cache.lock().get(&list) {
        if *at == modified {
            return Ok(Arc::clone(handles));
        }
    }
    let handles = Arc::new(listed(checkpoint)?.unwrap_or_default()); // JUSTIFIED: the list was just seen; one removed since names nothing
    cache.lock().insert(list, (modified, Arc::clone(&handles)));
    Ok(handles)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the store evicting a result a checkpoint still names, or depending on the
    // workspace to know. The list protects what it names; no checkpoint protects nothing; a checkpoint
    // without a list protects everything (an Err the caller fails closed on); clearing it returns there.
    #[test]
    fn a_checkpoints_list_protects_what_it_names_and_its_absence_fails_closed() {
        let dir = tempfile::tempdir().expect("test: a temp dir");
        let checkpoint = dir.path().join(CHECKPOINT);
        assert!(protected_at(&checkpoint).is_ok_and(|h| h.is_empty()), "no checkpoint protects nothing");
        std::fs::write(&checkpoint, b"{}").expect("test: a checkpoint");
        assert!(protected_at(&checkpoint).is_err(), "a checkpoint without its list fails closed");
        let names: HashSet<String> = ["aaaa", "bbbb"].into_iter().map(str::to_owned).collect();
        write(&checkpoint, &names).expect("test: the list is written");
        assert_eq!(protected_at(&checkpoint).ok().as_deref(), Some(&names), "the list protects what it names");
        clear(&checkpoint).expect("test: the list is cleared");
        assert!(protected_at(&checkpoint).is_err(), "cleared, the checkpoint fails closed again");
    }
}
