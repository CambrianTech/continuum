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
    /// The room's outcomes so far, one per settled card, by the genome that worked it.
    #[serde(default)]
    pub candidate: ArmTally,
    #[serde(default)]
    pub stable: ArmTally,
    /// The cards already credited: a card's verdict can arrive more than once (a round
    /// re-grades an instance), and a card counts once.
    #[serde(default)]
    pub credited_cards: Vec<Uuid>,
    /// How many of the gate's [`CHECKPOINTS`] have been looked at: each is looked at once.
    #[serde(default)]
    pub looked: u32,
}

/// The only points at which the gate looks: when BOTH arms have reached this many settled
/// cards. Looking after every card lets chance cross the bar somewhere along the way (a
/// no-effect gene promoted 6-11% of the time at a 0.975 bar, simulated); four looks keep a
/// no-effect gene's promotion at 3-8%, a harmful one's at 1%, and still promote a gene
/// that lifts her pass rate by 0.2 about half the time within 40 cards (Cormac on #4476:
/// promoted genes become the next baseline, so a noise gene is drift).
pub const CHECKPOINTS: [u32; 4] = [10, 20, 30, 40];
/// How sure the gate must be, at a checkpoint, that her cards pass MORE often with the
/// gene before it joins her genome; below [`RETIRE_BELOW`] it is likely worse and retires.
/// At the last checkpoint, anything not promoted retires: no gain shown.
pub const PROMOTE_AT: f64 = 0.975;
pub const RETIRE_BELOW: f64 = 0.10;

/// ln Γ(n) for a whole n ≥ 1: ln((n-1)!), exact as a sum (every argument the gate forms is
/// a whole number of cards plus one).
fn ln_gamma_whole(n: u32) -> f64 {
    (2..n).map(|k| f64::from(k).ln()).sum()
}

fn ln_beta(a: u32, b: u32) -> f64 {
    ln_gamma_whole(a) + ln_gamma_whole(b) - ln_gamma_whole(a + b)
}

/// PURE: the probability that the gene's pass rate is above her genome's, given the cards
/// each arm settled, each rate uniform before any card (Beta(1,1)), so Beta(passed+1,
/// failed+1) after. Exact, by the closed form for two Beta variables (Evan Miller):
/// P(B > A) = Σ_{i<αB} B(αA+i, βA+βB) / ((βB+i)·B(1+i, βB)·B(αA, βA)).
pub fn p_gene_better(candidate: ArmTally, stable: ArmTally) -> f64 {
    let (ab, bb) = (candidate.passed + 1, candidate.settled - candidate.passed + 1);
    let (aa, ba) = (stable.passed + 1, stable.settled - stable.passed + 1);
    (0..ab)
        .map(|i| {
            (ln_beta(aa + i, ba + bb) - f64::from(bb + i).ln() - ln_beta(1 + i, bb) - ln_beta(aa, ba)).exp()
        })
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// PURE: the gate, given the checkpoints already looked at. Returns how many checkpoints
/// have now been looked at, and the decision if one was reached. Between checkpoints it
/// neither looks nor decides.
pub fn gate(candidate: ArmTally, stable: ArmTally, looked: u32) -> (u32, Option<(TrialState, String)>) {
    let Some(&at) = CHECKPOINTS.get(looked as usize) else {
        return (looked, None);
    };
    if candidate.settled.min(stable.settled) < at {
        return (looked, None);
    }
    let looked = looked + 1;
    let p = p_gene_better(candidate, stable);
    let rates = format!(
        "{}/{} cards passed with the gene, {}/{} without; P(better) {:.3}",
        candidate.passed, candidate.settled, stable.passed, stable.settled, p
    );
    let decision = if p >= PROMOTE_AT {
        Some((TrialState::Promoted, format!("better in her work: {rates}")))
    } else if p < RETIRE_BELOW {
        Some((TrialState::Retired, format!("worse in her work: {rates}")))
    } else if looked as usize == CHECKPOINTS.len() {
        Some((TrialState::Retired, format!("no gain shown in her work: {rates}")))
    } else {
        None
    };
    (looked, decision)
}

/// One turn of a settled card, as the gate reads it: when it was staged, and, for each
/// generation SERVED by an engine that applies adapters, the model that served it and the
/// genes that ran. A cloud call ignores `active_adapters` and a fault ran nothing, so
/// neither says anything about a gene (Cormac on #4473).
#[derive(Debug, Clone, Default)]
pub struct CardTurn {
    pub staged_at_ms: u64,
    pub served: Vec<(String, std::collections::BTreeSet<String>)>,
}

impl CardTurn {
    /// PURE: the gate's reading of one staged turn's receipts.
    pub fn from_receipts<'a>(
        staged_at_ms: u64,
        receipts: impl IntoIterator<Item = &'a crate::cognition::provenance::GenerationReceipt>,
    ) -> Self {
        use crate::cognition::provenance::GenerationOutcome;
        let served = receipts
            .into_iter()
            .filter_map(|r| match &r.outcome {
                GenerationOutcome::Served { model, provider, .. }
                    if provider == crate::inference::llama_server::PROVIDER_ID =>
                {
                    Some((model.clone(), r.genes.iter().cloned().collect()))
                }
                _ => None,
            })
            .collect();
        Self { staged_at_ms, served }
    }
}

