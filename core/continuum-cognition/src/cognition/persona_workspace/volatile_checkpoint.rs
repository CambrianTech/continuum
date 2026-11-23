use super::*;
use crate::cognition::result_store::checkpoint_handles as handles;

/// Her checkpoint's path, under the one continuum home (`paths::continuum_home`: `$CONTINUUM_HOME`,
/// a test build's home, else `~/.continuum`), never a home built by hand.
pub(super) fn volatile_path(persona_id: Uuid) -> std::io::Result<std::path::PathBuf> {
    crate::cognition::result_store::checkpoint_handles::checkpoint_path(persona_id)
}

/// What one checkpoint did: nothing (her state is where the last one left it), or wrote this many
/// bytes, and what her archive's tick did first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Checkpointed {
    Unchanged { archive: crate::cognition::working_memory::ArchiveTick },
    Written { bytes: usize, archive: crate::cognition::working_memory::ArchiveTick },
}

/// Everything a checkpoint writes, as the last write found it: her memory's generation and what
/// the writer stamps from outside it (`acting_card`, the handoff without its clock).
/// Compared, never serialized: an unmoved mark is a skipped write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    /// Which working memory (a respawn builds a new one, whose generation starts over).
    instance: u64,
    generation: u64,
    acting_card: Option<Uuid>,
    handoff: Option<crate::cognition::handoff::Handoff>,
}

/// The mark of each persona's last written checkpoint, one row per resident.
fn last_marks() -> &'static parking_lot::Mutex<std::collections::HashMap<Uuid, Mark>> {
    static MARKS: std::sync::OnceLock<parking_lot::Mutex<std::collections::HashMap<Uuid, Mark>>> = std::sync::OnceLock::new();
    MARKS.get_or_init(Default::default)
}

/// Persist the volatile tier — atomic tmp+rename so a crash mid-write never
/// leaves a torn file. Called under the registry checkpoint gate, on a blocking
/// worker. Failures propagate to both periodic and shutdown lifecycle receipts.
///
/// Her archive ticks first: an act that left RAM is on disk before the checkpoint that no longer
/// carries it replaces the one that did (an archive failure keeps the old checkpoint). The
/// checkpoint carries only her resident part, bounded by her view's fold edge, and a periodic one
/// whose [`Mark`] has not moved since the last write is skipped: no snapshot, no JSON, no bytes
/// (the M5, 2026-11-10: 12-46 MB rewritten whole every ~15 s per mind, 120 writes per 30 min at
/// 51-161 ms each).
pub(super) fn save_volatile(
    persona_id: Uuid,
    reason: SaveReason,
    staged_deadline: Option<std::time::Instant>,
    wm: &super::super::working_memory::WorkingMemory,
) -> std::io::Result<Checkpointed> {
    let path = volatile_path(persona_id)?;
    // The registry checkpoint gate serializes this tick. Archive retirement reads the
    // previous checkpoint to protect live bodies, so finish it before locking the replacement.
    let archive = wm.tick_archive()?;
    let _checkpoint_lock = checkpoint_adoption::lock_checkpoint(&path, true)?;
    let generation = wm.checkpoint_generation();
    // THE HANDOFF (card 49b5e806): composed from what her turns already stamped (rooting,
    // work/note, the seam walk) plus the two things only the seam knows — what tears this
    // life, read from the deploy claim the reboot holds, and what stands on disk at her root.
    let now_ms = EpochMs::now().millis();
    let home = crate::paths::continuum_home().ok();
    let torn_by = crate::cognition::handoff::torn_by_for(reason, home.as_deref(), now_ms);
    let acting_root = acting_root_of(persona_id);
    let handoff = crate::cognition::handoff::compose(
        persona_id,
        torn_by,
        acting_root.as_deref(),
        staged_deadline,
        now_ms,
    );
    // What she believes she holds, stamped by the one writer that knows whose memory this
    // is: her next wake compares it to the board (card c8303c32). A belief the board has
    // NOT yet answered (restored, then `Unknown` on every read so far) outranks where her
    // hands stand now: the card she stands at is on the board and recoverable from it, the
    // pending comparison is recoverable from nowhere else — a checkpoint that wrote the
    // ambient root would settle it by forgetting (Astra on #4315, round 2).
    let acting_card = wm
        .peek_restored_acting_card()
        .or_else(|| acting_card_of(persona_id));
    let mark = Mark {
        instance: wm.instance_id(),
        generation,
        acting_card,
        handoff: handoff.clone().map(|mut handoff| {
            handoff.written_ms = 0;
            handoff
        }),
    };
    if matches!(reason, SaveReason::Periodic) && last_marks().lock().get(&persona_id) == Some(&mark) {
        return Ok(Checkpointed::Unchanged { archive });
    }
    let mut snapshot = wm.snapshot();
    snapshot.handoff = handoff;
    snapshot.acting_card = acting_card;
    let persisted = PersistedVolatile { wm: snapshot };
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{} names no checkpoint file", path.display())));
    };
    let bytes = serde_json::to_vec(&persisted)?; // file on disk: her checkpoint, read back at her next spawn
    // The list beside it never names fewer handles than the checkpoint on disk: the old one's and
    // this one's before the replace, this one's alone after (`spill::checkpoint_handles`).
    let names = named_handles(&persisted);
    let before = match handles::listed(&path)? {
        Some(listed) => listed,
        // No list yet (a checkpoint from before lists): the old checkpoint's own handles.
        None => read_volatile(&path).ok().flatten().map(|old| named_handles(&old)).unwrap_or_default(), // JUSTIFIED: an unreadable old checkpoint names nothing a restore could reach
    };
    handles::write(&path, &before.union(&names).cloned().collect())?;
    // Staged beside it in her held directory, synced, and renamed over it: a reader sees the last
    // checkpoint or this one, never half, and the placement survives power loss.
    let mut staging = continuum_host::durable::Staging::new(&continuum_host::confine::open_root(dir)?)?;
    staging.write_all(&bytes)?;
    staging.replace(std::path::Path::new(name))?;
    handles::write(&path, &names)?;
    last_marks().lock().insert(persona_id, mark);
    Ok(Checkpointed::Written { bytes: bytes.len(), archive })
}

/// The saved results a checkpoint names: its results ring AND its collapsed pointers (each pointer
/// is how she reopens a result that left the ring, so it is a reference exactly as a ring entry is).
/// The checkpoint's writer keeps this set in the list beside it, which eviction reads, so a saved
/// result survives the restart a deploy performs, a mind that has not restored yet, and a persona
/// that is offline (Astra on card 9197cd01).
pub(super) fn named_handles(persisted: &PersistedVolatile) -> std::collections::HashSet<String> {
    persisted.wm.reachable_handles()
}

/// Load the previous life's volatile tier under the same owner lock as saves
/// and adoption. Only an absent checkpoint permits a fresh resident; inability
/// to read or decode an existing one must reach the registration caller.
pub(super) fn load_volatile(persona_id: Uuid) -> std::io::Result<Option<PersistedVolatile>> {
    load_volatile_at(&volatile_path(persona_id)?)
}

pub(super) fn load_volatile_at(path: &std::path::Path) -> std::io::Result<Option<PersistedVolatile>> {
    let _lock = checkpoint_adoption::lock_checkpoint(path, true)?;
    read_volatile(path)
}

fn read_volatile(path: &std::path::Path) -> std::io::Result<Option<PersistedVolatile>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}
