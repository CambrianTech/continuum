//! `TrainingJobBoard` — the in-flight training-job registry that bridges L2
//! (a job was dispatched) and L3 (the job completed → eval → page-in).
//!
//! ## Why this exists (the missing retention)
//!
//! The training-job model is POLL-based: `genome/job-create` returns a
//! [`JobHandle`]; status is observed by polling `genome/job-status` →
//! [`super::FineTuningAdapter::poll`]. There is NO completion event crossing the
//! command boundary — the per-job [`super::JobController`] lives privately inside
//! each adapter, and cloud adapters (OpenAI, Mistral) are inherently poll-only. So
//! the ONE uniform way to observe completion across every provider is to poll the
//! handle on an interval.
//!
//! But nothing retained the handles the trigger creates: `dispatch_job_create`
//! returned `(JobHandle, provider)` to its caller and the handle was dropped on the
//! floor. This board IS that retention. The L2 trigger registers each
//! freshly-dispatched job here ([`TrainingJobBoard::register`]); the L3 sentinel
//! (`modules::training_completion_sentinel`) polls each one and, on
//! terminal completion, claims it ([`TrainingJobBoard::claim`]) and runs the
//! eval→page-in chain. Polling vs a `watch` channel is the honest fit: there is no
//! in-process completion event to subscribe to, and a cloud provider can only be
//! asked, never told.
//!
//! ## The ledger (why the board is no longer memory-only)
//!
//! Glass-boxed 2026-07-11: 41 training jobs were submitted in one day and NOT ONE
//! completion or failure was ever recorded — every core reboot killed the board,
//! the in-process watchers, AND the spawned `mlx_lm.lora` children together, and
//! nothing on disk remembered the jobs had existed. Silent death is the same
//! instrument-lie class as the eval hang the shell-deadline fix closed: the
//! operator reads "no failures" as "healthy" while the flywheel's tail is severed.
//!
//! So the board journals to an append-only JSONL ledger
//! (`~/.continuum/genome/jobs-ledger.jsonl`): one `registered` line per dispatch,
//! one `terminal` line per claim. On the first touch of
//! [`TrainingJobBoard::global`] each boot, [`TrainingJobBoard::reconcile_orphans`]
//! replays the ledger; any job registered but never terminal died with a previous
//! core — it is journaled `terminal/killed-by-reboot` and logged LOUD. No silent
//! resurrection, no silent loss: the death notice is the contract (resume is a
//! later, separate concern). A failed ledger append never blocks training — it is
//! itself logged loud and the in-memory board proceeds (observability must not
//! gate the work it observes). Same pattern family as the llama-server pidfile
//! reclaim: a previous life's leftovers are accounted for at boot, never ignored.
//!
//! ## Why process-global
//!
//! The writer (the `modules::training_trigger` module) and the reader (the
//! L3 sentinel module) are independently-constructed `ServiceModule`s with no shared
//! owner — exactly the shape the persona-workspace registry's global and the
//! channel-digest buffer already use. One board per core, reached through
//! [`TrainingJobBoard::global`].
//!
//! ## Claim-once semantics (no double-processing)
//!
//! The sentinel's tick removes a job from the board the moment it observes a
//! terminal status ([`claim`]), BEFORE spawning the (minutes-long) eval chain off
//! the tick. A subsequent tick can never re-handle the same completed job because
//! it is already gone. A terminal event leaves the board exactly once.

use continuum_base::time::EpochMs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use dashmap::DashMap;
use uuid::Uuid;

use super::types::{JobHandle, TrainingStatus};

/// One in-flight training job, plus the context the L3 sentinel needs to run the
/// eval→page-in chain when it completes WITHOUT re-deriving any of it. Cloned out of
/// the board on snapshot; the `handle.local_id` is the board key.
/// A job a previous core registered and never brought to a terminal state — what the
/// boot replay found in the ledger. Journaled `killed-by-reboot` (that process IS dead)
/// and handed to the trigger, which resumes it from its job directory when the input
/// survived (card 244757bc: three of these on BigMama, none ever resumed, while the
/// consumer deploys six times a day).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanedJob {
    pub local_id: Uuid,
    pub trigger_dispatch_id: Option<Uuid>,
    pub provider_id: String,
    pub persona_id: Uuid,
    pub persona_name: String,
    pub base_model: String,
    pub trait_kind: String,
    pub eval_set: Option<String>,
}

/// A job that ENDED, completed or failed, as the trigger needs it to find the job's directory
/// and return the examples its partition parked (card 8a2334d9). A failed job is listed too,
/// because one whose examples all parked never ran and has no other route back (card 565b3b64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndedJob {
    pub local_id: Uuid,
    pub persona_name: String,
    pub trait_kind: String,
    /// It completed training: its sent examples trained. False: it failed, and whether any of
    /// its examples ran is the trigger's to read from its partition.
    pub trained: bool,
}

