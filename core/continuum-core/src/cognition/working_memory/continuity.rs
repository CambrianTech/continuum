//! `WorkingMemory` pins, notices, facts, and the snapshot/restore (pause/resume) seam.

use super::*;

impl WorkingMemory {
    /// Record a PERCEPTION FACT ([unfulfilled]/[unverified]/[confabulation]/
    /// nudges) — proprioception ABOUT the mind, not a tool execution. Renders
    /// its own bracket tag and NEVER a receipt number: the 2026-07-12
    /// suppression onion was facts wearing `[action #N]` costumes and every
    /// consumer misreading them as receipts. Same rolling buffer + aging.
    /// Pin a keyed fact: replaces the fact under `key`, survives every eviction,
    /// renders first among the notices. For conditions that hold across acts
    /// (her hands' root, her checkout's environment) — `record_fact` is for
    /// one-off observations that may age out.
    /// Text is required prompt input. Producers own its size; raw payloads belong
    /// in the existing retrievable-result path, not an unbounded status notice.
    pub fn pin_fact(&self, key: &str, text: &str) {
        self.pin_fact_for_turns(key, text, 1);
    }

    /// Pin a keyed fact that outlives the turn it was made in: it survives
    /// `turns` calls to [`end_of_work_turn`](Self::end_of_work_turn). A release
    /// made during turn N must be read at the START of turn N+1 (the next pull
    /// decision), so it is pinned with `turns = 2` — the releasing turn's end
    /// takes one, the next turn's end takes the last (IntelMac's review of #3848:
    /// pin and unpin in the same restore block is a no-op).
    pub fn pin_fact_for_turns(&self, key: &str, text: &str, turns: u8) {
        self.pin_notice(key, text, turns, None);
    }

    /// A source-owned expansion reference remains required while its full detail
    /// competes with history. Both share the same key and turn lifetime.
    pub(crate) fn pin_expandable_fact(&self, key: &str, reference: &str, detail: Option<String>) {
        self.pin_notice(key, reference, 1, detail);
    }

