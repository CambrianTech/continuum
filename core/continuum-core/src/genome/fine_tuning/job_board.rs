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
//! ([`crate::modules::training_completion_sentinel`]) polls each one and, on
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
//! The writer (the [`crate::modules::training_trigger`] module) and the reader (the
//! L3 sentinel module) are independently-constructed `ServiceModule`s with no shared
//! owner — exactly the shape [`crate::cognition::persona_workspace::global`] and the
//! channel-digest buffer already use. One board per core, reached through
//! [`TrainingJobBoard::global`].
//!
//! ## Claim-once semantics (no double-processing)
//!
//! The sentinel's tick removes a job from the board the moment it observes a
//! terminal status ([`claim`]), BEFORE spawning the (minutes-long) eval chain off
//! the tick. A subsequent tick can never re-handle the same completed job because
//! it is already gone. A terminal event leaves the board exactly once.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use dashmap::DashMap;
use uuid::Uuid;

use super::types::JobHandle;

/// One in-flight training job, plus the context the L3 sentinel needs to run the
/// eval→page-in chain when it completes WITHOUT re-deriving any of it. Cloned out of
/// the board on snapshot; the `handle.local_id` is the board key.
#[derive(Debug, Clone)]
pub struct WatchedJob {
    /// The handle to poll — `handle.provider_id` looks the adapter back up in the
    /// [`super::FineTuningRegistry`], `handle.local_id` is the stable board key.
    pub handle: JobHandle,
    /// The persona whose genome this layer trains — the eval subject and the
    /// page-in target (its live `WorkspaceCycle`).
    pub persona_id: Uuid,
    /// The persona's display name — log/observability context only.
    pub persona_name: String,
    /// The base model the layer was forged against — log/observability context.
    pub base_model: String,
    /// The domain bucket (`DomainClassifier` output) this layer specializes — used
    /// as the gene NAME on page-in and the eval gene label.
    pub trait_kind: String,
    /// The gym that measures this trait — the `cognition/eval` `eval_set` JSONL path,
    /// carried verbatim from the [`super::types::TrainingJobRequest`]. The sentinel
    /// passes it to the A/B eval; `None` means the recipe declared no gym, so the
    /// gene is unmeasurable and the sentinel refuses to adopt it (never falls back to
    /// a default gym — [[fallbacks-are-illegal-fail-loud]]).
    pub eval_set: Option<String>,
}

/// Process-global registry of in-flight training jobs. DashMap-backed so the L2
/// writer and the L3 reader touch it lock-free from different tokio tasks. The
/// `Default` board has NO ledger (unit tests of the in-memory contract);
/// [`TrainingJobBoard::global`] journals to the real ledger and reconciles
/// orphans at boot — see the module doc.
#[derive(Debug, Default)]
pub struct TrainingJobBoard {
    jobs: DashMap<Uuid, WatchedJob>,
    /// Append-only journal path; `None` disables journaling.
    ledger: Option<PathBuf>,
}

static GLOBAL: OnceLock<TrainingJobBoard> = OnceLock::new();

