//! A job's pause, as a fact that outlives the core.
//!
//! An in-engine training run lives in the ENGINE process, and the engine survives a core
//! relaunch and is adopted by the next core. The in-memory hold set
//! ([`super::engine_lora_adapter::hold_training`]) does not survive: a relaunch empties it, and
//! the adopted run's steering then resumes a run someone had deliberately paused. So a pause
//! asked through `genome/job-pause` is a FACT on disk, `state/training-holds.json`, keyed by
//! the JOB (Cormac on the hold design): the next job on the same lane can never inherit it
//! (Codex). Every tick of that job's run reads it beside the in-memory set, including the first
//! tick after a restart, before any steer. Nothing re-arms it at boot and nothing times it out
//! with a task: an expired entry is simply no longer live, and the job's end removes its
//! entries.
//!
//! Every persisted hold carries a TTL, bounded by [`MAX_HOLD_TTL_MS`]. A forgotten hold must not
//! pause her learning for days; that is the deploy-hold lesson (card ee76c0df), where a
//! TTL-less file paused the fleet's deploys for three days unseen.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The longest one persisted hold may stand; a longer pause is a renewal, which is a decision
/// someone makes again.
// derived-or-floor: a ceiling — one working day, well past a benchmark round or an
// operator's investigation, well short of the three days the TTL-less deploy hold ran.
pub const MAX_HOLD_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// One persisted hold on in-engine training.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedTrainingHold {
    pub id: Uuid,
    /// The job it pauses: its `JobHandle::local_id`.
    pub job: Uuid,
    /// Why the run is held; it names the hold in `training.run.paused` probes.
    pub reason: String,
    pub created_ms: u64,
    pub ttl_ms: u64,
}

impl PersistedTrainingHold {
    pub fn live_at(&self, now_ms: u64) -> bool {
        now_ms < self.created_ms.saturating_add(self.ttl_ms)
    }
}

/// Wall-clock unix ms: holds are created and expire in wall time, which a core restart keeps.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // unwrap_or: a clock before 1970 reads every hold as live, never as expired early
}

/// The store's file under a continuum home.
pub fn store_path(home: &Path) -> PathBuf {
    home.join("state").join("training-holds.json")
}

/// Serializes read-modify-write on the file within this process: two verbs racing must not
/// lose one another's hold.
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn parse(bytes: &[u8]) -> Vec<PersistedTrainingHold> {
    serde_json::from_slice(bytes).unwrap_or_default() // unwrap_or_default: a corrupt file holds nothing; the verbs rewrite it whole
}

fn read(path: &Path) -> Vec<PersistedTrainingHold> {
    std::fs::read(path).map(|b| parse(&b)).unwrap_or_default() // unwrap_or_default: no file = no holds
}

fn write(path: &Path, holds: &[PersistedTrainingHold]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let body = serde_json::to_vec_pretty(holds).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The holds standing on `job` at `now_ms` (the run's tick; async so the tick never blocks).
pub async fn live_on(path: &Path, job: Uuid, now_ms: u64) -> Vec<PersistedTrainingHold> {
    tokio::fs::read(path)
        .await
        .map(|b| parse(&b))
        .unwrap_or_default() // unwrap_or_default: no file = no holds
        .into_iter()
        .filter(|h| h.job == job && h.live_at(now_ms))
        .collect()
}

/// Record a hold. A TTL of zero or past [`MAX_HOLD_TTL_MS`] is refused, never clamped: the
/// caller learns the bound instead of getting a hold shorter or longer than it asked for.
/// Expired entries are pruned on the same write.
pub fn add(path: &Path, job: Uuid, reason: &str, ttl_ms: u64, now_ms: u64) -> Result<PersistedTrainingHold, String> {
    if ttl_ms == 0 || ttl_ms > MAX_HOLD_TTL_MS {
        return Err(format!("ttl must be 1..={MAX_HOLD_TTL_MS} ms (one day); renew a longer hold"));
    }
    if reason.trim().is_empty() {
        return Err("a hold needs a reason: it names the pause in the run's probes".into());
    }
    let hold = PersistedTrainingHold { id: Uuid::new_v4(), job, reason: reason.to_string(), created_ms: now_ms, ttl_ms };
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let mut holds: Vec<_> = read(path).into_iter().filter(|h| h.live_at(now_ms)).collect();
    holds.push(hold.clone());
    write(path, &holds)?;
    Ok(hold)
}

/// Release every hold on `job` (a resume, or the job's end); the count released. Expired
/// entries are pruned on the same write.
pub fn release_job(path: &Path, job: Uuid, now_ms: u64) -> Result<usize, String> {
    let _guard = WRITE.lock().unwrap_or_else(|p| p.into_inner()); // a poisoned guard still serializes; the data is the file
    let before: Vec<_> = read(path).into_iter().filter(|h| h.live_at(now_ms)).collect();
    let after: Vec<_> = before.iter().filter(|h| h.job != job).cloned().collect();
    let released = before.len() - after.len();
    write(path, &after)?;
    Ok(released)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the store's whole contract in one pass. A hold is live until its TTL
    // passes, then gone with no task; it is its job's and never another's (the next job on the
    // lane must not inherit it, Codex); a release frees that job and no other; a zero or
    // over-long TTL is refused, never clamped.
    #[tokio::test]
    async fn a_pause_is_its_jobs_until_released_or_expired_and_never_longer_than_a_day() {
        let dir = tempfile::tempdir().expect("test: dir");
        let path = store_path(dir.path());
        let (job, other) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let long = add(&path, job, "benchmark round", 60_000, 1_000).expect("test: add");
        let short = add(&path, job, "operator", 10_000, 1_000).expect("test: add");
        let theirs = add(&path, other, "operator", 60_000, 1_000).expect("test: add");
        let ids = |v: Vec<PersistedTrainingHold>| v.into_iter().map(|h| h.id).collect::<Vec<_>>();
        assert_eq!(ids(live_on(&path, job, 5_000).await), vec![long.id, short.id]);
        assert_eq!(ids(live_on(&path, Uuid::from_u128(3), 5_000).await), Vec::<Uuid>::new(), "a new job inherits nothing");
        assert_eq!(ids(live_on(&path, job, 11_000).await), vec![long.id], "an expired hold is simply no longer live");
        assert_eq!(release_job(&path, job, 12_000).expect("test: release"), 1);
        assert!(live_on(&path, job, 12_000).await.is_empty());
        assert_eq!(ids(live_on(&path, other, 12_000).await), vec![theirs.id], "a release frees that job and no other");
        assert_eq!(release_job(&path, job, 12_000).expect("test: release"), 0, "a second release finds nothing");
        assert!(add(&path, job, "x", 0, 0).is_err());
        assert!(add(&path, job, "x", MAX_HOLD_TTL_MS + 1, 0).is_err(), "refused, never clamped");
    }
}
