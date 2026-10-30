//! `WorkingMemory` readers: what the mind perceives, dispatch events, collapse and settlement.

use super::*;

/// One act as her view renders it: seq, full, summary, pointer, and its stored whole (rendered
/// chars, handle) when its body is not resident.
/// One act as her view renders it: who it is (its `seq` in her ledger, the room it was for), its
/// levels, and where its whole is stored when it left RAM.
#[derive(Clone, Debug)]
pub(crate) struct ActRendering {
    pub seq: u64,
    /// The room the act was for; acts in one room are one chain.
    pub room: Option<uuid::Uuid>,
    pub full: String,
    pub summary: Option<String>,
    pub pointer: String,
    /// `(chars, handle)` of a whole that left RAM for its saved output.
    pub stored: Option<(usize, String)>,
    /// The hash of the act's RAW result bytes, before any label: two identical outcomes hash
    /// equal whatever their seq or room (`Absent` when the result is not resident).
    pub content: continuum_context::cause::Content,
}

impl WorkingMemory {
    /// Typed snapshot of the window, oldest → newest — the kind-aware sibling
    /// of [`Self::recent`] for consumers that render BY KIND (the steps-taken
    /// ledger renders `Receipt`s as numbered steps and `Fact`s under
    /// [notices]; docs/architecture/PERCEPTION-FACTS.md).
    pub fn recent_entries(&self) -> Vec<WmEntry> {
        let scope = self.scope.lock().clone();
        self.entries
            .lock()
            .iter()
            .filter(|e| scope_visible(e.scope.as_deref(), scope.as_deref()))
            .cloned()
            .collect()
    }

    /// Where her hands stand now. Receipts recorded from here on carry it, and
    /// receipts from another root leave perception until she returns there.
    pub fn set_scope(&self, root: Option<String>) {
        *self.scope.lock() = root;
    }

    /// How many acts have executed this session (survives reboots via the
    /// volatile snapshot's `next_action_seq`). The steps-taken ledger uses
    /// this to keep its zero-case HONEST: receipts are rare entries in a
    /// chatty capacity-bounded ring, so they age out — "no receipts in the
    /// window" must never be rendered as "nothing has executed" when this
    /// counter says otherwise (glass-boxed 2026-07-13: Asha's window held
    /// 3 silence Facts and zero Receipts minutes after real searches ran).
    pub fn actions_taken(&self) -> u64 {
        self.next_action_seq
            .load(Ordering::Relaxed)
            .saturating_sub(1)
    }

    /// TRUE if any entry in the window is a real tool receipt — the kind
    /// query that replaces string-scanning rendered text for `[action #`.
    pub fn has_receipt(&self) -> bool {
        self.entries
            .lock()
            .iter()
            .any(|e| matches!(e.kind, WmKind::Receipt { .. }))
    }

    /// The FULL result of the most recent act, with its `#seq` stamp — `None` before any
    /// act or after a `clear`. The perception faculty surfaces this so the mind sees the
    /// whole of what its hands just fetched, not the truncated trail head.
    pub fn last_action_full(&self) -> Option<(u64, String)> {
        self.last_action.lock().clone()
    }

    /// The full most-recent-act result ONLY while it is still ACTIVE — the mind is
    /// inside the act→observe loop that produced it and hasn't spoken yet. Returns `None`
    /// once the persona has SETTLED past it (the raw result did its "what next" job; the
    /// trail head, episodic memory, and re-fetch remain). This is what the perception
    /// faculty surfaces so a 2k-token `code/tree` dump stops re-prefilling on every
    /// conversational turn AFTER the mind already used it (#139 prefill / #165 stale
    /// replay). `last_action_full` stays the ungated accessor for callers that want the
    /// raw slot regardless of settlement (snapshotting, tests).
    /// The unsettled last action's sequence, whatever workspace she is rooted in now.
    fn in_flight_action_seq(&self) -> Option<u64> {
        self.last_action.lock().as_ref().map(|(seq, _)| *seq).filter(|s| *s >= self.active_from_seq.load(Ordering::Relaxed))
    }

