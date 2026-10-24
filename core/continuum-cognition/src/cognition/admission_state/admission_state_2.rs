use super::*;

impl AdmissionState {

    /// Algorithm 4 recall. Returns the top `limit` engrams ranked by
    /// `salience × recency-decay`, after applying decay to bring each
    /// engram's salience up to `now_ms`. Records a recall hit on the
    /// returned engrams (Hebbian rehearsal — use-it-keeps-it).
    ///
    /// Per [[source-drain-is-the-universal-pattern]] and the cognition-
    /// cache-hierarchy doc: salient + protected + recently-used
    /// engrams stay near the top; novel ones get the protection
    /// window; everything else drains. The substrate's continual-
    /// learning property compounds through this scoring — memory
    /// that gets used keeps coming back; memory that doesn't fades
    /// (but doesn't disappear — `apply_decay` honors SALIENCE_FLOOR).
    ///
    /// Returns engrams paired with their post-decay salience score so
    /// the caller can introspect what shaped the recall — per
    /// [[observability-is-half-the-architecture]] the cycle's L2 →
    /// prompt seam is observable, not opaque.
    pub fn recall_scored(&self, now_ms: u64, limit: usize) -> Vec<(Engram, f32)> {
        if limit == 0 {
            return Vec::new();
        }

        let engrams = self.engrams.lock().unwrap();

        // 1. Score every engram. apply_decay brings salience up to
        // `now_ms` for the engram (honors novelty protection +
        // SALIENCE_FLOOR + skips racing-tick double-decay).
        let mut scored: Vec<(Engram, f32)> = engrams
            .iter()
            .filter_map(|e| {
                self.recall_metadata.apply_decay(e.id, now_ms);
                self.recall_metadata
                    .get(e.id)
                    .map(|m| (e.clone(), m.salience))
            })
            .collect();
        drop(engrams);

        // 2. Top-N by score, descending. Stable sort so equal-score
        // engrams keep insertion order (recency tiebreak).
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        // 3. Record recall hits on the returned engrams. Hebbian
        // rehearsal: each access bumps salience + access_count + the
        // last_accessed clock. Without this step the scoring would
        // be one-way: salient memories would only ever decay, never
        // climb back from being used. record_recall_hit closes the
        // use-it-keeps-it feedback loop.
        //
        // After each in-memory mutation, observe through the
        // persistence sink so the disk row keeps pace. NoopSink
        // (default) does nothing; OrmPersistenceSink upserts the
        // engram_recall_metadata row through tokio::spawn.
        let sink = self.persistence.read().unwrap().clone();
        for (engram, _) in &scored {
            self.recall_metadata.record_recall_hit(engram.id, now_ms);
            if let Some(updated) = self.recall_metadata.get(engram.id) {
                sink.observe_metadata_update(engram.id, updated);
            }
        }

        scored
    }