/// How many times one job's lineage may be resumed before the board stops trying:
/// a job that dies at every boot is not an interrupted job, it is a broken one, and
/// the third death is the receipt that says so.
pub const MAX_RESUMES: u32 = 3;

#[derive(Debug, Clone)]
pub struct WatchedJob {
    /// Exact durable trigger intent which produced this handle, when present.
    pub trigger_dispatch_id: Option<Uuid>,
    /// The handle to poll — `handle.provider_id` looks the adapter back up in the
    /// [`super::FineTuningRegistry`], `handle.local_id` is the stable board key.
    pub handle: JobHandle,
    /// The persona whose genome this layer trains — the eval subject and the
    /// page-in target (its live `WorkspaceCycle`).
    pub persona_id: Uuid,
    /// The persona's display name — log/observability context only.
    pub persona_name: String,
    /// The base model the layer was forged against: the continuum id the serving daemon
    /// filters the adapter manifest by, so a trained gene is registered (and trialled)
    /// under the base that serves it. In-engine training trains on exactly this served id.
    pub base_model: String,
    /// The domain bucket (`DomainClassifier` output) this layer specializes — used
    /// as the gene NAME on page-in and the eval gene label.
    pub trait_kind: String,
    /// The `cognition/eval` `eval_set` JSONL path, carried verbatim from the
    /// [`super::types::TrainingJobRequest`] for a manual spot-check. The sentinel does
    /// not read it: a gene's verdict is its in-room trial, never an A/B eval.
    pub eval_set: Option<String>,
    /// The gene's embedding-space identity, MINTED at `genome/job-create` — the
    /// one moment the training corpus is in hand (before this field the chain
    /// broke there: an adopted gene's corpus was unreferenceable, audited
    /// 2026-08-22). Carried here so the sentinel can stamp it into the
    /// signature sidecar at adoption. `None` = mint failed or predates the
    /// field; the gene still adopts, it just routes by the fallback path.
    pub signature: Option<crate::genome::signature::GeneSignature>,
    /// What the decision said about this job's competence at job-create (reuse, fork or
    /// mint, with the numbers), so the gene's lineage can say which branch bore it.
    /// `None` = no signature was minted, so nothing could be compared.
    pub decision: Option<crate::genome::competence::Decision>,
}

/// Process-global registry of in-flight training jobs. DashMap-backed so the L2
/// writer and the L3 reader touch it lock-free from different tokio tasks. The
/// `Default` board has NO ledger (unit tests of the in-memory contract);
/// [`TrainingJobBoard::global`] journals to the real ledger and reconciles
/// orphans at boot — see the module doc.
#[derive(Debug, Default)]
pub struct TrainingJobBoard {
    /// The exclusive lock on `<ledger>.owner`, held for this board's lifetime when this board
    /// OWNS the ledger. Only one board on a node can hold it, so the holder knows every open
    /// entry it did not register belongs to a board that has died (the OS releases a lock with
    /// its holder: pid reuse cannot fool it). `None`: another board owns the ledger (a test
    /// process, a second binary), and this one never reconciles it (Cormac's design).
    owner: Option<std::fs::File>,
    jobs: DashMap<Uuid, WatchedJob>,
    /// Completed jobs whose gene is being adopted (convert, register, trial, stamp):
    /// claimed off `jobs` so no tick re-handles them, kept HERE so a full bucket still
    /// sees the competence as pending and joins it. Without this the minutes of an MLX
    /// convert were a window in which a second gene minted for the same competence
    /// (Cormac on #4794, point 6). In memory only: a core that dies mid-adoption loses
    /// the adoption, as before, and the ledger's terminal row already stands.
    adopting: DashMap<Uuid, WatchedJob>,
    /// Append-only journal path; `None` disables journaling.
    ledger: Option<PathBuf>,
    /// Corrupt offsets already reported by this board; rows stay untouched on disk.
    quarantined: DashMap<u64, ()>,
    #[cfg(any(test, feature = "test-fixtures"))]
    _test_directory: Option<std::sync::Arc<tempfile::TempDir>>,
    /// Orphans the boot replay found, until the trigger takes them (`take_orphans`).
    orphans: std::sync::Mutex<Vec<OrphanedJob>>,
    /// Trigger-dispatched jobs that ended FAILED at runtime, until the trigger takes them
    /// (`take_failed`) and decides whether their examples go back to her bucket (card
    /// 2dfce676). The sentinel only polls and claims; the decision is the trigger's.
    failed: std::sync::Mutex<Vec<WatchedJob>>,
    /// Jobs that ENDED, until the trigger takes them (`take_ended`) and returns the examples each
    /// one parked (cards 8a2334d9, 565b3b64): claimed Completed or Failed at runtime, and at boot
    /// every job the ledger records as ended, since a restart loses this list.
    ended: std::sync::Mutex<Vec<EndedJob>>,
}