    pub fn active_action_full(&self) -> Option<(u64, String)> {
        let actions = self.actions.lock();
        let action = self.last_action.lock().clone()?;
        let active_from = self.active_from_seq.load(Ordering::Relaxed);
        let scope = self.scope.lock().clone();
        let recorded = actions.get(action.0).and_then(|record| record.scope.as_deref());
        let visible = scope_visible(recorded, scope.as_deref());
        (action.0 >= active_from && visible).then_some(action)
    }

    /// Typed results for the exact active batch whose text the caller is rendering.
    /// Unlike `active_act`, this retains every call (including multiple images)
    /// and cannot replay evidence from a settled, replaced or different workspace.
    pub fn active_action_observations(&self, seq: u64) -> Vec<Observation> {
        if self.last_action.lock().as_ref().map(|(n, _)| *n) != Some(seq)
            || seq < self.active_from_seq.load(Ordering::Relaxed)
        {
            return Vec::new();
        }
        let scope = self.scope.lock().clone();
        self.entries
            .lock()
            .iter()
            .rev()
            .find(|entry| {
                matches!(entry.kind, WmKind::Receipt { n } if n == seq)
                    && entry.scope == scope
            })
            .map(|entry| entry.acts.clone())
            .unwrap_or_default() // No matching scoped receipt means no typed observations are eligible.
    }

    /// Original native batch for this exact active receipt, if captured. An empty
    /// result is legacy evidence, never permission to reverse-guess tool aliases.
    pub fn active_action_native_calls(&self, seq: u64) -> Vec<crate::ai::types::ToolCall> {
        if self.last_action.lock().as_ref().map(|(n, _)| *n) != Some(seq)
            || seq < self.active_from_seq.load(Ordering::Relaxed) { return Vec::new(); }
        let scope = self.scope.lock().clone();
        self.entries.lock().iter().rev()
            .find(|entry| entry.scope == scope && matches!(entry.kind, WmKind::Receipt { n } if n == seq))
            .map(|entry| entry.native_calls.clone()).unwrap_or_default()
    }

    /// Render the complete active payload with its recorded provenance. The
    /// caller retains the same raw bytes for request identity; this introduces
    /// no second payload clone, serialization, or inferred current-room label.
    pub(crate) fn full_result_message(&self, seq: u64, full: &str) -> String {
        let actions = self.actions.lock();
        let recorded = actions.get(seq);
        let attribution = ResultAttribution {
            seq,
            room: recorded.and_then(|record| record.room),
            operation: recorded.map(|record| record.operation.as_str()),
            spill_handle: recorded.and_then(|record| record.handle.as_deref()),
        };
        format!("Full result of your most recent action ({attribution}):\n{full}")
    }

    /// The handle the saved whole of act `seq` is reopened by (`tool/output`, by handle or by
    /// `act`): the one resolution of an act number to its saved output. A refusal when no
    /// record carries `seq`, or when its save failed: such a result has no other copy, so it
    /// is never shown as less than whole.
    pub fn saved_handle(&self, seq: u64) -> Result<String, ActOutputRefusal> {
        let actions = self.actions.lock();
        let record = actions.get(seq).ok_or(ActOutputRefusal::Unknown { act: seq })?;
        record.handle.clone().ok_or(ActOutputRefusal::Unsaved { act: seq })
    }

    /// The provenance of the act whose native results the turn carries: `[result #n; room …;
    /// operation …; output handle …]`, the label every result of hers wears in her view.
    /// A label, not a header: rendered as "Full result of your most recent action (…):"
    /// with nothing after it, it read as someone asking her to present her results, and
    /// Aris answered it ("The user is asking me to present the complete results of my most
    /// recent action", "#1642 is already inline above", the M5, 2026-10-21).
    pub(crate) fn result_label(&self, seq: u64) -> String {
        let actions = self.actions.lock();
        let recorded = actions.get(seq);
        let attribution = ResultAttribution {
            seq,
            room: recorded.and_then(|record| record.room),
            operation: recorded.map(|record| record.operation.as_str()),
            spill_handle: recorded.and_then(|record| record.handle.as_deref()),
        };
        format!("[result {attribution}]")
    }