    /// Score the top `limit` engrams by salience-decay — but DO NOT record recall
    /// hits. The hit (Hebbian rehearsal / salience uplift) must land on what a
    /// caller actually SURFACES, not on everything it scored. A relevance
    /// re-ranker (RecallFaculty) over-fetches candidates here, narrows by cosine
    /// similarity to the burst, then calls [`record_recall_hits`] on the final
    /// surfaced set — so the loop closes on the memories the persona truly used,
    /// not on candidates that lost the re-rank. (`recall_scored` is the
    /// no-re-ranker shortcut: score + hit in one pass.)
    pub fn recall_candidates(&self, now_ms: u64, limit: usize) -> Vec<(Engram, f32)> {
        if limit == 0 {
            return Vec::new();
        }
        // How long a wanderer INNER-SPEECH thought (EngramKind::SelfReflection — the
        // historian/dreamer lenses, #145) stays eligible for AMBIENT recall. A passing
        // thought is "current inner speech" only briefly; past this it has PASSED, and
        // resurfacing it as though it were current fact is a bug, not a memory. 15 min:
        // long enough that a fresh musing can still bubble up the same activity, short
        // enough that a stale self-assessment cannot masquerade as the present.
        const INNER_SPEECH_RECALL_TTL_MS: u64 = 15 * 60 * 1000;
        // The owning persona's id (if bound) — for the own-authored-chat gate below.
        // Snapshotted once, outside the per-engram closure.
        let owner = self.owner_id_snapshot();
        let engrams = self.engrams.lock().unwrap();
        // Enumerate in insertion order: `idx` is a monotonic recency rank
        // (higher == more recently admitted), used as the tiebreaker below.
        let mut scored: Vec<(usize, Engram, f32)> = engrams
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| {
                // Tool receipts — the persona's own "I ran X → result" — are
                // PROPRIOCEPTION, not durable knowledge. The recency/working-memory
                // channel carries them so she sees her own hands
                // ([[act-results-need-a-recency-channel-not-semantic-recall]]); they
                // must NOT compete in the SEMANTIC recall pool, where they were
                // drowning real memories (#166, seen live: recall surfaced "I ran
                // commands/list because Anwen is acting" as its top hits). Gate them
                // out here → recall returns knowledge (conversation, distilled
                // self-reflection), never the persona's own tool chatter. This is the
                // structural form of the earlier salience down-weight, which alone
                // couldn't stop them surfacing when knowledge was sparse.
                if matches!(e.origin, EngramOrigin::Tool(_)) {
                    return None;
                }
                // The persona's OWN authored chat is the SIBLING leak (#166, glass-boxed
                // 2026-07-17 on Casper: 271 self-authored "I ran X because Y is acting"
                // messages — receipt-mimicry SPEECH, #158 — admitted as durable Chat
                // engrams, recalling at salience 0.78-0.85 and teaching the survey-spin).
                // The Tool gate can't catch them (they're Chat-origin, not Tool). A
                // persona's own broadcast is proprioception, not external knowledge —
                // she said it, she knows she said it. Gate own-authored chat out of
                // AMBIENT recall the same way; it stays keyword-queryable
                // (`recall_by_keyword`). Fail-open: only when we can PROVE self-authorship
                // (owner bound AND sender matches) — never gate others' chat.
                if let (Some(owner_id), EngramOrigin::Chat(chat_ref)) = (owner, &e.origin) {
                    if chat_ref.sender_id == owner_id {
                        return None;
                    }
                }
                // Wanderer inner-speech (EngramKind::SelfReflection) is a PASSING
                // thought, not durable knowledge. A FRESH one may bubble up as ambient
                // inner speech (the arc's intent, dream_consolidation §wanderer); a STALE
                // one must NOT resurface as current fact. Glass-boxed 2026-07-14: Atlas
                // recalled a 25m-old "[thought:historian] you keep failing to claim" AS
                // present truth minutes after his claim SUCCEEDED, and rationalized the
                // contradiction into a loop — the "feedback vs rag" incoherence. Bound
                // its ambient-recall lifetime here; introspection still queries ALL of
                // them explicitly by `thought:<lens>` recall-key (`recall_by_keyword`, a
                // separate path). Dream-DISTILLED insight is EngramKind::Semantic
                // (durable) — untouched. Sibling of the Tool-receipt gate above (#166).
                if e.kind == crate::cognition::engram::EngramKind::SelfReflection
                    && now_ms.saturating_sub(e.admitted_at_ms) > INNER_SPEECH_RECALL_TTL_MS
                {
                    return None;
                }
                self.recall_metadata.apply_decay(e.id, now_ms);
                self.recall_metadata
                    .get(e.id)
                    .map(|m| (idx, e.clone(), m.salience))
            })
            .collect();
        drop(engrams);
        // Rank by salience, breaking ties by RECENCY (newest wins). Without the
        // recency tiebreak, recall-hit uplift flattens salience to a ceiling
        // (every surfaced memory rises toward 1.0), a stable sort then preserves
        // insertion order (oldest-first), and `truncate` evicts the NEWEST engram
        // — so a just-admitted act-observation never even reaches the re-ranker,
        // and the mind loops re-issuing the identical act, blind to its own hands.
        // Newest-wins-on-ties guarantees the freshest memory survives truncation;
        // the relevance re-rank (RecallFaculty) then surfaces it because it
        // literally contains what the burst is asking about.
        // See [[act-results-need-a-recency-channel-not-semantic-recall]].
        scored.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.cmp(&a.0))
        });
        scored.truncate(limit);
        scored.into_iter().map(|(_, e, s)| (e, s)).collect()
    }

    /// Record recall hits (salience uplift + access_count + persistence observe)
    /// on a specific set of engram ids — the memories a caller actually surfaced.
    /// The write half of the [`recall_candidates`] → re-rank → record loop.
    pub fn record_recall_hits(&self, ids: &[Uuid], now_ms: u64) {
        let sink = self.persistence.read().unwrap().clone();
        for id in ids {
            self.recall_metadata.record_recall_hit(*id, now_ms);
            if let Some(updated) = self.recall_metadata.get(*id) {
                sink.observe_metadata_update(*id, updated);
            }
        }
    }

    /// Recall a specific engram by id. None if not present in the store
    /// (either never admitted, or evicted in a future GC pass).
    pub fn recall_by_id(&self, id: Uuid) -> Option<Engram> {
        let engrams = self.engrams.lock().unwrap();
        engrams.iter().find(|e| e.id == id).cloned()
    }

    /// Recall engrams whose content contains `keyword` (case-insensitive
    /// substring match). Returns matches in newest-first order, capped
    /// at `limit`. v1 = linear scan over the in-memory store; PR-6 will
    /// add an ORM-side query / index.
    ///
    /// Empty `keyword` returns an empty Vec — the caller meant to skip
    /// search. (Avoids the gotcha where every engram contains the empty
    /// string.)
    pub fn recall_by_keyword(&self, keyword: &str, limit: usize) -> Vec<Engram> {
        if keyword.is_empty() || limit == 0 {
            return Vec::new();
        }
        let needle = keyword.to_lowercase();
        let engrams = self.engrams.lock().unwrap();
        engrams
            .iter()
            .rev()
            .filter(|e| e.content.to_lowercase().contains(&needle))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Recall engrams filtered by origin variant (Chat / Airc / Tool /
    /// SelfReflection). Newest first, capped at `limit`. Useful for
    /// callers that want "what did I learn from chat" vs "what did I
    /// learn from tool invocations".
    pub fn recall_by_origin_kind(&self, kind: EngramOriginKind, limit: usize) -> Vec<Engram> {
        if limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        engrams
            .iter()
            .rev()
            .filter(|e| EngramOriginKind::from(&e.origin) == kind)
            .take(limit)
            .cloned()
            .collect()
    }

}

/// Discriminator over `EngramOrigin` variants. Used by `recall_by_origin_kind`
/// so callers can filter without pattern-matching the full origin (which
/// carries variant-specific reference fields they don't need for the
/// filter decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngramOriginKind {
    Chat,
    Airc,
    Tool,
    SelfReflection,
    /// Authored by an external agent via the agent-memory bridge
    /// (`docs/cognition/AGENT-MEMORY-BRIDGE.md`).
    Agent,
}

impl From<&EngramOrigin> for EngramOriginKind {
    fn from(origin: &EngramOrigin) -> Self {
        match origin {
            EngramOrigin::Chat(_) => Self::Chat,
            EngramOrigin::Airc(_) => Self::Airc,
            EngramOrigin::Tool(_) => Self::Tool,
            EngramOrigin::SelfReflection { .. } => Self::SelfReflection,
            EngramOrigin::Agent(_) => Self::Agent,
        }
    }
}
