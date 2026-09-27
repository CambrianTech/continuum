//! A trained gene is judged in her own work, never in a harness beside it (Joel,
//! 2026-09-27: "The point is integrated not parallel").
//!
//! A gene that passes training's own pre-filter opens a [`GeneTrial`]: it is registered
//! (so the serving engine loads it, dormant, in place), and from then on each CARD she
//! works draws an arm, seeded by the trial and the card: the candidate arm works the card
//! with the gene paged in, the stable arm with her promoted genome alone. The draw is per
//! card, never per turn or per call, because the room judges cards: a settle verdict, the
//! tests, a review. Every turn's receipt names the genes that ran
//! ([`GenerationReceipt::genes`](crate::cognition::provenance::GenerationReceipt)), so a
//! card's outcome is credited to exactly the genome that worked it. The gate reads those
//! outcomes at a boundary and promotes or retires the gene; the trial row is the adoption
//! receipt (what was decided, on what counts, when), and a promoted gene's row is what
//! rolls it back.
//!
//! One small JSON file, bounded by the genes ever trialled, not by turns.

use crate::ai::types::ActiveAdapterRequest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The share of her cards a candidate gene works, in thousandths. Half: the fastest a
/// verdict can arrive (both arms fill at the same rate) while half her work stays on the
/// genome she has.
pub const DEFAULT_SHARE_MILLI: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialState {
    /// Working a share of her cards; no verdict yet.
    Trial,
    /// Part of her genome: every card runs it.
    Promoted,
    /// Out of her genome and out of the serving catalog.
    Retired,
}

/// Settled cards and how many passed, for one arm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmTally {
    pub settled: u32,
    pub passed: u32,
}

/// What the gate decided, on what evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialVerdict {
    pub candidate: ArmTally,
    pub stable: ArmTally,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneTrial {
    pub id: Uuid,
    pub persona_id: Uuid,
    /// The gene's name, as the adapter manifest and the fitness ledger speak it.
    pub alias: String,
    /// The GGUF-lora on disk; also what a receipt's `genes` names.
    pub path: PathBuf,
    /// The continuum id of the base it was trained on: it only rides turns served by it.
    pub base_model_id: String,
    pub share_milli: u32,
    pub opened_at_ms: u64,
    pub state: TrialState,
    #[serde(default)]
    pub decided_at_ms: Option<u64>,
    #[serde(default)]
    pub verdict: Option<TrialVerdict>,
}

impl GeneTrial {
    /// PURE: whether `card` works in the candidate arm. Seeded by the trial and the card
    /// alone, so every turn of one card lands in the same arm, across restarts, and two
    /// trials split her cards independently.
    pub fn candidate_arm(&self, card: Uuid) -> bool {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(self.id.as_bytes());
        h.update(card.as_bytes());
        let d = h.finalize();
        let draw = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) % 1000;
        draw < self.share_milli.min(1000)
    }

    fn page(&self) -> ActiveAdapterRequest {
        ActiveAdapterRequest {
            name: self.alias.clone(),
            path: self.path.to_string_lossy().into_owned(),
            domain: String::new(),
            scale: 1.0,
        }
    }
}

/// PURE: the genes a turn of `persona` runs with on `base`: every promoted gene, plus each
/// open trial's gene when `card` drew its candidate arm. A turn with no card (a
/// conversation turn) runs the promoted genome alone: only a card has an outcome to
/// credit a candidate with.
pub fn genes_for_turn(
    trials: &[GeneTrial],
    persona: Uuid,
    base: &str,
    card: Option<Uuid>,
) -> Vec<ActiveAdapterRequest> {
    trials
        .iter()
        .filter(|t| t.persona_id == persona && t.base_model_id == base)
        .filter(|t| match t.state {
            TrialState::Promoted => true,
            TrialState::Trial => card.is_some_and(|c| t.candidate_arm(c)),
            TrialState::Retired => false,
        })
        .map(GeneTrial::page)
        .collect()
}

/// The genes her next turn runs with, read from the trial file and the base the node serves
/// right now. `None` when either cannot be read: the caller then leaves her genome as it is
/// (never a guessed one), and the reason is on the probe stream.
pub fn live_genes(persona: Uuid, card: Option<Uuid>) -> Option<Vec<ActiveAdapterRequest>> {
    let base = crate::inference::llama_server::current_serving().active_model?;
    let store = GeneTrials::default_store()?;
    match store.load() {
        Ok(trials) => Some(genes_for_turn(&trials, persona, &base, card)),
        Err(error) => {
            crate::probe!(
                class = "genome.trial.unreadable",
                persona = %persona,
                error = error.as_str(),
                "the gene trial file did not read: this turn keeps the genome she has"
            );
            None
        }
    }
}

/// The trial file.
pub struct GeneTrials {
    path: PathBuf,
}