/// PURE: which arm of trial `t` a settled card belongs to, or `None` when it is no evidence
/// about `t` at all (Codex on #4476: a card must never be credited to a trial it was not
/// part of). A card counts for `t` only when EVERY one of its turns was staged after `t`
/// opened (the arm is drawn from the card's start), and only its generations served on
/// `t`'s base are read; with none, it says nothing. Then: the gene ran on every one of them
/// → candidate; on none → stable; on some → neither (a card that switched genome mid-way
/// cannot be credited to either).
pub fn arm_of(t: &GeneTrial, turns: &[CardTurn]) -> Option<bool> {
    if turns.is_empty() || turns.iter().any(|turn| turn.staged_at_ms < t.opened_at_ms) {
        return None;
    }
    let gene = t.path.to_string_lossy();
    let on_base: Vec<bool> = turns
        .iter()
        .flat_map(|turn| turn.served.iter())
        .filter(|(model, _)| *model == t.base_model_id)
        .map(|(_, genes)| genes.contains(gene.as_ref()))
        .collect();
    match (on_base.iter().all(|&ran| ran), on_base.iter().any(|&ran| ran)) {
        _ if on_base.is_empty() => None,
        (true, _) => Some(true),
        (false, false) => Some(false),
        (false, true) => None,
    }
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

/// Puts her genome back when a work turn ends, however it ends: the snapshot taken before
/// the turn by default, or her promoted genome when the turn got far enough to say so.
/// Restores on drop, so a cancelled or unwinding turn cannot leave a trial gene on her
/// cycle for the conversation turns that follow.
pub struct GenomeRestore {
    cycle: std::sync::Arc<crate::cognition::workspace::WorkspaceCycle>,
    to: Vec<ActiveAdapterRequest>,
}

impl GenomeRestore {
    pub fn snapshot(cycle: std::sync::Arc<crate::cognition::workspace::WorkspaceCycle>) -> Self {
        let to = cycle.genome();
        Self { cycle, to }
    }

    pub fn restore_to(&mut self, genes: Vec<ActiveAdapterRequest>) {
        self.to = genes;
    }
}

impl Drop for GenomeRestore {
    fn drop(&mut self) {
        self.cycle.page_in(std::mem::take(&mut self.to));
    }
}

/// The room judged a card: credit it to her open trials, and carry out any decision it
/// completes. Called where a card's verdict settles her staged credit
/// (`training_producer::settle_card_credit`), with that card's receipts.
pub fn credit_settled_card(persona: Uuid, card: Uuid, passed: bool, turns: &[CardTurn]) {
    let Some(store) = GeneTrials::default_store() else {
        return;
    };
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let decided = match store.credit_card(persona, card, passed, turns, now_ms) {
        Ok(d) => d,
        Err(error) => {
            crate::probe!(
                class = "genome.trial.unreadable",
                persona = %persona,
                error = error.as_str(),
                "the gene trial file did not take this card's outcome"
            );
            return;
        }
    };
    for t in decided {
        apply_decision(&t, now_ms);
    }
}

/// The effects of a decision. Promoted: its verdict joins the fitness ledger gene recall
/// ranks by (`progress/<persona>.jsonl`, the rows `cognition/eval` wrote before). Retired:
/// out of the adapter manifest, so the serving engine retires it in place, and the same
/// ledger row records the loss. Either way the trial row, already written, is the receipt.
fn apply_decision(t: &GeneTrial, now_ms: u64) {
    if t.state == TrialState::Retired {
        if let Err(error) = crate::forge::adapter_manifest::unregister(&t.path) {
            crate::probe!(
                class = "genome.trial.unregister_failed",
                gene = t.alias.as_str(),
                error = error.as_str(),
                "a retired gene stayed in the adapter manifest: the engine keeps it loaded (dormant) until it is removed"
            );
        }
    }
    if let Some(dir) = crate::genome::fitness_ledger::GeneFitnessIndex::default_dir() {
        let row = fitness_receipt(t, now_ms);
        let line = format!("{row}\n");
        let path = dir.join(format!("{}.jsonl", t.persona_id));
        let written = std::fs::create_dir_all(&dir).and_then(|_| {
            use std::io::Write;
            std::fs::OpenOptions::new().create(true).append(true).open(&path)?.write_all(line.as_bytes())
        });
        if let Err(error) = written {
            crate::probe!(
                class = "genome.trial.ledger_unwritten",
                gene = t.alias.as_str(),
                error = %error,
                "the verdict did not reach the fitness ledger: recall ranks this gene without it"
            );
        }
    }
    let verdict = t.verdict.as_ref();
    crate::probe!(
        class = "genome.trial.decided",
        persona = %t.persona_id,
        gene = t.alias.as_str(),
        trial = %t.id,
        promoted = t.state == TrialState::Promoted,
        candidate_passed = t.candidate.passed as u64,
        candidate_settled = t.candidate.settled as u64,
        stable_passed = t.stable.passed as u64,
        stable_settled = t.stable.settled as u64,
        reason = verdict.map_or("", |v| v.reason.as_str()),
        "her work decided a gene: promoted into her genome, or retired out of it"
    );
}

/// Preserve the evidence behind a scalar lift in the existing fitness ledger.
/// A path identifies the trial artifact locally; it is not a content hash or
/// proof of compatibility with another base or combination of adapters.
fn fitness_receipt(t: &GeneTrial, now_ms: u64) -> serde_json::Value {
    let rate = |a: ArmTally| if a.settled == 0 { 0.0 } else { f64::from(a.passed) / f64::from(a.settled) };
    let (candidate, stable) = t.verdict.as_ref()
        .map_or((t.candidate, t.stable), |v| (v.candidate, v.stable));
    serde_json::json!({
        "geneId": t.alias,
        "lift": rate(candidate) - rate(stable),
        "passRate": rate(candidate),
        "basePassRate": rate(stable),
        "capturedAtMs": now_ms,
        "source": "her-work",
        "trial": t.id,
        "personaId": t.persona_id,
        "baseModelId": t.base_model_id,
        "adapterPath": t.path,
        "openedAtMs": t.opened_at_ms,
        "trialState": t.state,
        "candidate": candidate,
        "stable": stable,
        "verdict": t.verdict,
        "creditedCardIds": t.credited_cards,
    })
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
            candidate: ArmTally::default(),
            stable: ArmTally::default(),
            credited_cards: Vec::new(),
            looked: 0,
        };
        all.push(trial.clone());
        self.save(&all)?;
        Ok(trial)
    }

    /// Credit one settled card to each open trial of `persona` it is evidence about
    /// ([`arm_of`]), once per card. Returns the trials the gate decided on this credit
    /// (already written as decided).
    pub fn credit_card(
        &self,
        persona: Uuid,
        card: Uuid,
        passed: bool,
        turns: &[CardTurn],
        now_ms: u64,
    ) -> Result<Vec<GeneTrial>, String> {
        let mut all = self.load()?;
        let mut decided = Vec::new();
        let mut changed = false;
        for t in all.iter_mut().filter(|t| t.persona_id == persona && t.state == TrialState::Trial) {
            if t.credited_cards.contains(&card) {
                continue;
            }
            let Some(candidate) = arm_of(t, turns) else {
                continue;
            };
            // Checked against the card's OWN draw (Cormac on #4476): a card that drew the gene
            // but ran without it (an unreadable trial file that turn) is neither arm's
            // evidence, and neither is one that ran a gene it did not draw.
            if candidate != t.candidate_arm(card) {
                continue;
            }
            let arm = if candidate { &mut t.candidate } else { &mut t.stable };
            arm.settled += 1;
            arm.passed += u32::from(passed);
            t.credited_cards.push(card);
            changed = true;
            let (looked, decision) = gate(t.candidate, t.stable, t.looked);
            t.looked = looked;
            if let Some((state, reason)) = decision {
                t.state = state;
                t.decided_at_ms = Some(now_ms);
                t.verdict = Some(TrialVerdict { candidate: t.candidate, stable: t.stable, reason });
                decided.push(t.clone());
            }
        }
        if changed {
            self.save(&all)?;
        }
        Ok(decided)
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
        let decided = store.decide(t.id, TrialState::Promoted, verdict.clone(), 30).unwrap().expect("decided");
        // Regression: flattening a verdict into lift alone loses base, sample
        // size and lineage, and decide()'s supplied tallies need not match t's counters.
        let receipt = fitness_receipt(&decided, 30);
        assert_eq!(receipt["baseModelId"], "qwen-27b");
        assert_eq!(receipt["personaId"], kimi.to_string());
        assert_eq!(receipt["trial"], t.id.to_string());
        assert_eq!(receipt["adapterPath"], "/genes/k1.gguf");
        assert_eq!(receipt["candidate"]["settled"], 6);
        assert_eq!(receipt["stable"]["passed"], 3);
        assert_eq!(receipt["trialState"], "promoted");
        assert_eq!(receipt["verdict"]["reason"], "no worse");
        assert!((receipt["lift"].as_f64().expect("test: numeric lift") - 1.0 / 6.0).abs() < 1e-12);
        assert!(store.decide(t.id, TrialState::Retired, verdict, 40).unwrap().is_none(), "decided once");
        let trials = store.load().unwrap();
        assert_eq!(genes_for_turn(&trials, kimi, "qwen-27b", None).len(), 1, "promoted runs every turn");
        assert_eq!(genes_for_turn(&trials, kimi, "qwen-27b", Some(card_off))[0].path, "/genes/k1.gguf");
    }

    // what this catches (Cormac on #4476): a gate that promotes noise. A tie or a small lead
    // must not promote (promoted genes become the next baseline, so noise is drift); the gate
    // looks only at its checkpoints and each once; a likely gain promotes, a likely loss
    // retires, and at the last checkpoint anything not promoted retires. The probability must
    // be one: equal evidence is a coin, and the two directions sum to one.
    #[test]
    fn the_gate_looks_at_checkpoints_and_promotes_only_a_likely_gain() {
        let a = |passed, settled| ArmTally { settled, passed };
        for (c, s) in [(a(3, 6), a(3, 6)), (a(0, 10), a(0, 10)), (a(12, 20), a(12, 20))] {
            assert!((p_gene_better(c, s) - 0.5).abs() < 1e-9, "equal evidence is a coin: {c:?}");
        }
        assert!((p_gene_better(a(5, 6), a(1, 6)) + p_gene_better(a(1, 6), a(5, 6)) - 1.0).abs() < 1e-9);
        assert!(p_gene_better(a(5, 6), a(1, 6)) > p_gene_better(a(4, 6), a(1, 6)));
        assert_eq!(gate(a(9, 9), a(0, 30), 0), (0, None), "the gene arm has not reached the first checkpoint");
        assert_eq!(gate(a(6, 10), a(4, 10), 0), (1, None), "a small lead at 10 is looked at and not enough");
        assert_eq!(gate(a(7, 11), a(4, 11), 1), (1, None), "between checkpoints: no look");
        assert_eq!(gate(a(9, 10), a(2, 10), 0).1.map(|d| d.0), Some(TrialState::Promoted));
        assert_eq!(gate(a(1, 10), a(8, 10), 0).1.map(|d| d.0), Some(TrialState::Retired));
        let last = gate(a(20, 40), a(20, 40), 3);
        assert_eq!(last.1.as_ref().map(|d| d.0), Some(TrialState::Retired), "40 cards, no gain: noise, retired");
        assert!(last.1.unwrap().1.contains("no gain shown"));
        assert_eq!(gate(a(20, 40), a(20, 40), 4), (4, None), "past the last checkpoint: decided already");
    }

    // what this catches (Cormac on #4473, Codex on #4476): a card credited to a trial it was
    // never part of. Only SERVED generations from an engine that applies adapters count, and
    // only those served on the trial's base; a card begun before the trial opened, or one
    // that ran the gene on some turns and not others, is no evidence either way.
    #[test]
    fn a_card_is_evidence_only_about_a_trial_it_was_worked_under() {
        use crate::cognition::provenance::{GenerationOutcome, GenerationReceipt};
        let local = crate::inference::llama_server::PROVIDER_ID;
        let served = |provider: &str, model: &str, genes: &[&str]| GenerationReceipt {
            room_inputs: Vec::new(),
            submitted_request_id: "r".into(),
            outcome: GenerationOutcome::Served { model: model.into(), provider: provider.into(), provider_request_id: None },
            capture: None,
            genes: genes.iter().map(|g| g.to_string()).collect(),
        };
        let dir = tempfile::tempdir().expect("test: dir");
        let t = GeneTrials::at(dir.path().join("t.json"))
            .open(Uuid::from_u128(1), "g", Path::new("/genes/k1.gguf"), "qwen-27b", 100)
            .unwrap();
        let turn = |at: u64, rs: &[GenerationReceipt]| CardTurn::from_receipts(at, rs.iter());
        let with = served(local, "qwen-27b", &["/genes/k1.gguf"]);
        let without = served(local, "qwen-27b", &[]);
        assert_eq!(arm_of(&t, &[turn(200, &[with.clone()]), turn(300, &[with.clone()])]), Some(true));
        assert_eq!(arm_of(&t, &[turn(200, &[without.clone()])]), Some(false));
        assert_eq!(arm_of(&t, &[turn(50, &[without.clone()]), turn(200, &[with.clone()])]), None, "begun before the trial opened");
        assert_eq!(arm_of(&t, &[turn(200, &[served(local, "other-base", &[])])]), None, "another base says nothing");
        assert_eq!(arm_of(&t, &[turn(200, &[served("anthropic", "qwen-27b", &[])])]), None, "a cloud call ignores genes");
        let fault = GenerationReceipt::faulted("r", "x").with_genes(vec!["/genes/k1.gguf".into()]);
        assert_eq!(arm_of(&t, &[turn(200, &[fault])]), None, "a fault ran nothing");
        assert_eq!(arm_of(&t, &[turn(200, &[with.clone()]), turn(300, &[without.clone()])]), None, "switched mid-card");
        assert_eq!(arm_of(&t, &[turn(200, &[with, served(local, "other-base", &[])])]), Some(true), "only its base is read");
    }

    // what this catches: a card counted twice when its verdict arrives again, a card counted
    // in an arm it did not draw (Cormac on #4476), and a trial that never decides once both
    // arms reach a checkpoint.
    #[test]
    fn each_settled_card_counts_once_in_the_arm_it_drew_and_the_gate_decides_at_a_checkpoint() {
        use crate::cognition::provenance::{GenerationOutcome, GenerationReceipt};
        let dir = tempfile::tempdir().expect("test: dir");
        let store = GeneTrials::at(dir.path().join("trials.json"));
        let kimi = Uuid::from_u128(0x6b1);
        let t = store.open(kimi, "kimi-dream-1", Path::new("/genes/k1.gguf"), "qwen-27b", 10).unwrap();
        let served = |genes: &[&str]| GenerationReceipt {
            room_inputs: Vec::new(),
            submitted_request_id: "r".into(),
            outcome: GenerationOutcome::Served { model: "qwen-27b".into(), provider: crate::inference::llama_server::PROVIDER_ID.into(), provider_request_id: None },
            capture: None,
            genes: genes.iter().map(|g| g.to_string()).collect(),
        };
        let with = vec![CardTurn::from_receipts(20, [&served(&["/genes/k1.gguf"])])];
        let without = vec![CardTurn::from_receipts(20, [&served(&[])])];
        let drew: Vec<Uuid> = (0..400u128).map(Uuid::from_u128).filter(|c| t.candidate_arm(*c)).take(10).collect();
        let stayed: Vec<Uuid> = (0..400u128).map(Uuid::from_u128).filter(|c| !t.candidate_arm(*c)).take(10).collect();

        assert!(store.credit_card(kimi, stayed[0], true, &with, 20).unwrap().is_empty());
        assert!(store.credit_card(kimi, drew[0], true, &without, 20).unwrap().is_empty());
        let now = store.load().unwrap().into_iter().find(|x| x.id == t.id).unwrap();
        assert_eq!((now.candidate, now.stable), (ArmTally::default(), ArmTally::default()), "genome against its own draw: no evidence");

        assert!(store.credit_card(kimi, drew[0], true, &with, 21).unwrap().is_empty());
        assert!(store.credit_card(kimi, drew[0], true, &with, 22).unwrap().is_empty(), "the same card again");
        let now = store.load().unwrap().into_iter().find(|x| x.id == t.id).unwrap();
        assert_eq!(now.candidate, ArmTally { settled: 1, passed: 1 });

        for c in &drew[1..] {
            assert!(store.credit_card(kimi, *c, true, &with, 30).unwrap().is_empty());
        }
        let mut decided = Vec::new();
        for (k, c) in stayed.iter().enumerate() {
            decided = store.credit_card(kimi, *c, k < 2, &without, 40).unwrap();
        }
        let d = decided.first().expect("the tenth stable card reaches the first checkpoint");
        assert_eq!(d.state, TrialState::Promoted, "10/10 with the gene against 2/10 without");
        assert_eq!(d.looked, 1);
        assert!(store.credit_card(kimi, Uuid::from_u128(9999), false, &without, 50).unwrap().is_empty(), "a decided trial takes no more cards");
    }
}
