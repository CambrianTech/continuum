//! The confirmed-save ledger of one page dir, and the page-format identity it is keyed by.
use super::*;

/// The confirmed-save set of one page dir, on disk, so it outlives the process
/// (a deploy). Before this, eligibility lived only in [`KvSlotPool::saved`], which
/// starts empty in every core: on the M5 2026-10-10 the first turns after the 18:49Z
/// deploy reused nothing (turn_cached 0 of 31,811 and 27,250) while their pages sat on
/// disk. The file holds exactly the in-memory set: a key enters only after the engine
/// confirms its save and leaves before any save overwrites it, so a timed-out or
/// interrupted save is never in it. It is read back only under the same page-format
/// identity it was written with ([`KvPageContract::fingerprint`]).
pub(crate) struct SavedLedger {
    pub(super) path: std::path::PathBuf,
    pub(super) fingerprint: String,
    /// What the fingerprint means ([`LEDGER_FORMAT`]); an adopter keeps the format of the
    /// file it adopted, so it never relabels another format's fingerprint as this one's.
    pub(super) format: u32,
    /// For a ledger written before [`LEDGER_FORMAT`]: the newest modification time of the
    /// files whose identity the page format rests on (model, adapters, draft). A legacy page
    /// written before it may hold another model's KV under the same name, so it is dropped.
    pub(super) identity_floor: Option<std::time::SystemTime>,
    /// The generation of the snapshot on disk; a write serializes on this and skips a
    /// snapshot older than it.
    pub(super) written: Mutex<u64>,
    /// The engine process these pages belong to, stamped by the spawn once the engine is
    /// ready ([`stamp_engine_incarnation`]) or carried over by an adoption.
    pub(super) incarnation: std::sync::OnceLock<(u32, u64)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SavedLedgerFile {
    /// 0 (absent) = written before the page-format identity: its fingerprint digested the
    /// exact window, slot count and engine file, so it changed on every relaunch.
    #[serde(default)]
    pub(super) format: u32,
    pub(super) fingerprint: String,
    /// The minds whose page (`a-<persona>.bin`) is confirmed saved. Forks are never here:
    /// they die with the core that ran them.
    #[serde(default)]
    pub(super) minds: Vec<Uuid>,
    /// A ledger written before one page per mind: (persona, room) per activity page. Read
    /// only to fold into [`Self::minds`] ([`fold_into_minds`]); never written again.
    #[serde(default, rename = "keys", skip_serializing)]
    pub(super) activities: Vec<(Uuid, Uuid)>,
    /// The engine process that wrote these pages: (pid, OS start time). A core that
    /// ADOPTS a running engine has no launch contract of its own; this is how it proves
    /// the ledger is that very process's, and so under its contract. Absent on a ledger
    /// written before the spawn stamped it: such a ledger is never adopted.
    #[serde(default)]
    pub(super) incarnation: Option<(u32, u64)>,
    /// Which mind's KV each physical slot of that process holds: (slot, persona). A core
    /// that adopts the same process resumes them in place instead of overwriting a
    /// resident's warm KV with an older page (or with another mind's prefill, unsaved).
    /// A pre-mind ledger's per-activity residents are not read: its pages restore instead.
    #[serde(default)]
    pub(super) mind_residents: Vec<(u32, Uuid)>,
}

/// What a ledger fingerprint digests: 2 = the page-format identity (model, adapters and
/// draft by revision, KV cache type, flash attention, engine build), never the window or
/// the slot count, which the engine checks itself on every restore.
pub(super) const LEDGER_FORMAT: u32 = 2;

/// The ledger's file name inside a page dir. Not an `a-*.bin` page, so the page
/// store's trim never counts or removes it.
pub(super) const SAVED_LEDGER_FILE: &str = "saved-ledger.json";

/// One numbered state of the pool's ledger, taken under the lock that changed it.
pub(super) struct LedgerSnapshot {
    pub(super) generation: u64,
    pub(super) minds: Vec<Uuid>,
    pub(super) residents: Vec<(u32, Uuid)>,
}

/// What a ledger restores: the restorable keys, and (for the same engine process only)
/// which slot each resident occupies.
#[derive(Default)]
pub(super) struct LoadedLedger {
    pub(super) keys: std::collections::HashSet<PageKey>,
    pub(super) residents: Vec<(u32, PageKey)>,
}

impl SavedLedger {
    pub(crate) fn in_dir(page_dir: &std::path::Path, fingerprint: String) -> Self {
        Self {
            path: page_dir.join(SAVED_LEDGER_FILE),
            fingerprint,
            format: LEDGER_FORMAT,
            identity_floor: None,
            written: Mutex::new(0),
            incarnation: std::sync::OnceLock::new(),
        }
    }

