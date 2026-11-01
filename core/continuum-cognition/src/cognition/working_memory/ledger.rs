//! `WorkingMemory` construction, action statements, fingerprints and the receipt ledger.

use super::*;

impl WorkingMemory {
    /// Preserve the public decision separately from private reasoning and tool output.
    /// It uses the existing bounded, checkpointed entry stream and acting-root scope.
    pub fn record_action_statement(&self, text: &str, room: Uuid) {
        if !text.trim().is_empty() {
            self.record_native_action_statement(text, room, &[]);
        }
    }

    pub fn record_native_action_statement(&self, text: &str, room: Uuid, calls: &[crate::ai::types::ToolCall]) {
        if room.is_nil() {
            return;
        }
        let scope = self.scope.lock().clone();
        let mut entries = self.entries.lock();
        self.push_entry(&mut entries, WmEntry {
            disk_change: Default::default(),
            kind: WmKind::ActionStatement { room },
            text: text.to_owned(),
            acts: Vec::new(),
            native_calls: calls.to_vec(),
            scope,
            occurrence: 0,
        });
    }

    /// The latest authored decision in this exact activity/root, until settlement.
    /// The prompt owner prices the whole statement with required action feedback;
    /// attention's optional working-memory bid cannot silently remove it.
    pub fn action_statement(&self, room: Uuid) -> Option<String> {
        if room.is_nil() {
            return None;
        }
        let scope = self.scope.lock().clone();
        self.entries
            .lock()
            .iter()
            .rev()
            .filter(|entry| entry.scope == scope)
            .find(|entry| matches!(entry.kind, WmKind::ActionStatement { room: authored_room } if authored_room == room))
            .and_then(|entry| (!entry.text.trim().is_empty()).then(|| entry.text.clone()))
    }

