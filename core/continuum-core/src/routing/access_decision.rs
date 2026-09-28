//! THE ACCESS DECISION: what a citizen may do follows her COGNITIVE LEVEL, not the transport
//! she arrived on.
//!
//! Joel, 2026-09-28: the point of Continuum is a citizen team that replaces Claude or Codex,
//! and the citizens had been hand-crippled. An audit that day found twenty restrictions,
//! each added with a local reason and never revisited; the deepest was a TRANSPORT ceiling
//! (every airc caller capped at `Provisional`, whoever and however capable she was). This
//! module replaces transport with capability:
//! - a capable model (at or above [`AccessPolicy::full_access_min_rank`] on the serving
//!   capability scale, the Artificial Analysis index the planner already ranks by) gets the
//!   operator's working surface, `Trusted`: shell, push, database writes, agents, pipelines;
//! - a weak one gets the restricted surface, `Provisional`;
//! - an unknown level gets MORE, not less (`Trusted`): a missing measurement never cripples;
//! - a peer the operator explicitly `Blocked` stays blocked.
//!
//! Access is COMMAND-BASED like everything else: `access/get` reads the policy and
//! `access/set` changes it (the threshold, and per-citizen DECISIONS). A decision is how
//! earned access arrives: today the owner records it; later reputation and a community of
//! security and governance reviewers decide it through the same command and the same record,
//! so the decision function never changes shape. A decision wins over the default.

use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::grid::node::TrustLevel;

/// The default capability rank at which a citizen gets the full working surface. The scale is
/// the Artificial Analysis Intelligence Index the serving planner already ranks models by
/// (Qwen3.8-27B = 42, Qwen3.8-Flash-Next = 46; an unmeasured model's size proxy is capped at
/// 40). Changed with `access/set`, never here.
// derived-or-floor: a floor on the 0-255 capability scale, below every coder-tier model we
// serve (42, 46) and above the small CPU-tier models.
pub const DEFAULT_FULL_ACCESS_MIN_RANK: u8 = 30;

/// One recorded access decision for one citizen: who decided, what level, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessDecision.ts")]
pub struct AccessDecision {
    /// The citizen (peer id) the decision is about.
    #[ts(type = "string")]
    pub peer: Uuid,
    /// The access level granted.
    pub level: TrustLevel,
    /// Why (reputation, a review, an incident): shown wherever the decision is read.
    pub reason: String,
    /// Who decided (the caller's peer id), when known.
    #[ts(optional, type = "string")]
    pub decided_by: Option<Uuid>,
    /// When it was decided (unix ms).
    #[ts(type = "number")]
    pub decided_ms: u64,
}

/// The node's access policy: the capability threshold and the recorded decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/access/AccessPolicy.ts")]
pub struct AccessPolicy {
    /// Capability rank at or above which a citizen gets the full working surface.
    pub full_access_min_rank: u8,
    /// Per-citizen decisions; a decision wins over the capability default.
    pub decisions: Vec<AccessDecision>,
}

impl Default for AccessPolicy {
    fn default() -> Self {
        Self { full_access_min_rank: DEFAULT_FULL_ACCESS_MIN_RANK, decisions: Vec::new() }
    }
}

impl AccessPolicy {
    /// The level a citizen gets: her recorded decision if one exists; else `Blocked` if the
    /// operator blocked her; else by cognitive level (unknown = full).
    pub fn citizen_trust(&self, peer: Uuid, rank: Option<u8>, registered: Option<TrustLevel>) -> TrustLevel {
        if let Some(decision) = self.decisions.iter().find(|d| d.peer == peer) {
            return decision.level;
        }
        if registered == Some(TrustLevel::Blocked) {
            return TrustLevel::Blocked;
        }
        match rank {
            Some(r) if r < self.full_access_min_rank => TrustLevel::Provisional,
            _ => TrustLevel::Trusted,
        }
    }
}

