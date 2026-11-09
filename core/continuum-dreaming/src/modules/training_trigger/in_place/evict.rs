//! A bucket's superseded generations, deleted from the engine's train dir (card 4114a2c6).
//!
//! Every confirmed checkpoint publishes a generation (`<stem>-<id>.gguf.checkpoint`, `stem` naming
//! the whole bucket key: `in_place::bucket_stem`). Only the bucket's committed one is read again
//! (a reseat restores it), and a candidate gene is a copy in the genome store
//! (`snapshot::place_gene`), so the bucket's other generations go on every candidate sweep, under
//! the bucket's own gate: no save or restore of this bucket runs while it does. A name that is not
//! exactly this bucket's stem and a generation suffix is never touched.
//!
//! Shadows (`<stem>-<seat>.gguf` and their `.pending` save dirs) are NOT this sweep's: a shadow
//! absent from the trigger's seated minds may still be loaded and held in custody after an
//! unconfirmed stop, so its file goes only behind custody's fence (owed, card 4114a2c6; Codex on
//! f13a6abea).

use std::path::Path;

use uuid::Uuid;

/// PURE: the id in `name` when it is exactly `<stem>-<32 hex>.gguf` (a generation's bare name, as
/// its checkpoint row holds it), else None.
pub(crate) fn named_id(stem: &str, name: &str) -> Option<Uuid> {
    let (owner, id) = name.strip_suffix(".gguf")?.rsplit_once('-')?;
    (owner == stem && id.len() == 32).then(|| Uuid::try_parse(id).ok()).flatten()
}

/// PURE: the generation id in train-dir entry `name` when it is exactly one of `stem`'s
/// generations (`<stem>-<32 hex>.gguf.checkpoint`), else None.
pub(crate) fn generation_id(stem: &str, name: &str) -> Option<Uuid> {
    named_id(stem, &format!("{}.gguf", name.strip_suffix(".gguf.checkpoint")?))
}

/// Delete every generation of `stem` in `dir` but `newest`; the count deleted. Blocking: run off the
/// runtime's threads.
pub(crate) fn sweep(dir: &Path, stem: &str, newest: Uuid) -> Result<usize, String> {
    let listing = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut deleted = 0;
    for item in listing {
        let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
        let superseded = item.file_name().to_str().and_then(|name| generation_id(stem, name)).is_some_and(|id| id != newest);
        if !superseded {
            continue;
        }
        let path = item.path();
        match std::fs::remove_dir_all(&path) {
            Ok(()) => deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(deleted)
}