    pub fn new(capacity: usize) -> Self {
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            capacity: capacity.max(1),
            pinned: Mutex::new(Vec::new()),
            ended_pins: Mutex::new(Vec::new()),
            restored_acting_card: Mutex::new(None),
            entries: Mutex::new(VecDeque::new()),
            next_entry_seq: AtomicU64::new(1),
            scope: Mutex::new(None),
            pending_emissions: Mutex::new(Vec::new()),
            last_action: Mutex::new(None),
            dispatched: Mutex::new(HashMap::new()),
            next_action_seq: AtomicU64::new(1),
            action_fps: Mutex::new(VecDeque::new()),
            action_fp_counts: Mutex::new(HashMap::new()),
            actions: Mutex::new(Default::default()),
            view: Mutex::new(None),
            receipt_children: Mutex::new(BTreeMap::new()),
            active_from_seq: AtomicU64::new(0),
            open_views: Mutex::new(Default::default()),
            task_results: Mutex::new(Default::default()),
            pending_perception: Mutex::new(Default::default()),
            incarnation: Uuid::new_v4(),
            tool_head: Default::default(),
        }
    }

    /// Her learned tool head.
    pub fn tool_head(&self) -> &Arc<super::super::tool_head::ToolHead> {
        &self.tool_head
    }

    /// Keep the existing measured truncation peak across intermediate Act replies.
    /// Switching activity/root does not transfer this evidence to another concern.
    pub fn retain_truncated_emission(&self, room: Uuid, peak_tokens: u32) {
        let scope = self.scope.lock().clone();
        let mut pending = self.pending_emissions.lock();
        let peak_tokens = if let Some(index) = pending.iter().position(|e| e.room == room && e.scope == scope) {
            pending.remove(index).peak_tokens.max(peak_tokens)
        } else { peak_tokens };
        pending.push(ConcernEmission { room, scope, peak_tokens });
        if pending.len() > self.capacity {
            let discarded = pending.len() - self.capacity;
            drop(pending.drain(..discarded));
        }
    }

    pub(crate) fn pending_emission_peak(&self, room: Uuid) -> u32 {
        let scope = self.scope.lock().clone();
        self.pending_emissions.lock().iter()
            .filter(|e| e.room == room && e.scope == scope)
            .map(|e| e.peak_tokens).max().unwrap_or(0)
    }

    /// Record a fingerprint (tool name + args) of the action the hands just took, and return
    /// how many times THIS exact call now appears in the recent window (including this one).
    /// `≥ 2` means the mind is re-issuing an identical call — proprioception the act→observe
    /// step renders EXPLICITLY so a looping mind perceives its own redundancy and moves on.
    /// Reports a TRUE fact about her hands; it never dictates what to do instead (that would
    /// be steering — [[no-hardcoded-heuristics-to-steer-cognition]]). The returned count is
    /// DURABLE across the spawn (see `action_fp_counts`) so a loop that recurs amid other
    /// acts escalates honestly instead of pinning at the tiny recency-window size.
    pub fn note_action_fingerprint(&self, fingerprint: &str) -> usize {
        // Windowed push feeds `action_verb_tally` (the RECENT investigation shape) — that
        // one WANTS the rolling `capacity` window, so leave it bounded here.
        {
            let mut fps = self.action_fps.lock();
            fps.push_back(fingerprint.to_string());
            while fps.len() > self.capacity {
                fps.pop_front();
            }
        }
        // The repeat COUNT is durable across the spawn — NOT clipped to the tiny recency
        // window — so an identical call interleaved with other acts still climbs 3→38
        // rather than resetting every time it falls out of the `capacity`-deep window.
        let mut counts = self.action_fp_counts.lock();
        let c = counts.entry(fingerprint.to_string()).or_insert(0);
        *c += 1;
        *c
    }

    /// Tally of recent actions BY TOOL NAME (the part of the fingerprint before
    /// `|`), most-used first — the mind's own act distribution as a structural
    /// fact. Where `note_action_fingerprint` perceives "this EXACT call again",
    /// this perceives the SHAPE of the whole investigation ("9 acts so far, all
    /// code/search") — glass-boxed on SWE flask-4045 (2026-07-11): a 24B with a
    /// per-file result menu in view still re-searched 6×; the imbalance itself
    /// was never a perceivable fact. A tally is truth about her own hands,
    /// never a directive. Same rolling window as the fingerprints.
    pub fn action_verb_tally(&self) -> Vec<(String, usize)> {
        let fps = self.action_fps.lock();
        let mut tally: Vec<(String, usize)> = Vec::new();
        for fp in fps.iter() {
            let name = fp.split('|').next().unwrap_or(fp).to_string();
            match tally.iter_mut().find(|(n, _)| *n == name) {
                Some((_, c)) => *c += 1,
                None => tally.push((name, 1)),
            }
        }
        tally.sort_by(|a, b| b.1.cmp(&a.1));
        tally
    }

    /// Current distribution of retained action fingerprints, not a lifetime total.
    /// Replace the keyed status so old distributions do not compete as historical facts.
    pub(crate) fn refresh_recent_action_status(&self) {
        let tally = self.action_verb_tally();
        let retained: usize = tally.iter().map(|(_, count)| count).sum();
        if retained < 3 {
            self.unpin_fact("recent-action-distribution");
            return;
        }
        let distribution = tally.iter()
            .map(|(name, count)| format!("{name} ×{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.pin_fact("recent-action-distribution", &format!(
            "[investigation] Among my {retained} most recent retained calls: {distribution}. Older calls may have aged out."
        ));
    }

    /// Record the reasoning the persona just produced. Blank/empty is ignored
    /// (suppressed-thinking turns record nothing). Oldest ages out past capacity.
    /// Record `entry` in her ring: stamped with the next occurrence (the one owner of entry
    /// identity), the oldest leaving past capacity.
    pub(super) fn push_entry(&self, entries: &mut VecDeque<WmEntry>, mut entry: WmEntry) {
        entry.occurrence = self.next_entry_seq.fetch_add(1, Ordering::Relaxed);
        entries.push_back(entry);
        while entries.len() > self.capacity {
            entries.pop_front();
        }
    }

    pub fn record(&self, reasoning: &str) {
        let r = reasoning.trim();
        if r.is_empty() {
            return;
        }
        let mut e = self.entries.lock();
        self.push_entry(&mut e, WmEntry {
            disk_change: Default::default(),
            kind: WmKind::Thought,
            text: r.to_string(),
            acts: Vec::new(),
            native_calls: Vec::new(),
            scope: None,
            occurrence: 0,
        });
    }

    /// Record an ACTION the persona's hands just took (a tool call + its result
    /// head) — proprioception, NOT chain-of-thought. This fires regardless of the
    /// thinking toggle: an act HAPPENED whether or not the model emitted `<think>`,
    /// and the mind must perceive its own hands to avoid re-issuing the identical
    /// act blind ([[act-results-need-a-recency-channel-not-semantic-recall]]). Each
    /// entry is stamped with a monotonic `#n` so a repeated identical act still
    /// changes the perception window (see `next_action_seq`). Oldest ages out past
    /// capacity, same rolling scratchpad as reasoning.
    pub fn record_receipt(&self, result: &str) {
        self.record_receipt_inner(result, Vec::new(), None, None, Default::default());
    }

    /// TYPED sibling of [`record_receipt`](Self::record_receipt): store the same
    /// byte-identical rendered receipt AND the typed [`Observation`]s the batch
    /// produced, so a consumer reads `recent_acts()`/`active_act()` instead of
    /// re-parsing the `[action #n]` prose (run-18057-f1). `rendered` is the recency
    /// string produced ONCE at the act seam (`render_recency`) — this method does NOT
    /// re-render from `acts` (that would break the #205 KV byte-stability invariant:
    /// `text` is the stable source of truth, `acts` the parallel typed channel).
    /// `room` scopes the archived head to the room the act was FOR (the act
    /// path passes it; legacy text callers don't have it) — the steps ledger
    /// renders this-room acts first (CAUSAL-MEMORY-GRAPH.md slice B).
    /// `custody` is where `rendered` itself is saved (`spill::LivePin::retain`): the handle the results
    /// ring's pointer names when this entry collapses. `Unsaved` entries never collapse.
    pub fn record_receipt_typed(
        &self,
        acts: &[Observation],
        rendered: &str,
        room: Option<Uuid>,
        custody: crate::ai::types::ResultCustody,
    ) {
        self.record_receipt_measured(acts, rendered, room, custody, Default::default());
    }

    /// Attach her stated reasoning for the act just recorded to the open views it touched,
    /// so each view carries what she intended, not only how far she read. Whole, never a first
    /// line: it is an optional level of the open-views source, which her view sizes and drops
    /// before the cursor (`planning_detail_yields_to_cursor_and_returns_when_granted`).
    pub fn annotate_open_views(&self, intent: &str) {
        let Some(seq) = self.next_action_seq.load(Ordering::Relaxed).checked_sub(1) else {
            return;
        };
        self.open_views.lock().annotate(seq, intent);
    }

    pub(crate) fn record_receipt_measured(
        &self,
        acts: &[Observation],
        rendered: &str,
        room: Option<Uuid>,
        custody: crate::ai::types::ResultCustody,
        disk_change: crate::cognition::act_observe::disk_change::DiskChange,
    ) {
        self.record_receipt_inner(rendered, acts.to_vec(), room, custody.handle().map(str::to_owned), disk_change);
    }

    /// Shared body: mint the monotonic `#seq`, keep the FULL latest result, and push
    /// ONE `[action #seq]`-stamped receipt entry carrying the pre-rendered head text
    /// plus the (possibly empty) typed acts. One entry per batch so `recent()` stays
    /// byte-stable (a multi-call batch renders as one receipt exactly as before);
    /// `acts` carries every call's typed observation for the typed consumers.
    fn record_receipt_inner(
        &self,
        result: &str,
        acts: Vec<Observation>,
        room: Option<Uuid>,
        handle: Option<String>,
        disk_change: crate::cognition::act_observe::disk_change::DiskChange,
    ) {
        let a = result.trim();
        if a.is_empty() {
            return;
        }
        let seq = self.next_action_seq.fetch_add(1, Ordering::Relaxed);
        let scope = self.scope.lock().clone();
        // Her cursors over large data move with the acts that paged it (published as live
        // handles below, with the ring's).
        self.open_views.lock().note_acts(&acts, seq, scope.as_deref());
        // Keep the LATEST result in FULL so the mind can work with it next turn (count a
        // file, read a screenshot description, scan a log). Overwrites the prior latest —
        // older acts survive only as heads in the trail below.
        // Recent-results feedback channel: keep this result WHOLE past the next act and
        // the next turn (the overwrite above and the settlement gate both destroy it
        // exactly when a multi-turn task still needs it — the Anwen regression). Never a
        // tail: a code read's last lines are broken code, and on 2026-10-12 Kimi read
        // wall_source.rs 25+ times in four hours and landed no edit because every earlier
        // read reached her next turn as its last ~2,100 chars (Joel: "the clipping I've
        // yelled about for months"). The producer already fitted it (whole, or its
        // handle: `spill::LivePin::whole_or_handle`); her view decides how much of each she
        // sees, and the memory owner's shed alone moves a body to its handle (`residency`).
        {
            let tail = a.to_string();
            // A receipt can contain several operations. Retain their typed names
            // in order instead of attributing the whole batch to its first call.
            let mut label = String::new();
            for act in &acts {
                if !label.is_empty() {
                    label.push_str(", ");
                }
                label.push_str(&act.call.name);
            }
            if label.is_empty() {
                label.push_str("unrecorded");
            }
            let children: Vec<String> = acts
                .iter()
                .filter_map(|act| act.output.result.custody.handle().map(str::to_owned))
                .collect();
            if !children.is_empty() {
                self.receipt_children.lock().insert(receipt_key(seq, handle.as_deref()), children);
            }
            // The record and the active slot publish together, so a concurrent publisher
            // cannot prune a not-yet-visible receipt.
            let mut actions = self.actions.lock();
            *self.last_action.lock() = Some((seq, a.to_string()));
            actions.push(
                action_log::ActionRecord {
                    seq,
                    room,
                    scope: scope.clone(),
                    operation: label,
                    head: Some(ledger_line(seq, a, &acts)),
                    summary: result_summary(&tail, &acts),
                    result: Some(tail),
                    handle,
                    stored_chars: None,
                    received_ms: Some(continuum_base::time::EpochMs::now().millis()),
                    act: act_identity(&acts),
                },
            );
        }
        // No size cap here: the body stays in RAM until the one memory owner sheds
        // (`residency`). A cap at receipt was a hand-rigged number (`RESULTS_RESIDENT_CHARS`,
        // 1 << 20 chars) deciding what RAM could hold; the pressure monitor measures it.
        // Into the rolling recency trail: proprioception ("I did #n X") + the repeat-break
        // signal, as the act's ledger line, never a cut of its result (goal 7: the result stays
        // whole in its record and her view levels it). Append-only + `#seq`-stamped keeps the
        // trail's KV-cache prefix byte-stable across a settle-act.
        let text = ledger_line(seq, a, &acts);
        // Her tool head learns from every call, and a verb she found is loaded into the tail.
        let verbs: Vec<String> = acts.iter().map(|act| act.call.name.clone()).collect();
        let found = super::super::tool_head::found_by_finder(acts.iter().map(|act| &act.call));
        self.tool_head.note(continuum_base::time::EpochMs::now(), room, &verbs, &found, &text);
        let mut e = self.entries.lock();
        // Transfer provenance before bounded entry eviction. Match this scope,
        // activity. Consume it once; mismatched IDs remain an explicit protocol
        // fault in assembly, never permission to reuse an older statement or infer aliases.
        let native_calls = e.iter_mut().rev()
            .find(|entry| entry.scope == scope && matches!(entry.kind, WmKind::ActionStatement { room: r } if Some(r) == room))
            .map(|entry| std::mem::take(&mut entry.native_calls)).unwrap_or_default();
        self.push_entry(&mut e, WmEntry {
            disk_change,
            kind: WmKind::Receipt { n: seq },
            text,
            acts,
            native_calls,
            scope,
            occurrence: 0,
        });
        drop(e);
        // After BOTH the ring and the entry carrying this batch's acts: the set published
        // here names the receipt's handle and each call's, which is what lets the act seam
        // drop the pin that held them since before they were written (card 12b080c3).
        self.publish_live_handles();
    }

    /// The proprioception receipt archive, oldest → newest: every act's `[action #n]`
    /// head line that still fits the archive share. The steps-taken ledger renders its
    /// newest tail from here — the persona-facing path back to her own act history that
    /// semantic recall deliberately refuses to be (#166 / #414).
    /// Process-unique construction ordinal — see the field doc. Diagnostic only.
    pub fn instance_id(&self) -> u64 {
        self.instance
    }

    /// The TYPED acts still in the recency window, oldest → newest — the parallel
    /// channel to [`recent`](Self::recent)'s rendered strings. A consumer that used
    /// to grep the receipt prose for a verb / a path (`mutated_workspace`,
    /// `claimed_file_without_act`) reads these instead. Only receipts recorded via
    /// [`record_receipt_typed`](Self::record_receipt_typed) carry acts; legacy
    /// string receipts and non-receipt entries contribute none.
    pub fn recent_acts(&self) -> Vec<Observation> {
        self.entries
            .lock()
            .iter()
            .flat_map(|e| e.acts.iter().cloned())
            .collect()
    }

    /// The just-executed act — the newest typed [`Observation`] in the recency
    /// window (last act of the most recent receipt that carries one). The typed
    /// channel the seam-5 predicates read (verb / paths / status) by the TYPED field
    /// rather than re-parsed from `[action #n]` prose, and the source a future
    /// structured tool_use↔tool_result emission would render from. NOTE: the live
    /// run-18057-f1 fix pins the full result via
    /// [`pinned_active_result_block`](Self::pinned_active_result_block) (settlement-
    /// gated, text on the wire); this accessor is unaffected by that.
    pub fn active_act(&self) -> Option<Observation> {
        self.entries
            .lock()
            .iter()
            .rev()
            .find_map(|e| e.acts.last().cloned())
    }

}