    /// The FULL result of the just-executed act, ready to render as a **pinned**
    /// trailing prompt block — the run-18057-f1 fix.
    ///
    /// This block used to live INSIDE the working-memory faculty's single
    /// [`Contribution`](crate::cognition::workspace::Contribution) at
    /// `WORKING_MEMORY_SALIENCE` (0.5). That bid competes in `arbiter.focus()`
    /// top-k, so under capacity pressure the whole contribution — the just-fetched
    /// grep/read result included — was truncated, and the persona generated blind
    /// to what her own hands had returned (the 0-byte SWE-bench patch). The message
    /// builder now reads this DIRECTLY and appends it as its own durable trailing
    /// turn, so no `focus()` pass stands between the hands and the mind.
    ///
    /// Identical gating to the old faculty block: settlement-gated via
    /// [`active_action_full`](Self::active_action_full) (stops re-prefilling once the
    /// turn settles — #139/#165), and shown only when the full result carries MORE
    /// than the trail head already does. `None` when there is no active result or it
    /// would add nothing.
    pub fn pinned_active_result_block(&self) -> Option<String> {
        let (seq, full) = self.active_action_full()?;
        if full.chars().count() <= self.budget().trail_head_chars() {
            return None;
        }
        // Shown as the producer handed it over: whole, or its handle. A consumer never
        // re-cuts (CONTENT-TRAVELS-BY-HANDLE.md, forbidden moves).
        Some(self.full_result_message(seq, &full))
    }

    /// The RECENT-RESULTS feedback block: the last few acts' result tails, kept
    /// across acts AND turn boundaries — the channel that lets a mind continue a
    /// multi-turn task instead of re-deriving it (the pinned block above is
    /// settlement-gated and the `last_action` slot is overwritten by any act, so
    /// without this a compile error vanished the moment she glanced at a file
    /// listing — Anwen, 2026-08-16, re-ran the same unfixed code three turns
    /// running). Deliberately NOT gated on settlement: the tails are small and
    /// append-only, so the KV prefix stays stable and the #139 full-dump prefill
    /// objection does not apply. `None` before any act.
    /// The recent-results channel as ONE MESSAGE PER RESULT — the KV layer-3
    /// shape (2026-08-23). As a single block, any eviction (pop_front under the
    /// char cap — every act once results are large) mutated the block's HEAD
    /// in place, re-prefilling everything after the burst: measured 75%→22%
    /// cache decay within one task, ~6.4k tokens of reuse against 29k prompts.
    /// As separate messages, eviction DROPS THE OLDEST MESSAGE whole and
    /// llama's --cache-reuse chunk-shifting re-aligns the surviving suffix
    /// instead of re-ingesting it. First message carries the banner so the
    /// semantics of the ledger stay stated.
    /// Her open views as one compact block, one line per view visible from where her hands
    /// stand ([`super::super::open_views::OpenViews::render`]). Rendered as a required notice
    /// (`perception_facts::OpenViews`), so the history fit never drops it.
    #[cfg(test)]
    pub fn open_views_block(&self) -> Option<String> {
        let scope = self.scope.lock().clone();
        self.open_views.lock().render(scope.as_deref())
    }

    /// Her open views visible from where her hands stand, one `(key, full, pointer)` each: the items of
    /// her view's open-views stream (`context_streams::open_views`).
    pub(crate) fn open_view_lines(&self) -> Vec<crate::cognition::open_views::ViewLine> {
        let scope = self.scope.lock().clone();
        self.open_views.lock().lines(scope.as_deref())
    }

    /// Keys of her open views her hands cannot see from where they stand now.
    pub(crate) fn open_views_out_of_scope(&self) -> Vec<String> {
        let scope = self.scope.lock().clone();
        self.open_views.lock().out_of_scope(scope.as_deref())
    }

