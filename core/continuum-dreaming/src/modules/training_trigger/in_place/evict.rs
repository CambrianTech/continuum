//! A bucket's old files in the engine's train dir, deleted (card 4114a2c6).
//!
//! Learning in place leaves, per bucket, a shadow per seat (`<persona>-<trait>-<seat>.gguf`, with
//! its `.pending` save directory) and a generation per confirmed checkpoint
//! (`<persona>-<trait>-<id>.gguf.checkpoint`). Only two are ever read again: the bucket's newest
//! confirmed generation (a reseat restores it) and the shadow a seat holds now. A candidate gene is
//! a copy in the genome store (`snapshot::place_gene`), never a file here. So everything else of the
//! bucket's goes, on every candidate sweep, under the bucket's own gate: no save of this bucket is
//! in flight while it runs. A name this sweep cannot parse as exactly this bucket's (a staged gene,
//! another bucket whose trait shares a prefix, a file another owner wrote) is never touched.

use std::collections::HashSet;
use std::path::Path;

use uuid::Uuid;

/// What one of a bucket's files in the train dir is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    Generation(Uuid),
    Shadow(Uuid),
    /// A shadow's save directory, named for its shadow's seat.
    Pending(Uuid),
}

/// PURE: what `name` is when it is exactly the bucket `prefix`'s (`<persona>-<trait>`), else None.
pub(crate) fn entry(prefix: &str, name: &str) -> Option<Entry> {
    let (stem, kind): (&str, fn(Uuid) -> Entry) = if let Some(stem) = name.strip_suffix(".gguf.checkpoint") {
        (stem, Entry::Generation)
    } else if let Some(stem) = name.strip_suffix(".gguf.pending") {
        (stem, Entry::Pending)
    } else if let Some(stem) = name.strip_suffix(".gguf") {
        (stem, Entry::Shadow)
    } else {
        return None;
    };
    let (owner, id) = stem.rsplit_once('-')?;
    if owner != prefix || id.len() != 32 {
        return None;
    }
    Uuid::try_parse(id).ok().map(kind)
}

/// PURE: whether the bucket's `entry` is stale. `newest` is its committed generation's id; `seated`
/// the seats any engine holds for it now, or None when that could not be read (then no shadow goes).
pub(crate) fn stale(entry: Entry, newest: Uuid, seated: Option<&HashSet<Uuid>>) -> bool {
    match entry {
        Entry::Generation(id) => id != newest,
        Entry::Shadow(seat) | Entry::Pending(seat) => seated.is_some_and(|held| !held.contains(&seat)),
    }
}

/// Delete the stale entries of bucket `prefix` in `dir`; the count deleted. Blocking: run off the
/// runtime's threads.
pub(crate) fn sweep(dir: &Path, prefix: &str, newest: Uuid, seated: Option<&HashSet<Uuid>>) -> Result<usize, String> {
    let listing = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut deleted = 0;
    for item in listing {
        let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = item.file_name();
        let Some(found) = name.to_str().and_then(|name| entry(prefix, name)) else {
            continue;
        };
        if !stale(found, newest, seated) {
            continue;
        }
        let path = item.path();
        let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        match removed {
            Ok(()) => deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(deleted)
}