impl GeneTrials {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `~/.continuum/genome/trials.json`; `None` without a home directory.
    pub fn default_store() -> Option<Self> {
        dirs::home_dir().map(|h| Self::at(h.join(".continuum").join("genome").join("trials.json")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every trial. A missing file is no trials; an unreadable one is an error the caller
    /// surfaces (a turn then runs its current genome, never a guessed one).
    pub fn load(&self) -> Result<Vec<GeneTrial>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(body) => serde_json::from_str(&body)
                .map_err(|e| format!("gene trials {} unreadable: {e}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("gene trials {}: {e}", self.path.display())),
        }
    }

    fn save(&self, all: &[GeneTrial]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Open a trial for a freshly trained gene. A gene already on file (the same path for
    /// the same persona) is not opened twice: its existing row is returned.
    pub fn open(
        &self,
        persona_id: Uuid,
        alias: &str,
        path: &Path,
        base_model_id: &str,
        now_ms: u64,
    ) -> Result<GeneTrial, String> {
        let mut all = self.load()?;
        if let Some(existing) = all.iter().find(|t| t.persona_id == persona_id && t.path == path) {
            return Ok(existing.clone());
        }
        let trial = GeneTrial {
            id: Uuid::new_v4(),
            persona_id,
            alias: alias.to_string(),
            path: path.to_path_buf(),
            base_model_id: base_model_id.to_string(),
            share_milli: DEFAULT_SHARE_MILLI,
            opened_at_ms: now_ms,
            state: TrialState::Trial,
            decided_at_ms: None,
            verdict: None,
        };
        all.push(trial.clone());
        self.save(&all)?;
        Ok(trial)
    }

    /// Record the gate's decision on an open trial. A trial already decided is left as it
    /// is (a decision is made once; a later reversal is a new trial).
    pub fn decide(
        &self,
        id: Uuid,
        state: TrialState,
        verdict: TrialVerdict,
        now_ms: u64,
    ) -> Result<Option<GeneTrial>, String> {
        let mut all = self.load()?;
        let Some(t) = all.iter_mut().find(|t| t.id == id && t.state == TrialState::Trial) else {
            return Ok(None);
        };
        t.state = state;
        t.decided_at_ms = Some(now_ms);
        t.verdict = Some(verdict);
        let decided = t.clone();
        self.save(&all)?;
        Ok(Some(decided))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a card's outcome credited to a genome that did not work it. The arm
    // must be one fixed answer per (trial, card), whatever turn or restart asks; the share
    // must actually split her cards; a conversation turn (no card) never runs a candidate;
    // a promoted gene runs on every turn of its base and only its base; a retired one on none.
    #[test]
    fn a_card_draws_one_arm_and_only_its_base_runs_the_gene() {
        let dir = tempfile::tempdir().expect("test: dir");
        let store = GeneTrials::at(dir.path().join("trials.json"));
        let kimi = Uuid::from_u128(0x6b1);
        let t = store.open(kimi, "kimi-dream-1", Path::new("/genes/k1.gguf"), "qwen-27b", 10).unwrap();
        assert_eq!(store.open(kimi, "again", Path::new("/genes/k1.gguf"), "qwen-27b", 20).unwrap().id, t.id, "one trial per gene");

        let cards: Vec<Uuid> = (0..400u128).map(Uuid::from_u128).collect();
        let on = cards.iter().filter(|c| t.candidate_arm(**c)).count();
        assert!((140..=260).contains(&on), "about half of 400 cards draw the candidate: {on}");
        for c in &cards[..20] {
            assert_eq!(t.candidate_arm(*c), t.candidate_arm(*c));
        }
        let trials = store.load().unwrap();
        let card_on = *cards.iter().find(|c| t.candidate_arm(**c)).unwrap();
        let card_off = *cards.iter().find(|c| !t.candidate_arm(**c)).unwrap();
        assert_eq!(genes_for_turn(&trials, kimi, "qwen-27b", Some(card_on)).len(), 1);
        assert!(genes_for_turn(&trials, kimi, "qwen-27b", Some(card_off)).is_empty());
        assert!(genes_for_turn(&trials, kimi, "qwen-27b", None).is_empty(), "no card, no candidate");
        assert!(genes_for_turn(&trials, kimi, "other-base", Some(card_on)).is_empty());
        assert!(genes_for_turn(&trials, Uuid::nil(), "qwen-27b", Some(card_on)).is_empty());

        let verdict = TrialVerdict { candidate: ArmTally { settled: 6, passed: 4 }, stable: ArmTally { settled: 6, passed: 3 }, reason: "no worse".into() };
        store.decide(t.id, TrialState::Promoted, verdict.clone(), 30).unwrap().expect("decided");
        assert!(store.decide(t.id, TrialState::Retired, verdict, 40).unwrap().is_none(), "decided once");
        let trials = store.load().unwrap();
        assert_eq!(genes_for_turn(&trials, kimi, "qwen-27b", None).len(), 1, "promoted runs every turn");
        assert_eq!(genes_for_turn(&trials, kimi, "qwen-27b", Some(card_off))[0].path, "/genes/k1.gguf");
    }
}