/// A bounded evidence read. Neither absence, a partial scan, nor an I/O error
/// establishes that an uncertain dispatch did not create a provider job.
pub enum JournalLookup<T> {
    Observed(T),
    NotObserved,
    Incomplete {
        next_offset: u64,
    },
    /// This page contained unreadable evidence. A positive match remains usable;
    /// no match never establishes absence. Counts are per page, not cumulative.
    Corrupt {
        observed: Option<T>,
        malformed: u32,
        next_offset: u64,
    },
}

pub type DispatchLookup = JournalLookup<JobHandle>;

static GLOBAL: OnceLock<TrainingJobBoard> = OnceLock::new();
const DISPATCH_JOURNAL_PAGE_BYTES: u64 = 64 * 1024;

/// `<continuum home>/genome/jobs-ledger.jsonl` — sibling of the job artifact dirs
/// ([`super::job_dir::genome_root`]). The node's job ledger. A test process never opens the
/// home ledger: a `cargo test` that reached it replayed the LIVE core's open jobs as a dead
/// core's and journaled them killed-by-reboot (2026-10-05, the 5090: 18 of Kimi's jobs, at
/// the two instants a test ran). `paths::continuum_home` gives every test build (this crate's
/// AND core's, mind's…) a per-process home, where a `#[cfg(test)]` copy here protected only
/// this crate's own tests (card d11d9827). No home means no ledger, said, never one in the cwd.
fn default_ledger_path() -> Option<PathBuf> {
    super::job_dir::genome_root().map(|genome| genome.join("jobs-ledger.jsonl"))
}

/// Take the ledger's owner lock if no live board holds it. `None` when another board owns
/// it (a probe says so) or the lock file cannot be opened (logged: this board then never
/// reconciles, which leaves jobs open rather than killing a live core's).
fn take_ledger_ownership(ledger: &Path) -> Option<std::fs::File> {
    let mut name = ledger.file_name()?.to_os_string();
    name.push(".owner");
    let lock_path = ledger.with_file_name(name);
    if let Some(dir) = lock_path.parent() {
        if let Err(error) = std::fs::create_dir_all(dir) {
            tracing::error!(%error, dir = %dir.display(), "the job ledger's directory cannot be created; this board will not reconcile");
            return None;
        }
    }
    let file = match std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path) {
        Ok(file) => file,
        Err(error) => {
            tracing::error!(%error, lock = %lock_path.display(), "the job ledger's owner lock cannot be opened; this board will not reconcile");
            return None;
        }
    };
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            continuum_base::probe!(
                class = "training.job.ledger_owned_elsewhere",
                lock = %lock_path.display(),
                "another live board owns this job ledger; this one reads it and never reconciles it"
            );
            None
        }
        Err(std::fs::TryLockError::Error(error)) => {
            tracing::error!(%error, lock = %lock_path.display(), "the job ledger's owner lock cannot be taken; this board will not reconcile");
            None
        }
    }
}

/// A registration row as the job it describes: THE one reading of a registration
/// (orphan reconciliation, `registration`). `None` when its ids do not parse.
fn orphan_of(id: &str, reg: &serde_json::Value) -> Option<OrphanedJob> {
    let field = |k: &str| reg.get(k).and_then(|v| v.as_str()).map(str::to_string);
    let local_id = Uuid::parse_str(id).ok()?;
    let persona_id = field("persona_id").and_then(|p| Uuid::parse_str(&p).ok())?;
    Some(OrphanedJob {
        local_id,
        trigger_dispatch_id: field("trigger_dispatch_id").and_then(|d| Uuid::parse_str(&d).ok()),
        provider_id: field("provider_id").unwrap_or_default(), // unwrap_or_default: an unknown provider lets job-create choose
        persona_id,
        persona_name: field("persona_name").unwrap_or_default(), // unwrap_or_default: the job dir lookup then fails loud as not-resumable
        base_model: field("base_model").unwrap_or_default(), // unwrap_or_default: same — the request.json carries the truth
        trait_kind: field("trait_kind").unwrap_or_default(), // unwrap_or_default: same
        eval_set: field("eval_set"),
    })
}

/// The exact bytes one ledger row occupies: compact JSON and its newline, built
/// before any I/O so the append is a single write. PURE, so the two-writer test
/// below can pin that a row never spans a syscall boundary by construction.
fn ledger_line(line: &serde_json::Value) -> String {
    let mut s = line.to_string();
    s.push('\n');
    s
}

