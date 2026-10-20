//! The `Faculty` that bids working memory into the workspace.

use super::*;

/// Render the already-budgeted trail in order. A later settlement may correct
/// or repeat an earlier report; similarity cannot replace that event's content.
fn render_trail(recent: &[String]) -> String {
    let mut out = String::new();
    for entry in recent {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("- ");
        out.push_str(entry);
    }
    out
}

/// Perception-tier faculty that bids the persona's recent reasoning into the
/// workspace — so the deliberator can resume its own train of thought rather than
/// re-deriving it cold each turn. Abstains when there's nothing recorded (first
/// turns, or thinking suppressed).
pub struct WorkingMemoryFaculty {
    memory: Arc<WorkingMemory>,
}

impl WorkingMemoryFaculty {
    pub fn new(memory: Arc<WorkingMemory>) -> Self {
        Self { memory }
    }

    fn faculty_id() -> FacultyId {
        FacultyId::Custom(WM_FACULTY_ID.to_string())
    }
}

#[async_trait]
impl Faculty for WorkingMemoryFaculty {
    fn id(&self) -> FacultyId {
        Self::faculty_id()
    }

    // Perception tier (default): reacts to the raw world-state, bidding the recent
    // reasoning into phase 1 so the deliberator conditions on it in phase 2.
    async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
        let mut entries = self.memory.recent_entries();
        // Public action statements use their room-scoped assistant projection,
        // never the optional interior-thought bid. Facts use the required notice projection.
        entries.retain(|entry| !matches!(entry.kind, WmKind::ActionStatement { .. } | WmKind::Fact));
        let dispatched = self.memory.dispatched_snapshot();
        if entries.is_empty() && dispatched.is_empty() {
            return None;
        }
        // Render BY KIND (the recent_entries contract; PERCEPTION-FACTS.md).
        // Substrate Facts ([resumed]/[context]/[repetition]…) are the SYSTEM's
        // second-person notices posted into her window — NOT her thoughts.
        // Glass-boxed 2026-08-01 (#264 fourth finding): lumping them into the
        // "my interior state" block made the model resolve the voice conflict
        // by converting the notice to first person and SPEAKING it — 2f50b223
        // posted "My session was interrupted under a minute ago and my memory
        // restored; nothing was in flight" verbatim into the room, a
        // scaffold-echo of the [resumed] briefing. Same bug class as the
        // second-person header this comment block's sibling above fixed
        // (#264 third finding): provenance framing must match the voice.
        // Each entry CLIPPED to the same head bound receipts already use —
        // measured 2026-08-24: ONE unclipped reasoning trace (a 16k-token
        // think emission) made this block 70,086 chars (~20k tokens), the
        // single dominant mass of an 85k prompt. The full text stays in the
        // entry (the pinned path and recall read it); only the TRAIL RENDER
        // is bounded, with the collapse idiom naming what was elided.
        // A WORK turn perceives its own recent past compactly: the newest few
        // reasonings/receipts in full and the older ones as one count line —
        // collapsed, not clipped. Measured 2026-09-05: this block was 14–23 KB of
        // a ~22k-token prompt on every act, with zero KV reuse; a card's work turn
        // needs what she just did, not the morning.
        let mut collapsed_older = 0usize;
        if ws.workspace_deliverable {
            const WORK_TURN_RECENT_KEEP: usize = 3;
            let non_fact: Vec<usize> = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !matches!(e.kind, WmKind::Fact))
                .map(|(i, _)| i)
                .collect();
            if non_fact.len() > WORK_TURN_RECENT_KEEP {
                let drop: std::collections::HashSet<usize> = non_fact
                    [..non_fact.len() - WORK_TURN_RECENT_KEEP]
                    .iter()
                    .copied()
                    .collect();
                collapsed_older = drop.len();
                entries = entries
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !drop.contains(i))
                    .map(|(_, e)| e)
                    .collect();
            }
        }
        // Per-entry head in a WORK turn: 1,500 chars — past that a work act's
        // reasoning is history, not state.
        let head = if ws.workspace_deliverable {
            self.memory.budget().trail_head_chars().min(1_500)
        } else {
            self.memory.budget().trail_head_chars()
        };
        // A THOUGHT keeps its end (its decision); a receipt or settlement keeps its
        // head, where its identity is (`[action #N]` and what was done).
        let clip = |e: &WmEntry| -> String {
            if matches!(e.kind, WmKind::Thought) {
                return collapse_thought(&e.text, head);
            }
            let t = e.text.as_str();
            if t.chars().count() <= head {
                t.to_string()
            } else {
                let kept: String = t.chars().take(head).collect();
                format!("{kept} …[{} more chars — my full thought, collapsed]", t.chars().count() - head)
            }
        };
        let recent: Vec<String> = entries
            .iter()
            .map(clip)
            .collect();
        // Render oldest-first, newest-LAST: position alone carries recency (the last
        // line is the most recent thought), the universal chat-history convention.
        //
        // We deliberately do NOT prefix each trace with a relative "(turn -N)" label.
        // That offset rewrites every act (what was -1 becomes -2 when a new trace
        // appends), mutating the whole block from its first byte. Combined with this
        // whole contribution now rendering as a TRAILING turn nearest generation
        // (`Contribution::trailing`, #205) rather than inside the system message, the
        // append-only formatting finally pays off: a settle-act appends its one new
        // trace to the very end of the token stream, so the entire system prefix AND
        // the prior conversation keep their KV cache and only the new tail re-prefills.
        // (Before #205 this block sat in the system message, so appending a trace
        // shifted every conversation token after it — the append-only property was
        // defeated by position and ~4000 tokens re-prefilled per act.) Each trace
        // already carries its own stable absolute marker (e.g. "[action #41]");
        // recency needs no per-act-mutating annotation.
        let n = recent.len();
        let mut sections: Vec<String> = Vec::new();
        // The append-only reasoning/action trail (may be empty if the only live content
        // is dispatched background work).
        if !recent.is_empty() {
            sections.push(format!(
                // First person, explicitly not-a-message (glass-boxed 2026-07-31,
                // #264 third finding): this block rides a `user`-role turn, and the
                // old second-person header — "Your recent thoughts and actions" —
                // parsed as a PEER addressing her. Asha opened turn after turn with
                // "Thank you for the summary of recent actions and thoughts",
                // politely replying to her own interoception. Self-knowledge must
                // never be phrased in another's voice.
                "My own recent thoughts and actions (working memory — this is my \
                 interior state, not a message from anyone):\n{}",
                format!("{}{}", render_trail(&recent), if collapsed_older > 0 { format!("\n[{collapsed_older} earlier acts this session, collapsed — their receipts are still in my memory]") } else { String::new() })
            ));
        }
        // The FULL result of the most recent act is NO LONGER emitted here. It is the
        // load-bearing content for the mind to act on what its hands fetched, and as part
        // of THIS 0.5-salience bid it rode `arbiter.focus()` top-k and was silently evicted
        // under capacity pressure (the run-18057-f1 0-byte patch). It is now PINNED by the
        // message builder as its own durable trailing turn — see
        // [`WorkingMemory::active_action_full`] and [`WorkingMemory::full_result_message`]
        // — so no attention pass can drop it.
        // This contribution keeps only the append-only trail + dispatched status,
        // which are proprioceptive summary, not the just-fetched result.
        // Substrate notices are projected by PerceptionFact independently of this bid.
        // Background commands the mind sent away — sentinels, compiles, debuggers —
        // streaming their status back by handle. The mind sees what's outstanding and what
        // just finished, and can pass a handle to a follow-up command (cancel/query). Fresh
        // each event, so it trails the stable prefix like the full-result block.
        if !dispatched.is_empty() {
            let lines = dispatched
                .iter()
                .map(|(h, label, latest, status)| {
                    let hs = h.to_string();
                    let short = hs.get(..8).unwrap_or(hs.as_str());
                    let tag = match status {
                        DispatchStatus::Running => "running",
                        DispatchStatus::Done => "done",
                        DispatchStatus::Failed => "failed",
                    };
                    let tail = if latest.is_empty() {
                        String::new()
                    } else {
                        format!(": {latest}")
                    };
                    format!("- #{short} {label} [{tag}]{tail}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "Commands you dispatched (background, by handle):\n{lines}"
            ));
        }
        Some(
            Contribution::context(
                Self::faculty_id(),
                sections.join("\n\n"),
                WORKING_MEMORY_SALIENCE,
                format!(
                    "working memory: {n} trace(s), {} dispatched",
                    dispatched.len()
                ),
            )
            // Proprioception that grows each act — renders as a trailing turn nearest
            // generation, never in the system message, so the KV prefix stays stable
            // across a settle-act (#205). See [`Contribution::trailing`].
            .trailing(),
        )
    }
}