    /// Adopt a legacy (pre-[`LEDGER_FORMAT`]) ledger's pages written after `floor`.
    pub(crate) fn with_identity_floor(mut self, floor: Option<std::time::SystemTime>) -> Self {
        self.identity_floor = floor;
        self
    }

    /// The ledger a running engine left in `page_dir`, if that engine is the process
    /// `incarnation` names: the fingerprint it was written under becomes this ledger's
    /// own, because it is the same process under the same launch contract. Any other
    /// ledger (another process, an unstamped one, none) is not this engine's.
    pub(super) fn of_running_engine(page_dir: &std::path::Path, incarnation: (u32, u64)) -> Result<Self, AdoptRefusal> {
        let bytes = match std::fs::read(page_dir.join(SAVED_LEDGER_FILE)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AdoptRefusal::NoLedger),
            Err(_) => return Err(AdoptRefusal::Unreadable),
        };
        let file = serde_json::from_slice::<SavedLedgerFile>(&bytes).map_err(|_| AdoptRefusal::Unreadable)?;
        match file.incarnation {
            None => return Err(AdoptRefusal::Unstamped),
            Some(written_by) if written_by != incarnation => return Err(AdoptRefusal::AnotherProcess),
            Some(_) => {}
        }
        let mut ledger = Self::in_dir(page_dir, file.fingerprint);
        ledger.format = file.format;
        let _ = ledger.incarnation.set(incarnation);
        Ok(ledger)
    }

    /// Atomic replace (write a sibling, then `replace_file`) into a dir that must already
    /// exist: the lane creates its page dir; the ledger never creates one. Writes are
    /// serialized and a snapshot older than the one on disk is dropped, so the file is
    /// always the newest set even when writers race.
    pub(super) fn write(&self, snapshot: LedgerSnapshot) -> std::io::Result<()> {
        let mut written = self.written.lock();
        if snapshot.generation <= *written {
            return Ok(());
        }
        let file = SavedLedgerFile {
            format: self.format,
            fingerprint: self.fingerprint.clone(),
            minds: snapshot.minds,
            activities: Vec::new(),
            incarnation: self.incarnation.get().copied(),
            mind_residents: snapshot.residents,
        };
        let bytes = serde_json::to_vec(&file)?; // file on disk: the page dir's confirmed-save ledger, read back by `load`
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        continuum_host::durable::replace(&tmp, &self.path)?;
        *written = snapshot.generation;
        Ok(())
    }