impl TrainingJobBoard {
    /// Command-facing read delegates filesystem work to the existing board owner (the
    /// caller names it: [`Self::global`] in production, its own board in a test).
    /// One page per request; callers receive continuation rather than a scan loop.
    pub async fn terminal_history<B: std::ops::Deref<Target = Self> + Send + 'static>(
        board: B,
        handle: JobHandle,
        offset: u64,
    ) -> Result<JournalLookup<TrainingStatus>, String> {
        tokio::task::spawn_blocking(move || board.lookup_terminal(&handle, offset))
        .await
        .map_err(|e| format!("training history worker failed: {e}"))?
    }
    /// Normal path reads the existing live board. Recovery scans at most 64 KiB
    /// of its existing journal per call, with an explicit continuation offset.
    /// Caller performs this filesystem boundary on a blocking worker.
    pub fn lookup_trigger_dispatch(
        &self,
        dispatch_id: Uuid,
        offset: u64,
    ) -> Result<DispatchLookup, String> {
        if let Some(handle) = self.jobs.iter().find_map(|job| {
            (job.trigger_dispatch_id == Some(dispatch_id)).then(|| job.handle.clone())
        }) {
            return Ok(DispatchLookup::Observed(handle));
        }
        #[derive(serde::Deserialize)]
        struct Entry {
            event: String,
            #[serde(default)]
            trigger_dispatch_id: Option<Uuid>,
            #[serde(default)]
            local_id: Option<Uuid>,
            #[serde(default)]
            provider_id: Option<String>,
            #[serde(default)]
            provider_job_id: Option<String>,
        }
        self.scan_journal_page(offset, |line| {
            let entry: Entry = serde_json::from_slice(line)
                .map_err(|e| format!("malformed training dispatch journal entry: {e}"))?;
            if entry.event == "registered" && entry.trigger_dispatch_id == Some(dispatch_id) {
                return Ok(Some(JobHandle {
                    local_id: entry
                        .local_id
                        .ok_or("training dispatch evidence omitted local_id")?,
                    provider_id: entry
                        .provider_id
                        .ok_or("training dispatch evidence omitted provider_id")?,
                    provider_job_id: entry
                        .provider_job_id
                        .ok_or("training dispatch evidence omitted provider_job_id")?,
                }));
            }
            Ok(None)
        })
    }

    /// Read only a terminal receipt whose complete provider handle matches.
    /// Old records without typed status never imply successful training. An
    /// explicit legacy reboot interruption is recoverable only when its full
    /// registration handle was verified in the same bounded page.
    pub(crate) fn lookup_terminal(
        &self,
        handle: &JobHandle,
        offset: u64,
    ) -> Result<JournalLookup<TrainingStatus>, String> {
        #[derive(serde::Deserialize)]
        struct TerminalEntry {
            event: String,
            local_id: Option<Uuid>,
            handle: Option<JobHandle>,
            status: Option<TrainingStatus>,
            provider_id: Option<String>,
            provider_job_id: Option<String>,
            reason: Option<String>,
        }
        let mut registration_matches = false;
        self.scan_journal_page(offset, |line| {
            let entry: TerminalEntry = serde_json::from_slice(line)
                .map_err(|e| format!("malformed training journal entry: {e}"))?;
            if entry.event == "registered" && entry.local_id == Some(handle.local_id) {
                registration_matches = entry.provider_id.as_deref() == Some(handle.provider_id.as_str())
                    && entry.provider_job_id.as_deref() == Some(handle.provider_job_id.as_str());
                return Ok(None);
            }
            if entry.event != "terminal" || entry.local_id != Some(handle.local_id) {
                return Ok(None);
            }
            let Some(recorded) = entry.handle else {
                if entry.status.is_none()
                    && registration_matches
                    && entry.reason.as_deref() == Some("killed-by-reboot")
                {
                    return Ok(Some(TrainingStatus::Failed {
                        error: "killed-by-reboot (legacy journal receipt)".into(),
                    }));
                }
                return Ok(None);
            };
            if recorded.local_id != handle.local_id
                || recorded.provider_id != handle.provider_id
                || recorded.provider_job_id != handle.provider_job_id
            {
                return Ok(None);
            }
            match entry.status {
                Some(
                    status @ (TrainingStatus::Completed { .. }
                    | TrainingStatus::Failed { .. }
                    | TrainingStatus::Cancelled),
                ) => Ok(Some(status)),
                Some(_) => Err("terminal training receipt carries a nonterminal status".into()),
                None => Ok(None),
            }
        })
    }

    /// One bounded filesystem read shared by dispatch recovery and status history.
    /// Run on a blocking worker; a partial page is never evidence of absence.
    fn scan_journal_page<T>(
        &self,
        offset: u64,
        mut select: impl FnMut(&[u8]) -> Result<Option<T>, String>,
    ) -> Result<JournalLookup<T>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let Some(path) = &self.ledger else {
            return Ok(JournalLookup::NotObserved);
        };
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(JournalLookup::NotObserved)
            }
            Err(e) => return Err(format!("read training dispatch journal: {e}")),
        };
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        if offset > len {
            return Err("training dispatch journal shrank during recovery".into());
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::with_capacity(DISPATCH_JOURNAL_PAGE_BYTES as usize);
        file.take(DISPATCH_JOURNAL_PAGE_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Ok(JournalLookup::NotObserved);
        }
        let Some(last_newline) = bytes.iter().rposition(|b| *b == b'\n') else {
            if bytes.len() as u64 == DISPATCH_JOURNAL_PAGE_BYTES {
                return Err(
                    "training dispatch journal record exceeds the bounded recovery page".into(),
                );
            }
            return Ok(JournalLookup::Incomplete {
                next_offset: offset,
            });
        };
        let mut observed = None;
        let mut malformed = 0;
        let mut line_offset = offset;
        for record in bytes[..=last_newline].split_inclusive(|b| *b == b'\n') {
            let line = &record[..record.len() - 1];
            if !line.is_empty() {
                match select(line) {
                    Ok(value) => {
                        if observed.is_none() {
                            observed = value;
                        }
                    }
                    Err(error) => {
                        malformed += 1;
                        if self.quarantined.insert(line_offset, ()).is_none() {
                            continuum_base::probe!(class = "training.journal.corrupt",
                                offset = line_offset, error = %error,
                                "unreadable journal row retained; recovery advances with explicit uncertainty");
                        }
                    }
                }
            }
            line_offset += record.len() as u64;
        }
        let next_offset = offset + last_newline as u64 + 1;
        if malformed > 0 {
            return Ok(JournalLookup::Corrupt {
                observed,
                malformed,
                next_offset,
            });
        }
        if let Some(value) = observed {
            return Ok(JournalLookup::Observed(value));
        }
        if next_offset < len {
            Ok(JournalLookup::Incomplete { next_offset })
        } else {
            Ok(JournalLookup::NotObserved)
        }
    }

    /// The one board for this core. Lazily built on first touch; the first build
    /// replays the ledger — jobs a previous core left in flight are journaled dead
    /// (`killed-by-reboot`) and reported loud, so a severed flywheel tail is
    /// visible instead of silent.
    pub fn global() -> &'static TrainingJobBoard {
        GLOBAL.get_or_init(|| {
            let ledger = default_ledger_path();
            if ledger.is_none() {
                tracing::error!(
                    "no home directory: the training job ledger is off, so a job this core starts \
                     is not accounted for across a restart"
                );
            }
            let board = TrainingJobBoard::with_ledger(ledger.clone());
            let orphaned = board.reconcile_orphans().len();
            board.reconcile_ended();
            if let (true, Some(ledger)) = (orphaned > 0, ledger) {
                tracing::warn!(
                    orphaned,
                    ledger = %ledger.display(),
                    "training jobs from a previous core died with it (killed-by-reboot) — \
                     spawned trainers and their watchers do not survive a restart; the L3 \
                     eval→page-in chain never ran for them"
                );
            }
            board
        })
    }

    /// A board journaling to `ledger` (`None` disables — the `Default` board).
    /// Tests point this at a temp file.
    pub fn with_ledger(ledger: Option<PathBuf>) -> Self {
        TrainingJobBoard {
            owner: ledger.as_deref().and_then(take_ledger_ownership),
            jobs: DashMap::new(),
            adopting: DashMap::new(),
            quarantined: DashMap::new(),
            orphans: std::sync::Mutex::new(Vec::new()),
            failed: std::sync::Mutex::new(Vec::new()),
            ended: std::sync::Mutex::new(Vec::new()),
            ledger,
            #[cfg(any(test, feature = "test-fixtures"))]
            _test_directory: None,
        }
    }

    /// Keep the fixture journal alive for every owner sharing this board.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn with_test_storage(directory: std::sync::Arc<tempfile::TempDir>) -> Self {
        let mut board = Self::with_ledger(Some(directory.path().join("jobs-ledger.jsonl")));
        board._test_directory = Some(directory);
        board
    }

    /// Append one JSON line to the ledger. Never blocks or panics the caller: an
    /// append failure is loud in the log and the in-memory board proceeds —
    /// observability must not gate the work it observes.
    fn journal(&self, line: &serde_json::Value) {
        let Some(path) = &self.ledger else { return };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            // ONE LINE, ONE write(2). `writeln!(f, "{line}")` on a raw `File` let
            // `Value`'s Display emit the JSON as dozens of `write_str` fragments, each
            // its own syscall; O_APPEND makes each SYSCALL land atomically at the end,
            // not each LINE, so two tasks journaling at once zipped their records
            // together byte-wise — 136 of 701 lines on the M5 unreadable (card
            // faed9279), and every later reader of that ledger failed on them forever.
            // Serialize first, then a single `write_all` of the whole line.
            f.write_all(ledger_line(line).as_bytes())
        };
        if let Err(e) = write() {
            tracing::warn!(error = %e, ledger = %path.display(), "job-ledger append failed");
        }
    }

    /// Replay the ledger: every `registered` id with no matching `terminal` line
    /// died with a previous core (its trainer child and in-process watcher cannot
    /// survive a restart). Journal each as `terminal/killed-by-reboot`, log loud,
    /// and return the count. Idempotent — the terminal lines written here close
    /// the ids for the next replay. A missing ledger is a first boot, not an error.
    /// The ledger's latest registration of `local_id`, as the job it describes.
    pub fn registration(&self, local_id: Uuid) -> Option<OrphanedJob> {
        let id = local_id.to_string();
        self.ledger_rows()
            .filter(|row| row.get("event").and_then(|e| e.as_str()) == Some("registered"))
            .filter(|row| row.get("local_id").and_then(|i| i.as_str()) == Some(id.as_str()))
            .last()
            .and_then(|row| orphan_of(&id, &row))
    }

    /// The ledger's latest registration of `local_id`, as the handle its adapter owns:
    /// what an operator-settled dispatch adopts (`genome/training-trigger/resolve`).
    pub fn registered_handle(&self, local_id: Uuid) -> Option<JobHandle> {
        if let Some(handle) = self.jobs.iter().find_map(|job| (job.handle.local_id == local_id).then(|| job.handle.clone())) {
            return Some(handle);
        }
        let id = local_id.to_string();
        let row = self
            .ledger_rows()
            .filter(|row| row.get("event").and_then(|e| e.as_str()) == Some("registered"))
            .filter(|row| row.get("local_id").and_then(|i| i.as_str()) == Some(id.as_str()))
            .last()?;
        let field = |name: &str| row.get(name).and_then(|v| v.as_str()).map(str::to_owned);
        Some(JobHandle {
            provider_id: field("provider_id")?,
            provider_job_id: field("provider_job_id")?,
            local_id,
        })
    }

    /// Has `local_id` reached a terminal row on the ledger?
    pub fn is_terminal(&self, local_id: Uuid) -> bool {
        let id = local_id.to_string();
        self.ledger_rows().any(|row| {
            row.get("event").and_then(|e| e.as_str()) == Some("terminal")
                && row.get("local_id").and_then(|i| i.as_str()) == Some(id.as_str())
        })
    }

    /// Positive durable evidence that this job already produced a training outcome.
    /// Artifact relocation must not make it eligible for returning its input again.
    /// False is not proof that a failed job executed zero optimizer steps.
    pub fn completed_training(&self, local_id: Uuid) -> bool {
        let id = local_id.to_string();
        self.ledger_rows().any(|row| {
            row.get("event").and_then(|e| e.as_str()) == Some("terminal")
                && row.get("local_id").and_then(|i| i.as_str()) == Some(id.as_str())
                && row.get("status").is_some_and(|status| matches!(
                    serde_json::from_value::<TrainingStatus>(status.clone()),
                    Ok(TrainingStatus::Completed { .. })
                ))
        })
    }

    /// Every parseable row of the ledger, oldest first (none when there is no ledger).
    fn ledger_rows(&self) -> impl Iterator<Item = serde_json::Value> {
        let text = self.ledger.as_ref().and_then(|path| std::fs::read_to_string(path).ok()).unwrap_or_default(); // unwrap_or_default: no ledger file yet = no rows
        text.lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn reconcile_orphans(&self) -> Vec<OrphanedJob> {
        let Some(path) = &self.ledger else { return Vec::new() };
        // Only the ledger's owner reconciles it: an open entry is a dead board's ONLY to the
        // board that holds the lock no live board could hold beside it.
        if self.owner.is_none() {
            return Vec::new();
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let mut open: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let (Some(event), Some(id)) = (
                v.get("event").and_then(|e| e.as_str()),
                v.get("local_id").and_then(|i| i.as_str()).map(String::from),
            ) else {
                continue;
            };
            match event {
                "registered" => {
                    open.insert(id, v);
                }
                "terminal" => {
                    open.remove(&id);
                }
                _ => {}
            }
        }
        let mut orphans = Vec::new();
        for (id, reg) in &open {
            tracing::warn!(
                local_id = %id,
                persona = %reg.get("persona_name").and_then(|p| p.as_str()).unwrap_or("?"),
                trait_kind = %reg.get("trait_kind").and_then(|t| t.as_str()).unwrap_or("?"),
                "orphaned training job: registered by a previous core, never reached a \
                 terminal state — journaling killed-by-reboot; the trigger resumes it \
                 from its job directory if the input survived"
            );
            self.journal(&serde_json::json!({
                "event": "terminal",
                "reason": "killed-by-reboot",
                "local_id": id,
                "at_ms": EpochMs::now().millis(),
            }));
            // a registration without parseable ids cannot be resumed, only reported
            if let Some(orphan) = orphan_of(id, reg) {
                orphans.push(orphan);
            }
        }
        if let Ok(mut held) = self.orphans.lock() {
            held.extend(orphans.iter().cloned());
        }
        orphans
    }

    /// The orphans the boot replay found, once: the trigger takes them on its first
    /// tick with a live executor and resumes what it can.
    pub fn take_orphans(&self) -> Vec<OrphanedJob> {
        self.orphans.lock().map(|mut o| std::mem::take(&mut *o)).unwrap_or_default() // unwrap_or_default: a poisoned lock yields nothing to resume, never a panic at boot
    }

    /// The trigger-dispatched jobs that ended FAILED since the last take: the trigger's tick
    /// takes them and returns the ones the engine refused (card 2dfce676).
    pub fn take_failed(&self) -> Vec<WatchedJob> {
        self.failed.lock().map(|mut f| std::mem::take(&mut *f)).unwrap_or_default() // unwrap_or_default: a poisoned lock yields nothing to return, never a panic on the tick
    }

    /// The jobs that ended since the last take: the trigger's tick takes them and returns what
    /// each one parked (cards 8a2334d9, 565b3b64).
    pub fn take_ended(&self) -> Vec<EndedJob> {
        self.ended.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default() // unwrap_or_default: a poisoned lock yields nothing to return, never a panic on the tick
    }

    /// Put back ended jobs whose remainder could not, or may not yet, be returned this tick.
    pub fn requeue_ended(&self, jobs: Vec<EndedJob>) {
        if jobs.is_empty() {
            return;
        }
        if let Ok(mut held) = self.ended.lock() {
            held.extend(jobs);
        }
    }

    /// At boot, every job the ledger records as ended (completed, or failed), for the trigger to
    /// return what each one parked: the runtime list dies with the core, and a job that ended
    /// before this existed (Kimi's 55f2e6de completed; her 2de02f40 correction's all-parked job
    /// failed) has no other route. One pass over the ledger. A job whose remainder already went
    /// back is answered by its receipt, so listing it again costs one file check. Only the
    /// ledger's owner lists them, as with orphans.
    pub fn reconcile_ended(&self) {
        if self.owner.is_none() {
            return;
        }
        let mut registered: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        let mut ended: Vec<(String, bool)> = Vec::new();
        for row in self.ledger_rows() {
            let (Some(event), Some(id)) = (
                row.get("event").and_then(|e| e.as_str()).map(str::to_owned),
                row.get("local_id").and_then(|i| i.as_str()).map(str::to_owned),
            ) else {
                continue;
            };
            let status = row.get("status").and_then(|status| serde_json::from_value::<TrainingStatus>(status.clone()).ok());
            match (event.as_str(), status) {
                ("registered", _) => {
                    registered.insert(id, row);
                }
                ("terminal", Some(status @ (TrainingStatus::Completed { .. } | TrainingStatus::Failed { .. }))) => {
                    let trained = matches!(status, TrainingStatus::Completed { .. });
                    match ended.iter_mut().find(|(seen, _)| *seen == id) {
                        Some(entry) => entry.1 |= trained, // a completion outranks any failure row
                        None => ended.push((id, trained)),
                    }
                }
                _ => {}
            }
        }
        let jobs: Vec<EndedJob> = ended
            .iter()
            .filter_map(|(id, trained)| {
                let reg = registered.get(id)?;
                let field = |k: &str| reg.get(k).and_then(|v| v.as_str()).map(str::to_owned);
                Some(EndedJob {
                    local_id: Uuid::parse_str(id).ok()?,
                    persona_name: field("persona_name")?,
                    trait_kind: field("trait_kind")?,
                    trained: *trained,
                })
            })
            .collect();
        if let Ok(mut held) = self.ended.lock() {
            held.extend(jobs);
        }
    }

    /// The first job in `local_id`'s resume lineage, from the ledger's `resumed` rows.
    pub fn resume_origin(&self, local_id: Uuid) -> Uuid {
        let mut origin = local_id;
        for row in self.replay_rows() {
            if row.get("event").and_then(|e| e.as_str()) == Some("resumed")
                && row.get("new_local_id").and_then(|v| v.as_str()) == Some(&origin.to_string())
            {
                if let Some(o) = row.get("origin_local_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()) {
                    origin = o;
                }
            }
        }
        origin
    }

    /// How many resumes this lineage has already had.
    pub fn resume_attempts(&self, origin: Uuid) -> u32 {
        let origin = origin.to_string();
        self.replay_rows()
            .filter(|row| {
                row.get("event").and_then(|e| e.as_str()) == Some("resumed")
                    && row.get("origin_local_id").and_then(|v| v.as_str()) == Some(origin.as_str())
            })
            .count() as u32
    }

    /// Journal an orphan whose input went back to her bucket instead of into a new job:
    /// a trial was judging the competence, an existing gene carried it, or her surprise
    /// was below the floor. Its lineage ends here; the bucket's next fill is a fresh
    /// decision, not a resume.
    pub fn journal_returned<H: serde::Serialize>(&self, origin: Uuid, from: Uuid, held_by: &H, attempt: u32) {
        // Serialized here, once: a typed value in the journal, and a value that cannot
        // serialize (there is none today) is written as the failure it is, never as null.
        let held_by = serde_json::to_value(held_by)
            .unwrap_or_else(|e| serde_json::json!({ "unserializable": e.to_string() })); // unwrap_or_else: the row still says WHY the reason is missing
        self.journal(&serde_json::json!({
            "event": "returned",
            "origin_local_id": origin.to_string(),
            "from_local_id": from.to_string(),
            "held_by": held_by,
            "attempt": attempt,
            "at_ms": EpochMs::now().millis(),
        }));
    }

    /// Journal a resume: `from` died with a core, `to` carries its input on.
    pub fn journal_resumed(&self, origin: Uuid, from: Uuid, to: Uuid, attempt: u32) {
        self.journal(&serde_json::json!({
            "event": "resumed",
            "origin_local_id": origin.to_string(),
            "from_local_id": from.to_string(),
            "new_local_id": to.to_string(),
            "attempt": attempt,
            "at_ms": EpochMs::now().millis(),
        }));
    }

    fn replay_rows(&self) -> impl Iterator<Item = serde_json::Value> {
        let text = self
            .ledger
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default(); // unwrap_or_default: no ledger = no history, an empty lineage
        text.lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Register a freshly-dispatched job to watch. Called by the L2 trigger right
    /// after `genome/job-create` succeeds. Keyed by `handle.local_id` (the stable,
    /// substrate-side correlation id), so a re-register of the same job replaces
    /// rather than duplicates. Journals `registered` so a core death before the
    /// terminal line is accounted for at the next boot.
    pub fn register(&self, job: WatchedJob) {
        self.journal(&serde_json::json!({
            "event": "registered",
            "trigger_dispatch_id": job.trigger_dispatch_id,
            "local_id": job.handle.local_id.to_string(),
            "provider_id": job.handle.provider_id,
            "provider_job_id": job.handle.provider_job_id,
            "persona_id": job.persona_id.to_string(),
            "persona_name": job.persona_name,
            "base_model": job.base_model,
            "trait_kind": job.trait_kind,
            "eval_set": job.eval_set,
            "at_ms": EpochMs::now().millis(),
        }));
        self.jobs.insert(job.handle.local_id, job);
    }

    /// Snapshot every job currently being watched (cloned). The sentinel polls each
    /// of these per tick; iterating a snapshot (not the live map) keeps the tick
    /// free of held DashMap guards across `await` ([[fallbacks-are-illegal-fail-loud]]
    /// sibling: never hold a lock across await).
    pub fn snapshot(&self) -> Vec<WatchedJob> {
        self.jobs.iter().map(|e| e.value().clone()).collect()
    }

    /// Every job whose competence is still PENDING for a bucket: in flight, or completed
    /// and being adopted. The gates read this (a fill joins, never mints beside it); the
    /// sentinel polls [`snapshot`](Self::snapshot), the in-flight ones alone.
    pub fn pending(&self) -> Vec<WatchedJob> {
        self.jobs.iter().chain(self.adopting.iter()).map(|e| e.value().clone()).collect()
    }

    /// The adoption of a completed job ended (adopted, or refused): its competence is no
    /// longer pending. Idempotent.
    pub fn adoption_done(&self, local_id: Uuid) {
        self.adopting.remove(&local_id);
    }

    /// Atomically remove and return a job — the sentinel's "claim" the instant it
    /// observes a terminal status, BEFORE spawning the eval chain, so no later tick
    /// re-handles it. `None` if already claimed or not terminal. Journals the
    /// borrowed typed outcome and correlation so reboot cannot erase why it ended.
    /// Training completion is not proof the later evaluation/adoption succeeded.
    pub fn claim(&self, local_id: Uuid, status: &TrainingStatus) -> Option<WatchedJob> {
        if matches!(
            status,
            TrainingStatus::Queued
                | TrainingStatus::WaitingForCapacity { .. }
                | TrainingStatus::Running { .. }
        ) {
            return None;
        }
        let job = self.jobs.remove(&local_id).map(|(_, job)| job);
        if let Some(job) = &job {
            let trained = matches!(status, TrainingStatus::Completed { .. });
            if trained {
                self.adopting.insert(local_id, job.clone());
            }
            if trained || matches!(status, TrainingStatus::Failed { .. }) {
                if let Ok(mut ended) = self.ended.lock() {
                    ended.push(EndedJob {
                        local_id,
                        persona_name: job.persona_name.clone(),
                        trait_kind: job.trait_kind.clone(),
                        trained,
                    });
                }
            }
            if matches!(status, TrainingStatus::Failed { .. }) && job.trigger_dispatch_id.is_some() {
                if let Ok(mut failed) = self.failed.lock() {
                    failed.push(job.clone());
                }
            }
            self.journal(&serde_json::json!({
                "event": "terminal",
                "reason": "claimed",
                "status": status,
                "handle": &job.handle,
                "trigger_dispatch_id": job.trigger_dispatch_id,
                "local_id": local_id.to_string(),
                "at_ms": EpochMs::now().millis(),
            }));
        }
        job
    }

    /// How many jobs are in flight — observability + test assertions.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether any job is in flight.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

#[cfg(test)]
mod tests;