    /// Every act she has a record of, oldest first, each in the renderings her context view
    /// chooses between: `(seq, full, summary, pointer)`, the summary its shape's level (`continuum_context::levels`). Full is the whole result while retained,
    /// else what its pointer says; the pointer names the handle that reopens it, or is the
    /// ledger line when nothing was saved. The act still in flight is not among them (it
    /// rides whole as the newest message of the turn); the flag says whether there is one.
    /// Each act as her view's actions source renders it: (seq, full, summary, pointer, stored).
    /// `stored` is the whole's rendered length and handle once its body left RAM for its
    /// handle (`evict_resident_results_over`, the memory owner's shed): `full` is then the
    /// pointer, and the view prices and loads the whole through the source (`continuum_context::StoredWhole`).
    pub(crate) fn action_renderings(&self) -> (Vec<ActRendering>, bool) {
        let active = self.in_flight_action_seq();
        let actions = self.actions.lock();
        let in_flight = active.is_some_and(|seq| actions.get(seq).is_some());
        let renderings = actions
            .records()
            .filter(|record| Some(record.seq) != active)
            .map(|record| {
                let attribution = ResultAttribution::of(record);
                let workspace = workspace_label(record.scope.as_deref());
                let whole = record.result.as_deref().map(|result| format!("[result {attribution}{workspace}] {result}"));
                let pointer = match (&record.handle, &whole, &record.head) {
                    // An older build's pointer, as it was written (its operation lives there).
                    (Some(_), _, _) if record.written_pointer.is_some() => record.written_pointer.clone().into_iter().collect(),
                    (Some(_), _, _) => format!("[result {attribution}{workspace} — collapsed; query tool/output to expand]"),
                    // Retained but never saved: no handle could reopen a collapsed line, so it stays
                    // whole (goal 7: the full result stays reachable; Cormac on #5255).
                    (None, Some(whole), _) => whole.clone(),
                    (None, None, Some(head)) => head.clone(),
                    // Unsaved and off the ledger: what it was, which never collapses anyway.
                    (None, None, None) => format!("[action #{}] {}", record.seq, record.operation),
                };
                let stored = match (&record.result, &record.handle, record.stored_chars) {
                    (None, Some(handle), Some(chars)) => {
                        let label = format!("[result {attribution}{workspace}] ").chars().count();
                        Some((label + chars, handle.clone()))
                    }
                    _ => None,
                };
                let full = whole.unwrap_or_else(|| pointer.clone());
                let summary = record.summary.as_deref().map(|summary| format!("[result {attribution}{workspace} — summary; expand with tool/output] {summary}"));
                let content = record.result.as_deref().map_or(continuum_context::cause::Content::Absent, |result| {
                    continuum_context::cause::Content::Hashed(continuum_context::cause::ContentHash::of(result))
                });
                ActRendering { seq: record.seq, room: record.room, full, summary, pointer, stored, content }
            })
            .collect();
        (renderings, in_flight)
    }

    /// The label an act's whole carries in her view (`[result #n; …] `), for a stored whole
    /// loaded back from its handle, so it reads exactly as it did resident.
    pub(crate) fn action_whole_label(&self, seq: u64) -> Option<String> {
        let actions = self.actions.lock();
        let record = actions.get(seq)?;
        Some(format!("[result {}{}] ", ResultAttribution::of(record), workspace_label(record.scope.as_deref())))
    }

    pub fn recent_results_messages(&self) -> Vec<String> {
        self.recent_results_with_pointers().into_iter().map(|(body, _)| body).collect()
    }

    /// Source-owned alternatives for prompt fitting: only a saved result may
    /// collapse to its expansion handle. The live ring and its custody stay intact.
    pub(crate) fn recent_results_with_pointers(&self) -> Vec<(String, Option<String>)> {
        // The active result renders WHOLE as the "Full result of your most recent action"
        // message; holding it here too would spend its share twice. It joins the ring's
        // tail when the next act replaces it: an append at the frontier, so the prefix
        // ahead of it stays byte-stable.
        // One mind: every result is shown from every workspace, labelled with its own.
        // The in-flight action is excluded by sequence, never by root: one prefix everywhere.
        let active = self.in_flight_action_seq();
        let actions = self.actions.lock();
        let eligible = |record: &&action_log::ActionRecord| Some(record.seq) != active;
        let visible_pointers: Vec<String> = actions.indexed_pointers().into_iter().map(collapsed_pointer).collect();
        if !actions.retained().any(|record| eligible(&record)) && visible_pointers.is_empty() {
            return Vec::new();
        }
        // Banner as its OWN byte-stable message (never a record, so a collapse can never
        // take it — gluing it to entry #1 would delete the ledger's semantics on the first
        // collapse).
        let mut out = Vec::new();
        out.push((
            "Recent results of your own actions (oldest first; #n matches the action ledger):"
                .to_string(), None,
        ));
        // Every eligible result rides as a message of its own: the ring's RAM is bounded by the
        // one memory owner's shed (`residency`), never by a count here.
        // A count fold ("newest 16 as messages, the rest as index lines", 2026-10-16) rotated
        // the head on EVERY act, which on a KV that cannot shift re-prefilled everything
        // after it; a shed happens only when RAM is short, so between sheds the ring appends.
        out.extend(actions.retained().filter(eligible).filter_map(|record| {
            let result = record.result.as_deref()?;
            let attribution = ResultAttribution::of(record);
            let workspace = workspace_label(record.scope.as_deref());
            let pointer = record.handle.as_ref().map(|_| format!("[result {attribution}{workspace} — collapsed; query tool/output to expand]"));
            Some((format!("[result {attribution}{workspace}] {result}"), pointer))
        }));
        // Collapsed-results index LAST — it CHANGES on every collapse, and message order is
        // MONOTONE IN STABILITY: the banner + surviving entries ahead of it stay a
        // byte-stable prefix; only this one small message re-prefills.
        if !visible_pointers.is_empty() {
            out.push((format!(
                "Older results, collapsed to pointers (query to expand):\n{}",
                visible_pointers.join("\n")
            ), None));
        }
        out
    }