    /// The keys this ledger may restore: written under the same fingerprint (or a legacy
    /// ledger's pages newer than every identity file), with their page file still present.
    /// Anything else is no eligibility, never a guess: a missing file, another format's
    /// ledger, a malformed one. Residents come back only for the same engine process.
    pub(super) fn load(&self) -> LoadedLedger {
        let Some(dir) = self.path.parent() else {
            return LoadedLedger::default();
        };
        let unread = |why: &str| {
            continuum_base::probe!(
                class = "inference.kv_page.ledger_unread",
                path = %self.path.display(),
                why = why,
                "no confirmed-save ledger restored: the pages in this dir re-prefill once"
            );
            LoadedLedger::default()
        };
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoadedLedger::default(),
            Err(_) => return unread("unreadable"),
        };
        let Ok(file) = serde_json::from_slice::<SavedLedgerFile>(&bytes) else {
            return unread("malformed");
        };
        let legacy = file.format < LEDGER_FORMAT && self.format == LEDGER_FORMAT;
        if legacy && self.identity_floor.is_none() {
            return unread("written before the page-format identity, and no identity files to date it against");
        }
        if !legacy && (file.format != self.format || file.fingerprint != self.fingerprint) {
            return unread("written under another page format");
        }
        let listed = file.minds.len() + file.activities.len();
        let minds: Vec<MindKey> = file.minds.iter().copied().filter_map(MindKey::new).collect();
        let activities: Vec<(MindKey, Uuid)> = file
            .activities
            .iter()
            .filter(|(_, room)| !room.is_nil())
            .filter_map(|(persona, room)| MindKey::new(*persona).map(|m| (m, *room)))
            .collect();
        if minds.len() + activities.len() < listed {
            // A key with a nil persona (or a nil room, pre-mind) names nothing: its page re-prefills once.
            continuum_base::probe!(
                class = "inference.kv_page.ledger_keys_dropped",
                path = %self.path.display(),
                dropped = (listed - minds.len() - activities.len()) as u64,
                "confirmed-save ledger entries without a mind were dropped: their pages re-prefill once"
            );
        }
        // A ledger written before the page-format identity is dated instead of compared: a
        // page written after every identity file last changed was computed by those files.
        // The engine still refuses, loudly, a page whose cache type, attention layout, layer
        // shapes, window fit or adapters differ.
        let floor = if legacy {
            self.identity_floor.unwrap_or(std::time::SystemTime::UNIX_EPOCH) // unwrap_or: a legacy ledger with no floor returned above
        } else {
            std::time::SystemTime::UNIX_EPOCH
        };
        let fold = fold_into_minds(dir, &minds, &activities, floor);
        if legacy {
            continuum_base::probe!(
                class = "inference.kv_page.ledger_migrated",
                path = %self.path.display(),
                adopted = fold.keys.len() as u64,
                "a ledger written before the page-format identity was adopted for the pages written after the model's files last changed; the rest re-prefill once"
            );
        }
        if !activities.is_empty() {
            continuum_base::probe!(
                class = "inference.kv_page.ledger_folded",
                path = %self.path.display(),
                activities = activities.len() as u64,
                minds = fold.keys.len() as u64,
                renamed = fold.renamed as u64,
                "a per-activity ledger was folded into one page per mind: each mind keeps its newest page; the other pages stay on disk, unlisted"
            );
        }
        // The residents were in THIS process's slots only if it is the process that wrote them.
        let same_process = self.incarnation.get().is_some() && self.incarnation.get().copied() == file.incarnation;
        let residents = if same_process && !legacy {
            file.mind_residents.into_iter().filter_map(|(slot, persona)| PageKey::mind(persona).map(|k| (slot, k))).collect()
        } else {
            Vec::new()
        };
        LoadedLedger { keys: fold.keys, residents }
    }
}

/// What [`fold_into_minds`] restored, and how many pages it moved to a mind's name.
pub(super) struct Fold {
    pub(super) keys: std::collections::HashSet<PageKey>,
    pub(super) renamed: usize,
}

/// ONE page per mind from a ledger's entries: each confirmed mind whose page is on disk,
/// and each pre-mind activity page folded into its mind. Of a mind's candidate pages (its
/// own page, if present, and each listed activity page) the NEWEST by modification time
/// wins; an activity page that wins is renamed, with its sidecars, to the mind's name. Every
/// other page file is left on disk, only unlisted (the page store's trim ages it out). A
/// page modified before `floor` is never a candidate.
///
/// Idempotent: the mind's own page is a candidate even when the ledger does not list it, so
/// a fold interrupted after its rename (before the folded ledger was written) finds the
/// renamed page newest on the next load and never moves an older page over it. A crash
/// between moving a page and its sidecars leaves that page without them (the engine then
/// restores it with no checkpoints); the migration runs once.
pub(super) fn fold_into_minds(
    dir: &std::path::Path,
    minds: &[MindKey],
    activities: &[(MindKey, Uuid)],
    floor: std::time::SystemTime,
) -> Fold {
    let written = |name: &str| -> Option<std::time::SystemTime> {
        let md = std::fs::metadata(dir.join(name)).ok()?;
        let at = md.modified().ok()?;
        (md.is_file() && at >= floor).then_some(at)
    };
    let mut keys = std::collections::HashSet::new();
    let mut renamed = 0;
    for mind in minds {
        let key = PageKey::Mind(*mind);
        if written(&page_filename(&key)).is_some() {
            keys.insert(key);
        }
    }
    let mut by_mind: std::collections::BTreeMap<MindKey, Vec<Uuid>> = std::collections::BTreeMap::new();
    for (mind, room) in activities {
        by_mind.entry(*mind).or_default().push(*room);
    }
    for (mind, rooms) in by_mind {
        let key = PageKey::Mind(mind);
        let target = page_filename(&key);
        let own = written(&target).map(|at| (at, None));
        let newest = rooms
            .into_iter()
            .filter_map(|room| {
                let name = activity_page_filename(mind.persona(), room);
                written(&name).map(|at| (at, Some(name)))
            })
            .chain(own)
            .max_by_key(|(at, _)| *at);
        match newest {
            None => {}
            Some((_, None)) => {
                keys.insert(key);
            }
            Some((_, Some(source))) => match move_page(dir, &source, &target) {
                Ok(()) => {
                    keys.insert(key);
                    renamed += 1;
                }
                Err(error) => continuum_base::probe!(
                    class = "inference.kv_page.fold_unmoved",
                    page = %key,
                    source = %source,
                    error = %error,
                    "a mind's newest per-activity page could not be moved to its mind page: it re-prefills once"
                ),
            },
        }
    }
    Fold { keys, renamed }
}