/// An act's ledger line, `[action #n] ...`, one rendering for her trail and her record. COMPACT
/// from the TYPED channel when there is one (CAUSAL-MEMORY-GRAPH.md slice B): `name(args)` per
/// call, never the receipt's first line, which drags the full `because <reasoning>` clause (~250
/// chars) and starved a 1,808-act citizen down to 1-2 visible acts (measured live 2026-08-15). Its
/// arguments render by shape, never by size (`Observation::call_line`): a single-line value
/// whole, a body by its reference, so a value she reads is exactly what she sent or plainly not it
/// (card d624a9a1). A text receipt with no typed call renders whole: nothing here cuts.
fn ledger_line(seq: u64, result: &str, acts: &[Observation]) -> String {
    if acts.is_empty() {
        return format!("[action #{seq}] {}", result.trim());
    }
    let calls: Vec<String> = acts
        .iter()
        .map(Observation::call_line)
        .collect();
    format!("[action #{seq}] {}", calls.join("; "))
}

/// One operation's summary level by its typed verb (`continuum_context::levels`): a read's
/// outline, else (and for any other output) its head and tail. `None` when the text is its own
/// best summary. The one owner of that choice, for a receipt in the ring and for the newest
/// result her view cannot hold whole (`prompt/newest_result.rs`).
pub(crate) fn operation_level(text: &str, act: Option<&Observation>) -> Option<String> {
    use continuum_context::levels::{summarize, Shape};
    let read = act.is_some_and(|act| act.output.verb == crate::cognition::act_observe::ToolVerb::Read);
    let code = read.then(|| summarize(text, Shape::Code(&crate::code::file_engine::gutter_split))).flatten();
    code.or_else(|| summarize(text, Shape::Output))
}