    fn pin_notice(&self, key: &str, text: &str, turns: u8, detail: Option<String>) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        let turns = turns.max(1);
        let mut p = self.pinned.lock();
        if let Some(slot) = p.iter_mut().find(|f| f.key == key) {
            slot.text = t.to_string();
            slot.turns_left = turns;
            slot.detail = detail;
        } else {
            p.push(TurnPinnedFact {
                key: key.to_string(),
                text: t.to_string(),
                turns_left: turns,
                detail,
            });
        }
    }

    /// A work turn ended (her hands are home): every pin loses one turn; pins at
    /// zero leave. Called once per work turn at the restore, so a 1-turn pin made
    /// at rooting lives exactly that turn and a 2-turn pin reaches the next.
    pub fn end_of_work_turn(&self) {
        let mut p = self.pinned.lock();
        for f in p.iter_mut() {
            f.turns_left = f.turns_left.saturating_sub(1);
        }
        p.retain(|f| f.turns_left > 0);
    }

    /// The pinned facts in pin order — what the notices open with.
    pub fn pinned_facts(&self) -> Vec<String> {
        self.pinned.lock().iter().map(|f| f.text.clone()).collect()
    }

    pub(crate) fn notice_details(&self) -> Vec<String> {
        self.pinned.lock().iter().filter_map(|fact| fact.detail.clone()).collect()
    }

    /// Substrate notices use the input projection, not the optional thought bid.
    /// Their storage, scope and lifetime remain owned by this memory. The entry
    /// ring is count-bounded; neither fact text nor keyed pins have a byte ceiling.
    pub(crate) fn notices(&self) -> Vec<String> {
        let scope = self.scope.lock().clone();
        let mut notices = self.pinned_facts();
        notices.extend(self.entries.lock().iter()
            .filter(|entry| matches!(entry.kind, WmKind::Fact)
                && (entry.scope.is_none() || entry.scope == scope))
            .map(|entry| entry.text.clone()));
        notices
    }

    /// Drop the pinned fact under `key` (the condition ended: hands restored).
    pub fn unpin_fact(&self, key: &str) {
        self.pinned.lock().retain(|f| f.key != key);
    }

    /// Records required prompt input. Producers must account for text size; the
    /// shared prompt budget refuses oversized notices instead of silently clipping.
    pub fn record_fact(&self, fact: &str) {
        let f = fact.trim();
        if f.is_empty() {
            return;
        }
        let mut e = self.entries.lock();
        e.push_back(WmEntry {
            disk_change: Default::default(),
            kind: WmKind::Fact,
            text: f.to_string(),
            acts: Vec::new(),
            native_calls: Vec::new(),
            scope: None,
        });
        while e.len() > self.capacity {
            e.pop_front();
        }
    }

    /// Snapshot the volatile tier for PAUSE/RESUME across deploy reboots — the
    /// Memento fix (Joel 2026-07-12: "they wake up blank like Memento — an
    /// engineering failure; the flywheel falls apart"). Nine reboots today =
    /// nine blank wakes; the mind's momentum IS the product. The snapshot is
    /// the same serialization grid-sync will ship (one format, one seam —
    /// [[persona-mind-persists-across-shutdowns]]).
    pub fn snapshot(&self) -> VolatileSnapshot {
        // ONE lock for both parallel arrays. Two `.lock()` calls inside the same
        // struct literal self-deadlock: literal temporaries (the first guard) live
        // until the whole expression ends, so the second lock waits on ourselves
        // forever. Found as a 100%-reproducible test hang, 2026-08-15 — every
        // snapshot save (i.e. every deploy pause) would have wedged the same way.
        let (entries, action_statements) = {
            let ring = self.entries.lock();
            let mut entries = Vec::new();
            let mut authored = Vec::new();
            for (index, entry) in ring.iter().enumerate() {
                if matches!(entry.kind, WmKind::ActionStatement { .. }) {
                    authored.push((index, entry.clone()));
                } else {
                    entries.push(entry.clone());
                }
            }
            (entries, authored)
        };
        // The log, projected into the wire shape older builds read (see
        // `VolatileSnapshot::action_records`), under one lock so a concurrent collapse
        // cannot move a record between the projections.
        let archive_chars = self.budget().receipt_archive_chars();
        let last_action = self.last_action.lock().clone();
        let (archive, recent_results, result_scopes, pointers) = {
            let actions = self.actions.lock();
            let archive: Vec<(u64, Option<Uuid>, String)> =
                actions.archive(archive_chars).into_iter().map(|r| (r.seq, r.room, r.head.clone())).collect();
            let recent_results: Vec<(u64, Option<Uuid>, String, String, Option<String>)> = actions
                .retained()
                .filter_map(|r| r.result.clone().map(|result| (r.seq, r.room, result, r.operation.clone(), r.handle.clone())))
                .collect();
            let result_scopes: BTreeMap<u64, String> = actions
                .retained()
                .filter_map(|r| r.scope.clone().map(|scope| (r.seq, scope)))
                .collect();
            let pointers: Vec<(u64, String, String)> = actions
                .indexed_pointers()
                .into_iter()
                .filter_map(|r| r.handle.clone().map(|handle| (r.seq, super::collapsed_pointer(r), handle)))
                .collect();
            (archive, recent_results, result_scopes, pointers)
        };
        VolatileSnapshot {
            pending_emissions: self.pending_emissions.lock().clone(),
            entries,
            action_statements,
            last_action,
            action_fps: self.action_fps.lock().iter().cloned().collect(),
            receipt_heads: archive.iter().map(|(_, _, head)| head.clone()).collect(),
            receipt_head_rooms: archive.iter().map(|(_, room, _)| *room).collect(),
            receipt_head_seqs: archive.iter().map(|(seq, ..)| *seq).collect(),
            recent_results,
            result_scopes,
            next_action_seq: self.next_action_seq.load(Ordering::Relaxed),
            saved_at_ms: continuum_base::time::EpochMs::now().millis(),
            build_sha: env!("CONTINUUM_BUILD_GIT_SHA").to_string(),
            // Stamped by `save_volatile`, which knows WHOSE memory this is; the memory
            // itself carries no persona id.
            acting_card: None,
            handoff: None,
            interrupted_dispatches: self
                .dispatched
                .lock()
                .values()
                .filter(|d| d.status == DispatchStatus::Running)
                .map(|d| d.label.clone())
                .collect(),
            evicted_pointers: pointers.iter().map(|(_, pointer, handle)| (pointer.clone(), handle.clone())).collect(),
            evicted_pointer_seqs: pointers.iter().map(|(seq, ..)| *seq).collect(),
            receipt_children: self.receipt_children.lock().clone(),
            open_views: self.open_views.lock().saved(),
        }
    }

    /// Restore a snapshot into this (fresh) working memory at spawn — she wakes
    /// mid-thought instead of blank. Capacity re-clamps on the way in so a
    /// snapshot from a larger-capacity life never overflows this one.
    ///
    /// The checkpoint becomes a PERCEIVABLE fact: its save age and the dispatch
    /// labels it recorded as pending. It does not establish when an interruption
    /// happened, whether those operations completed, or what side effects they
    /// produced. The persona decides what to do with that uncertainty.
    pub fn restore(&self, mut snap: VolatileSnapshot) {
        // Read before any field moves out of the snapshot below.
        let records = snap.action_records();
        // A restored mind keeps only the newest concern evidence its existing
        // WM capacity admits. Older abandoned concerns cannot become eternal peaks.
        let mut pending = Vec::<ConcernEmission>::new();
        for mut emission in snap.pending_emissions {
            if let Some(index) = pending.iter().position(|e| e.room == emission.room && e.scope == emission.scope) {
                emission.peak_tokens = emission.peak_tokens.max(pending.remove(index).peak_tokens);
            }
            pending.push(emission);
            if pending.len() > self.capacity { pending.remove(0); }
        }
        *self.pending_emissions.lock() = pending;
        // Reconstitute the same ring before applying its existing capacity and
        // notice filtering. Older readers ignore the additive authored field.
        for (index, entry) in snap.action_statements {
            if matches!(entry.kind, WmKind::ActionStatement { .. }) {
                snap.entries.insert(index.min(snap.entries.len()), entry);
            }
        }
        {
            let mut e = self.entries.lock();
            e.clear();
            // A notice about a MOMENT is not memory: checkpoints written before
            // 2026-09-16 carried the previous restore's `[resumed]`/`[rebuilt]` lines as
            // entries, so every later restore re-loaded them and Kira's window said
            // "restored from a checkpoint saved under a minute ago … rebuilt 480243761 →
            // 374144c34" on every turn for five hours. The notices are pinned for two
            // turns below; any that a snapshot still carries are dropped here.
            e.extend(snap.entries.into_iter().filter(|w| {
                !(w.kind == WmKind::Fact
                    && (w.text.starts_with("[resumed]") || w.text.starts_with("[rebuilt]")))
            }));
            while e.len() > self.capacity {
                e.pop_front();
            }
        }
        // Same re-clamp contract as `entries`: the log bounds itself to THIS life's share, and
        // the collapse below folds results a bigger-window life held into pointers, never a
        // silent deletion.
        *self.actions.lock() =
            action_log::ActionLog::from_records(records, self.budget().receipt_archive_chars());
        *self.last_action.lock() = snap.last_action;
        *self.receipt_children.lock() = snap.receipt_children;
        *self.open_views.lock() = super::super::open_views::OpenViews::from_saved(snap.open_views);
        // Against the REAL window only; an unknown one folds nothing (the rule lives in
        // `collapse_results_over_cap`): a floor here folded results the served window holds,
        // cap 1,536 measured at restore on the 5090, card df0035bf.
        self.collapse_results_over_cap();
        self.publish_live_handles();
        {
            let mut f = self.action_fps.lock();
            f.clear();
            f.extend(snap.action_fps);
            while f.len() > self.capacity {
                f.pop_front();
            }
        }
        self.next_action_seq
            .store(snap.next_action_seq.max(1), Ordering::Relaxed);
        *self.restored_acting_card.lock() = snap.acting_card;

        // THE HANDOFF FIRST (card 49b5e806): pins render in pin order, and the record
        // that names what she was doing, holding, and owing outranks the notices about
        // the checkpoint's age and the rebuild. Same two-turn life as [resumed]: the
        // wake's turn and the one that acts on it. The build compared is the one that
        // BOOTED, so a deploy that failed to land is said as such (Kimi's #3).
        if let Some(handoff) = snap.handoff.as_ref() {
            let now_ms = continuum_base::time::EpochMs::now().millis();
            self.pin_fact_for_turns(
                "handoff",
                &crate::cognition::handoff::render_on_wake(
                    handoff,
                    env!("CONTINUUM_BUILD_GIT_SHA"),
                    now_ms,
                ),
                2,
            );
        }

        // Render the checkpoint evidence AFTER the entries land, so it is the
        // NEWEST thing in her window when she wakes. A fact, never an instruction.
        let checkpoint_age = (snap.saved_at_ms > 0)
            .then(|| {
                continuum_base::time::EpochMs::read().map(|now| now.millis().saturating_sub(snap.saved_at_ms)).ok()
            })
            .flatten()
            .map(|ms| {
                let mins = ms / 60_000;
                if mins == 0 {
                    "under a minute".to_string()
                } else {
                    format!("~{mins} min")
                }
            });
        let saved = match checkpoint_age {
            Some(age) => format!("saved {age} ago"),
            None => "with an unknown save time".to_string(),
        };
        let pending = if snap.interrupted_dispatches.is_empty() {
            crate::cognition::framing_echo::RESUMED_NO_PENDING.to_string()
        } else {
            format!(
                "Dispatches recorded as pending at that save: {}. Their completion and side effects are unknown.",
                snap.interrupted_dispatches.join("; ")
            )
        };
        // A TURN-SCOPED pin, never a ring entry: the checkpoint's age and its pending
        // dispatches are true at the wake and stale by the next act; as an entry the
        // line outlived the whole life (and was re-checkpointed into the next one).
        // Two turns, like [released]: the wake's turn and the one that acts on it.
        self.pin_fact_for_turns(
            "resumed",
            &format!("[resumed] your memory was restored from a checkpoint {saved}. {pending}"),
            2,
        );

        // The OTHER discontinuity, and until 2026-08-07 an invisible one: the
        // substrate itself was rebuilt while she was away (#165).
        //
        // Her receipts are TRUE — she really did get that result — but each one
        // describes how ONE binary behaved. Measured live: after the #326
        // adapter fix, `work/list(claimable=true)` began returning real cards
        // and ZERO authorization refusals occurred, while citizens went right on
        // reporting "attempting to claim tasks or list them without success",
        // because the refusal was still the newest thing in the window. Every
        // fix we ship is invisible to the beings it was for until their memory
        // of the old world ages out — and every measurement taken in that
        // period is contaminated.
        //
        // A FACT, never an instruction, and never a deletion: she keeps the
        // memory (proprioception depends on it) and gains the one thing she had
        // no way to know — that a result recorded before the rebuild is not
        // evidence about the system she is talking to now. What to re-check is
        // hers to decide.
        let current = env!("CONTINUUM_BUILD_GIT_SHA");
        if !snap.build_sha.is_empty() && snap.build_sha != current {
            let short = |s: &str| s.chars().take(9).collect::<String>();
            // Same lifetime as [resumed]: the rebuild is news at the wake, not a
            // standing condition — after two turns her receipts on THIS build exist.
            self.pin_fact_for_turns("rebuilt", &format!(
                "[rebuilt] the substrate was rebuilt while you were away ({} → {}). Your \
                 workspace, its checkout, and every file you read are UNCHANGED by that — what \
                 you learned about the code still stands; resume from your last conclusion, do \
                 not re-read to check. Only the TOOLS may behave differently now: a tool that \
                 failed before may work on this build, worth re-trying rather than concluding \
                 from memory.",
                short(&snap.build_sha),
                short(current),
            ), 2);
        }
    }

    /// The card the restored checkpoint believed she held, taken ONCE: the first held-card
    /// turn after a wake reads the board and answers it. `None` after that, and on a wake
    /// whose snapshot rooted nowhere.
    pub fn take_restored_acting_card(&self) -> Option<uuid::Uuid> {
        self.restored_acting_card.lock().take()
    }

    /// The same belief WITHOUT consuming it — for a read that could not reach a verdict
    /// (the board did not show the card) and must ask again next turn. Consumed only by
    /// [`Self::take_restored_acting_card`] once the board has answered.
    pub fn peek_restored_acting_card(&self) -> Option<uuid::Uuid> {
        *self.restored_acting_card.lock()
    }

    /// THE EVENT KIMI ASKED FOR FIRST (card c8303c32, 2026-09-21): her checkpoint restored
    /// a workspace rooted at a card, and the board no longer counts the card as hers. Until
    /// now the only signal was a submit refusal, hours later — "a checkpoint can restore me
    /// with a workspace and no claim, and right now nothing tells me those two disagree."
    /// A fact into her window, pinned for two turns like `[released]`: the wake's turn and
    /// the one that decides what to pull. The verdict is the BOARD'S (see [`ClaimGone`]),
    /// never inferred from an owner field — an expired claim still names its last holder,
    /// and a card in Review is absent from her live claims for a reason that is not a lapse.
    pub fn note_claim_gone(&self, card: uuid::Uuid, verdict: ClaimGone) {
        let id8: String = card.simple().to_string().chars().take(8).collect();
        let edge = |ms: Option<u64>| {
            ms.and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
                .map(|t| format!(" (lease edge {}Z)", t.format("%H:%M")))
                .unwrap_or_default()
        };
        let untouched = "Your checkout and your patch are untouched.";
        let text = match verdict {
            // Each sentence is what the board SHOWS NOW — never a story about how it got
            // there (the wake did not watch the lease lapse or the transfer happen).
            ClaimGone::HeldBy { peer, expires_at_ms } => format!(
                "[claim] card {id8} is NO LONGER YOURS: your checkpoint had you working it, and \
                 the board now shows {} holding it{}. {untouched} Do not resume as its holder — \
                 work/get {card} shows its state; take it back with work/claim only when it \
                 reads claimable.",
                peer.simple().to_string().chars().take(8).collect::<String>(),
                edge(expires_at_ms),
            ),
            ClaimGone::Lapsed { expired_at_ms } => format!(
                "[claim] card {id8} is NO LONGER YOURS: your checkpoint had you working it, and \
                 the board now shows it unheld — the last lease on it ran out{} and nobody holds \
                 it. {untouched} work/claim {card} takes it back, then continue from your ledger.",
                edge(expired_at_ms),
            ),
            ClaimGone::MovedOn { state } => format!(
                "[claim] card {id8} is no longer in your hands: the board now shows it in {state} \
                 — that is the card's life advancing, not a lapse. {untouched} Nothing to \
                 re-claim; work/get {card} shows where it stands."
            ),
            ClaimGone::Unknown => format!(
                "[claim] your checkpoint had you working card {id8}, but no board you stand in \
                 shows it now, so nothing can be said about who holds it. {untouched} \
                 work/get {card} before assuming it is yours or free."
            ),
        };
        self.pin_fact_for_turns("claim", &text, 2);
    }

}
