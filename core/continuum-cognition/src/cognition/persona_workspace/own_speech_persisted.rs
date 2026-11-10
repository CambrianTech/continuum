use super::*;

/// Her own-speech rings on disk. Room-keyed since 2026-08-14 — a ring restored
/// without its room would re-create the cross-room repetition fact the keying
/// fix exists to kill.
///
/// `Legacy` is the pre-room-scoping shape (a flat `Vec<String>`) and exists so
/// an older `volatile.json` still PARSES: a torn mind-file that fails to
/// deserialize is a silent blank wake, which is strictly worse than a dropped
/// scratchpad. Legacy utterances carry no room, so they cannot be attributed
/// and are discarded on load — covered by `hydrate_speech_rings`, which
/// re-seeds from the durable transcript where every message HAS a room (#265).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum OwnSpeechPersisted {
    ByRoom(Vec<(Uuid, Vec<String>)>),
    Legacy(Vec<String>),
}

impl OwnSpeechPersisted {
    /// Total utterances across every room — what the restore probe reports.
    pub(super) fn len(&self) -> usize {
        match self {
            Self::ByRoom(by_room) => by_room.iter().map(|(_, u)| u.len()).sum(),
            Self::Legacy(flat) => flat.len(),
        }
    }
}

pub(super) fn volatile_path(persona_id: Uuid) -> std::io::Result<std::path::PathBuf> {
    let home = crate::paths::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot resolve persistent home directory",
        )
    })?;
    Ok(home
        .join(".continuum/personas")
        .join(persona_id.to_string())
        .join("volatile.json"))
}

/// What one checkpoint did: nothing (her state is where the last one left it), or wrote this many
/// bytes, and what her archive's tick did first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Checkpointed {
    Unchanged { archive: crate::cognition::working_memory::ArchiveTick },
    Written { bytes: usize, archive: crate::cognition::working_memory::ArchiveTick },
}

/// Everything a checkpoint writes, as the last write found it: her memory's generation and what
/// the writer stamps from outside it (`acting_card`, her own speech, the handoff without its clock).
/// Compared, never serialized: an unmoved mark is a skipped write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    /// Which working memory (a respawn builds a new one, whose generation starts over).
    instance: u64,
    generation: u64,
    acting_card: Option<Uuid>,
    own_speech: Vec<(Uuid, Vec<String>)>,
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
    let _checkpoint_lock = checkpoint_adoption::lock_checkpoint(&path, true)?;
    let archive = wm.tick_archive()?;
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
    let own_speech = super::super::deliberation_budget::own_speech_by_room(crate::identity::PeerId::from_uuid(persona_id));
    let mark = Mark {
        instance: wm.instance_id(),
        generation,
        acting_card,
        own_speech: own_speech.clone(),
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
    let persisted = PersistedVolatile { wm: snapshot, own_speech: OwnSpeechPersisted::ByRoom(own_speech) };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    use std::io::Write;
    let bytes = serde_json::to_vec(&persisted)?; // file on disk: her checkpoint, read back at her next spawn
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, &path)?;
    checkpoint_adoption::sync_parent(&path)?;
    last_marks().lock().insert(persona_id, mark);
    Ok(Checkpointed::Written { bytes: bytes.len(), archive })
}

/// Load the previous life's volatile tier under the same owner lock as saves
/// and adoption. Only an absent checkpoint permits a fresh resident; inability
/// to read or decode an existing one must reach the registration caller.
/// The spill handles a persona's PERSISTED working memory still points at: its checkpoint's
/// results ring AND its collapsed pointers (each pointer is how she reopens a result that left
/// the ring, so it is a reference exactly as a ring entry is). Disk-pressure eviction protects these as well as the live minds' handles, so
/// a saved result survives the restart a deploy performs, a mind that has not restored yet,
/// and a persona that is offline (Astra on card 9197cd01). Cached per persona by the
/// checkpoint's modification time, so an eviction pass over many files reads each checkpoint
/// once and a new checkpoint is always seen. An unreadable checkpoint is an `Err`: the caller
/// fails closed and evicts nothing of that persona's.
pub(crate) fn persisted_result_handles(
    persona_id: Uuid,
) -> std::io::Result<std::sync::Arc<std::collections::HashSet<String>>> {
    result_handles_at(&volatile_path(persona_id)?)
}

/// [`persisted_result_handles`] for the checkpoint at `path` (the filesystem seam).
pub(super) fn result_handles_at(
    path: &std::path::Path,
) -> std::io::Result<std::sync::Arc<std::collections::HashSet<String>>> {
    type Cached = (std::time::SystemTime, std::sync::Arc<std::collections::HashSet<String>>);
    static CACHE: std::sync::OnceLock<
        parking_lot::Mutex<std::collections::HashMap<std::path::PathBuf, Cached>>,
    > = std::sync::OnceLock::new();
    let modified = match std::fs::metadata(path) {
        Ok(meta) => meta.modified()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Default::default());
        }
        Err(error) => return Err(error),
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, handles)) = cache.lock().get(path) {
        if *at == modified {
            return Ok(std::sync::Arc::clone(handles));
        }
    }
    let handles: std::sync::Arc<std::collections::HashSet<String>> = std::sync::Arc::new(
        load_volatile_at(path)?
            .map(|persisted| persisted.wm.reachable_handles())
            .unwrap_or_default(),
    );
    cache.lock().insert(path.to_path_buf(), (modified, std::sync::Arc::clone(&handles)));
    Ok(handles)
}

pub(super) fn load_volatile(persona_id: Uuid) -> std::io::Result<Option<PersistedVolatile>> {
    load_volatile_at(&volatile_path(persona_id)?)
}

pub(super) fn load_volatile_at(path: &std::path::Path) -> std::io::Result<Option<PersistedVolatile>> {
    let _lock = checkpoint_adoption::lock_checkpoint(path, true)?;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}