    /// Fold a streamed event from a DISPATCHED (background) command into the mind's
    /// recency, keyed by its handle. The async twin of `record_action`: a sentinel /
    /// compile / debugger the persona sent away emits events CONTINUOUSLY — `Running`
    /// progress updates the handle's slot IN PLACE (no trail spam), then a terminal
    /// `Done`/`Failed` marks it. The mind perceives outstanding + freshly-finished handles
    /// via the faculty next heartbeat, so it can act on a result the moment it lands
    /// without ever blocking. `label` is set once (first event) and preserved across
    /// updates. Bounded by `MAX_DISPATCHED` — a finished handle is evicted before a
    /// running one when over cap ([[act-results-need-a-recency-channel-not-semantic-recall]]).
    pub fn record_dispatch_event(
        &self,
        handle: Uuid,
        label: &str,
        content: &str,
        status: DispatchStatus,
    ) {
        let seq = self.next_action_seq.fetch_add(1, Ordering::Relaxed);
        let mut m = self.dispatched.lock();
        let entry = m.entry(handle).or_insert_with(|| DispatchedAction {
            label: label.trim().to_string(),
            latest: String::new(),
            status: DispatchStatus::Running,
            seq,
        });
        entry.latest = content.trim().to_string();
        entry.status = status;
        entry.seq = seq;
        if m.len() > MAX_DISPATCHED {
            // Evict the oldest-updated FINISHED handle; never drop one still running.
            if let Some(victim) = m
                .iter()
                .filter(|(_, a)| a.status != DispatchStatus::Running)
                .min_by_key(|(_, a)| a.seq)
                .map(|(h, _)| *h)
            {
                m.remove(&victim);
            }
        }
    }

    /// The label of a dispatched handle THIS persona owns, or `None` if the handle isn't
    /// ours (never dispatched here, or already evicted). The async-dispatch listener uses
    /// it to claim ONLY its own completions off the shared `command:completed` bus — other
    /// clients' handles, and this persona's synchronous commands, are ignored.
    pub fn dispatched_label(&self, handle: Uuid) -> Option<String> {
        self.dispatched.lock().get(&handle).map(|a| a.label.clone())
    }

    /// Snapshot of dispatched (background) commands — `(handle, label, latest, status)`,
    /// most-recently-updated last. The faculty renders these so the mind sees what it sent
    /// away and where each stands; the handle is reusable in a follow-up command.
    pub fn dispatched_snapshot(&self) -> Vec<(Uuid, String, String, DispatchStatus)> {
        let m = self.dispatched.lock();
        let mut v: Vec<_> = m
            .iter()
            .map(|(h, a)| {
                (
                    *h,
                    a.label.clone(),
                    a.latest.clone(),
                    a.status.clone(),
                    a.seq,
                )
            })
            .collect();
        v.sort_by_key(|t| t.4);
        v.into_iter()
            .map(|(h, l, latest, s, _)| (h, l, latest, s))
            .collect()
    }