/// `~/.continuum/genome/jobs-ledger.jsonl` — sibling of the job artifact dirs
/// (the MLX adapter's `job_dir_for` uses the same `~/.continuum/genome` root).
fn default_ledger_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".continuum/genome/jobs-ledger.jsonl")
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl TrainingJobBoard {
    /// The one board for this core. Lazily built on first touch; the first build
    /// replays the ledger — jobs a previous core left in flight are journaled dead
    /// (`killed-by-reboot`) and reported loud, so a severed flywheel tail is
    /// visible instead of silent.
    pub fn global() -> &'static TrainingJobBoard {
        GLOBAL.get_or_init(|| {
            let board = TrainingJobBoard::with_ledger(Some(default_ledger_path()));
            let orphaned = board.reconcile_orphans();
            if orphaned > 0 {
                tracing::warn!(
                    orphaned,
                    ledger = %default_ledger_path().display(),
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
            jobs: DashMap::new(),
            ledger,
        }
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
            writeln!(f, "{line}")
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
    pub fn reconcile_orphans(&self) -> usize {
        let Some(path) = &self.ledger else { return 0 };
        let Ok(text) = std::fs::read_to_string(path) else {
            return 0;
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
        for (id, reg) in &open {
            tracing::warn!(
                local_id = %id,
                persona = %reg.get("persona_name").and_then(|p| p.as_str()).unwrap_or("?"),
                trait_kind = %reg.get("trait_kind").and_then(|t| t.as_str()).unwrap_or("?"),
                "orphaned training job: registered by a previous core, never reached a \
                 terminal state — journaling killed-by-reboot"
            );
            self.journal(&serde_json::json!({
                "event": "terminal",
                "reason": "killed-by-reboot",
                "local_id": id,
                "at_ms": now_ms(),
            }));
        }
        open.len()
    }

    /// Register a freshly-dispatched job to watch. Called by the L2 trigger right
    /// after `genome/job-create` succeeds. Keyed by `handle.local_id` (the stable,
    /// substrate-side correlation id), so a re-register of the same job replaces
    /// rather than duplicates. Journals `registered` so a core death before the
    /// terminal line is accounted for at the next boot.
    pub fn register(&self, job: WatchedJob) {
        self.journal(&serde_json::json!({
            "event": "registered",
            "local_id": job.handle.local_id.to_string(),
            "provider_id": job.handle.provider_id,
            "provider_job_id": job.handle.provider_job_id,
            "persona_id": job.persona_id.to_string(),
            "persona_name": job.persona_name,
            "base_model": job.base_model,
            "trait_kind": job.trait_kind,
            "eval_set": job.eval_set,
            "at_ms": now_ms(),
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

    /// Atomically remove and return a job — the sentinel's "claim" the instant it
    /// observes a terminal status, BEFORE spawning the eval chain, so no later tick
    /// re-handles it. `None` if it was already claimed. Journals `terminal/claimed`
    /// so the ledger's replay sees this id as closed.
    pub fn claim(&self, local_id: Uuid) -> Option<WatchedJob> {
        let job = self.jobs.remove(&local_id).map(|(_, job)| job);
        if job.is_some() {
            self.journal(&serde_json::json!({
                "event": "terminal",
                "reason": "claimed",
                "local_id": local_id.to_string(),
                "at_ms": now_ms(),
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
mod tests {
    use super::*;

    fn watched(local_id: Uuid, provider: &str) -> WatchedJob {
        WatchedJob {
            handle: JobHandle {
                provider_id: provider.to_string(),
                provider_job_id: "job-x".to_string(),
                local_id,
            },
            persona_id: Uuid::new_v4(),
            persona_name: "asha".to_string(),
            base_model: "qwen3-coder".to_string(),
            trait_kind: "code".to_string(),
            eval_set: Some("docs/genome/coder-eval.jsonl".to_string()),
        }
    }

    // what this catches: the claim-once contract. A registered job is visible in the
    // snapshot, claim() returns it exactly once and removes it, and a second claim
    // yields None — the invariant that prevents the L3 sentinel from running the
    // eval chain twice for one completed job.
    #[test]
    fn claim_returns_a_job_exactly_once() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "local-candle"));

        assert_eq!(board.len(), 1, "registered job must be in flight");
        assert_eq!(board.snapshot().len(), 1, "snapshot sees the registered job");

        let claimed = board.claim(id).expect("first claim returns the job");
        assert_eq!(claimed.handle.local_id, id);
        assert!(board.is_empty(), "claim removes the job from the board");
        assert!(
            board.claim(id).is_none(),
            "a second claim of the same job must be None — no double-processing"
        );
    }

    // what this catches: re-register replaces (keyed by local_id), so a duplicate
    // dispatch of the same correlation id never inflates the in-flight count.
    #[test]
    fn register_is_keyed_by_local_id() {
        let board = TrainingJobBoard::default();
        let id = Uuid::new_v4();
        board.register(watched(id, "openai"));
        board.register(watched(id, "openai"));
        assert_eq!(board.len(), 1, "same local_id must not duplicate");
    }

    // what this catches: the severed-flywheel-tail bug (glass-boxed 2026-07-11 —
    // 41 submissions, zero recorded outcomes). A job registered by a "previous
    // core" (same ledger file, fresh board) but never claimed MUST be reconciled
    // as killed-by-reboot at the next boot — loud, journaled, and idempotent —
    // while a claimed job reconciles clean. // regression for task #137
    #[test]
    fn orphaned_jobs_reconcile_as_killed_by_reboot() {
        let dir = std::env::temp_dir().join(format!("job-ledger-test-{}", Uuid::new_v4()));
        let ledger = dir.join("jobs-ledger.jsonl");

        // "Previous core": registers two jobs, claims one, then dies (dropped).
        let previous = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        let done = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        previous.register(watched(done, "mlx"));
        previous.register(watched(orphan, "mlx"));
        previous.claim(done).expect("claim the finished job");
        drop(previous);

        // "Next boot": replay finds exactly the unclaimed job.
        let next = TrainingJobBoard::with_ledger(Some(ledger.clone()));
        assert_eq!(
            next.reconcile_orphans(),
            1,
            "exactly the never-terminal job is orphaned"
        );
        let text = std::fs::read_to_string(&ledger).expect("ledger exists");
        assert!(
            text.contains("killed-by-reboot") && text.contains(&orphan.to_string()),
            "the orphan's death is journaled: {text}"
        );
        // Idempotent: the terminal line just written closes the id.
        assert_eq!(
            next.reconcile_orphans(),
            0,
            "a second replay sees the orphan as closed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