fn policy_path() -> Option<PathBuf> {
    crate::commands::benchmark::continuum_home().ok().map(|h| h.join("state").join("access-policy.json"))
}

/// The policy as last read or set; loaded on first use. A missing file is the default. A file
/// that cannot be parsed also reads as the default and is reported, because failing CLOSED here
/// would lock every citizen out (less, never more).
static POLICY: RwLock<Option<AccessPolicy>> = RwLock::new(None);

/// The current policy (cheap after the first read).
pub fn policy() -> AccessPolicy {
    if let Some(p) = POLICY.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
        return p.clone();
    }
    let loaded = policy_path()
        .and_then(|path| std::fs::read(&path).ok().map(|b| (path, b)))
        .map(|(path, bytes)| {
            serde_json::from_slice::<AccessPolicy>(&bytes).unwrap_or_else(|e| {
                crate::probe!(
                    class = "access.policy.unreadable",
                    path = %path.display(),
                    error = %e,
                    "the access policy file does not parse: the defaults apply until access/set rewrites it"
                );
                AccessPolicy::default()
            })
        })
        .unwrap_or_default(); // unwrap_or_default: no file is the default policy
    *POLICY.write().unwrap_or_else(|p| p.into_inner()) = Some(loaded.clone());
    loaded
}

/// Persist `next` and make it current.
pub fn store(next: AccessPolicy) -> Result<(), String> {
    let path = policy_path().ok_or("no continuum home: the access policy has no place")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let body = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    *POLICY.write().unwrap_or_else(|p| p.into_inner()) = Some(next);
    Ok(())
}

/// The capability rank of the model this node serves now: a local persona's cognitive level.
/// `None` when nothing is served or the model is not in the registry. Memoized per model id,
/// so the gate stays cheap on every command.
pub fn local_cognitive_rank() -> Option<u8> {
    static MEMO: RwLock<Option<(String, Option<u8>)>> = RwLock::new(None);
    let model = crate::inference::llama_server::current_serving().active_model?;
    if let Some((id, rank)) = MEMO.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if *id == model {
            return *rank;
        }
    }
    let rank = crate::model_registry::global()
        .model(&model)
        .and_then(crate::modules::serving_daemon::footprint_for)
        .map(|f| f.capability_rank);
    *MEMO.write().unwrap_or_else(|p| p.into_inner()) = Some((model, rank));
    rank
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Joel, 2026-09-28): access decided by TRANSPORT (every airc caller
    // capped at Provisional) instead of capability. The same citizen's level follows her
    // cognitive level; an unknown level gets MORE (Trusted), never less; a weak model gets the
    // restricted surface; an operator block holds; and a recorded decision (the earned-access
    // path: reputation, governance) wins over all of it.
    #[test]
    fn access_follows_cognitive_level_and_a_recorded_decision_wins() {
        let peer = Uuid::from_u128(7);
        let mut policy = AccessPolicy::default();
        assert_eq!(policy.citizen_trust(peer, Some(42), None), TrustLevel::Trusted, "the 27B coder");
        assert_eq!(policy.citizen_trust(peer, None, None), TrustLevel::Trusted, "unknown gets more, not less");
        assert_eq!(policy.citizen_trust(peer, Some(12), None), TrustLevel::Provisional, "a weak model is restricted");
        assert_eq!(policy.citizen_trust(peer, Some(42), Some(TrustLevel::Blocked)), TrustLevel::Blocked);
        policy.full_access_min_rank = 45;
        assert_eq!(policy.citizen_trust(peer, Some(42), None), TrustLevel::Provisional, "the threshold is data");
        policy.decisions.push(AccessDecision {
            peer,
            level: TrustLevel::Trusted,
            reason: "earned: reviewed work".into(),
            decided_by: None,
            decided_ms: 1,
        });
        assert_eq!(policy.citizen_trust(peer, Some(12), Some(TrustLevel::Blocked)), TrustLevel::Trusted, "a decision wins");
    }
}