/// Move page `source` to `target` with its sidecars ([`PAGE_SIDECARS`]): the page first,
/// then each sidecar — moved when the source has it, else the target's stale one removed,
/// so the target never carries another page's checkpoints or adapter vector.
fn move_page(dir: &std::path::Path, source: &str, target: &str) -> std::io::Result<()> {
    std::fs::rename(dir.join(source), dir.join(target))?;
    for sidecar in PAGE_SIDECARS {
        let from = dir.join(format!("{source}{sidecar}"));
        let to = dir.join(format!("{target}{sidecar}"));
        if from.is_file() {
            std::fs::rename(from, to)?;
        } else if let Err(error) = std::fs::remove_file(to) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error);
            }
        }
    }
    Ok(())
}

impl KvPageContract {
    /// A stable digest of what makes a saved page's KV valid: the model, adapters and draft
    /// (by path, size and mtime), the KV cache type and flash attention. NOT the engine
    /// build: a page and its checkpoint sidecar each carry the engine's own state format
    /// (`LLAMA_STATE_SEQ_MAGIC`/`VERSION`, the fork's `CKPT` magic and version), which the
    /// engine checks and refuses loudly on a mismatch, so keying on the build only threw
    /// away every page at each engine upgrade (IntelMac, 2026-10-23 21:17Z: a convergence
    /// relaunch onto the installed engine left every resident cold, 30-34 min prefills).
    /// NOT the window or the slot count: the engine refuses a page that does not fit its
    /// slot (`Restored prompt does not fit`, `not enough cells`), loudly, and keying on
    /// them made every relaunch's few-hundred-token window jitter a new ledger (the M5,
    /// 2026-10-23 04:39Z: `written under another contract`, every resident cold). `None`
    /// when any part is unknown: what cannot be compared exactly is never persisted.
    pub(crate) fn fingerprint(&self) -> Option<String> {
        use sha2::Digest;
        self.page_dir.as_ref()?;
        let cache_type = self.cache_type.as_ref()?;
        let flash_attn = self.flash_attn?;
        let revisions = self.revisions.as_ref()?;
        let mut h = sha2::Sha256::new();
        let mut field = |label: &str, value: &[u8]| {
            h.update(label.as_bytes());
            h.update((value.len() as u64).to_le_bytes());
            h.update(value);
        };
        field("format", &LEDGER_FORMAT.to_le_bytes());
        field("model_id", self.model_id.as_bytes());
        field("model", self.model.as_os_str().as_encoded_bytes());
        for a in &self.adapters {
            field("adapter", a.as_bytes());
        }
        field("cache_type", cache_type.as_bytes());
        field("flash_attn", &[u8::from(flash_attn)]);
        for (path, size, modified) in revisions {
            field("revision", path.as_os_str().as_encoded_bytes());
            field("size", &size.to_le_bytes());
            let nanos = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).ok()?;
            field("modified", &nanos.to_le_bytes());
        }
        Some(format!("{:x}", h.finalize()))
    }

    /// The newest modification among the identity files (model, adapters, draft): the date
    /// a legacy ledger's pages must postdate.
    pub(crate) fn identity_floor(&self) -> Option<std::time::SystemTime> {
        self.revisions.as_ref()?.iter().map(|(_, _, modified)| *modified).max()
    }
}