    /// Tell the spill owner which saved results this mind still points at (ring entries and
    /// collapsed pointers), so disk-pressure eviction never deletes one (card 9197cd01).
    /// The whole set, replaced, so nothing can drift.
    pub(super) fn publish_live_handles(&self) {
        let receipts = self.actions.lock().receipt_keys();
        let mut children = self.receipt_children.lock();
        // Dependencies live exactly as long as their receipt: drop those of receipts that
        // are neither retained nor indexed as a pointer.
        children.retain(|key, _| receipts.contains(key));
        let views = self.open_views.lock().saved();
        let handles = reachable_handles(receipts, &children, &views);
        drop(children);
        crate::cognition::tool_executor::spill::publish_live_handles(self.instance, handles);
    }

    /// Record that the persona SETTLED — produced an utterance and closed the current
    /// concern. Pushes a [`WM_SETTLEMENT_PREFIX`]-marked boundary into the same rolling
    /// buffer (honest proprioception: "I already answered X", perceivable by the mind
    /// next tick). The boundary lets the repeat-perception guard distinguish a spin
    /// (identical call re-issued WITHIN one settling, before any answer) from a
    /// legitimate re-use of the same tool for a genuinely NEW concern after an answer.
    /// Blank is ignored. Oldest ages out past capacity, same rolling scratchpad.
    pub fn record_settlement(&self, answer_head: &str, room: Option<Uuid>) {
        let a = answer_head.trim();
        let scope = self.scope.lock().clone();
        // Existing authored-settlement boundary, not proof the task was completed.
        // An unknown or different activity cannot discharge this concern's evidence.
        self.pending_emissions.lock().retain(|e| Some(e.room) != room || e.scope != scope);
        let mut e = self.entries.lock();
        // End only the authored decision for this activity and exact root. An
        // unknown room cannot establish that boundary. The existing unscoped
        // settlement marker keeps its historical semantics.
        e.retain(|entry| {
            entry.scope != scope
                || !matches!(entry.kind, WmKind::ActionStatement { room: authored_room } if Some(authored_room) == room)
        });
        e.push_back(WmEntry {
            disk_change: Default::default(),
            kind: WmKind::Settlement,
            text: if a.is_empty() {
                WM_SETTLEMENT_PREFIX.to_string()
            } else {
                format!("{WM_SETTLEMENT_PREFIX} {a}")
            },
            scope: None,
            acts: Vec::new(),
            native_calls: Vec::new(),
        });
        while e.len() > self.capacity {
            e.pop_front();
        }
        drop(e);
        // Settling closes over the current act→observe loop: the most recent action's
        // FULL result has done its "what next" job. Advance the active boundary PAST it
        // so the full block stops riding along subsequent conversational turns (#139/#165
        // stale-result-replay). A later act re-activates by minting a higher seq. Nothing
        // acted yet (last_action None) → nothing to close. `fetch_max` so it never regresses.
        if let Some((seq, _)) = self.last_action.lock().as_ref() {
            let next_active = seq.saturating_add(1);
            self.active_from_seq
                .fetch_max(next_active, Ordering::Relaxed);
        }
    }

    /// Snapshot of recent reasonings, oldest → newest.
    pub fn recent(&self) -> Vec<String> {
        self.entries.lock().iter().map(|e| e.text.clone()).collect()
    }

    /// Drop all volatile traces — the scratchpad goes dark. Used at the boundary
    /// between genuinely disjoint concerns (e.g. each independent task in a
    /// `cognition/eval` pass, presented back-to-back by a grader with no temporal
    /// continuity): carrying the prior concern's proprioception into the next would
    /// be contamination, not memory. The monotonic action stamp is NOT reset, so
    /// stamps never collide across a clear (a stale reference can't alias a fresh
    /// act). The live heartbeat never calls this — there, concerns flow
    /// continuously and old traces age out naturally past `capacity`.
    pub fn clear(&self) {
        self.pending_emissions.lock().clear();
        self.entries.lock().clear();
        *self.last_action.lock() = None; // the full latest result is proprioception too
        self.dispatched.lock().clear(); // dispatched handles belong to the cleared concern
        self.action_fps.lock().clear(); // loop-awareness resets with the concern
        self.unpin_fact("recent-action-distribution");
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}