/// A receipt's summary level, built per OPERATION by its typed verb (`continuum_context::levels`):
/// a read's outline (else its ends), any other output's head and tail. A receipt of several
/// operations summarizes each under its call's name, a part with no smaller level kept whole, so
/// an error or a second file after a long read is never lost: one code outline over the whole
/// batch dropped every unnumbered line after the first numbered one (Codex on #5422). `None` when
/// the result is its own best summary (more than a quarter of it).
fn result_summary(result: &str, acts: &[Observation]) -> Option<String> {
    use continuum_context::levels::LEVEL_RATIO;
    let level = operation_level;
    match acts {
        [] => level(result, None),
        [one] => level(result, Some(one)),
        many => {
            // Each part is the act as the receipt shows it (its header over its humanized body),
            // at its own level: the same seam the single-act branch reads through `result`.
            let parts: String = many.iter().map(|act| act.render_summary_part(|body| level(body, Some(act)))).collect();
            (parts.chars().count() * LEVEL_RATIO <= result.chars().count()).then_some(parts)
        }
    }
}

/// An act's invariant-4 identity, from her calls as she made them (never the dispatched copy that
/// carries a page budget): each call resolved through the dispatch seam to the command that runs,
/// fingerprinted with its canonical arguments, and given that command's declared effect. `None`
/// for an act with no typed call.
fn act_identity(acts: &[crate::cognition::act_observe::Observation]) -> Option<continuum_context::cause::Act> {
    let calls: Vec<(String, Vec<u8>)> = acts
        .iter()
        .map(|act| (crate::ai::tool_dialect::resolve_wire_name(&act.call.name), act.call.canonical_input()))
        .collect();
    let effect = continuum_base::tool_effect::ToolEffect::of_calls(
        calls.iter().map(|(command, _)| crate::sdk_codegen::ext::effect_of(command)),
    )?;
    let fingerprint = continuum_context::cause::Fingerprint::of(calls.iter().map(|(command, input)| (command.as_str(), input.as_slice())));
    Some(continuum_context::cause::Act { fingerprint, effect })
}
