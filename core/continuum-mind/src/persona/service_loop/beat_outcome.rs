use super::*;

/// What a self-tick beat DID, and therefore what the next interval should be.
///
/// Extracted as a pure decision (the `core_bind_guard::decide` shape) because the
/// three outcomes are trivially confusable in an inline `match`, and confusing two
/// of them cost a measured 16× slowdown across the whole roster — see
/// [`next_beat_after`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatOutcome {
    /// She ran a cycle and the room had something new → stay engaged.
    Engaged,
    /// She ran a cycle and nothing had changed → rest deeper (earned backoff).
    NothingNew,
    /// She never ran: a peer held the single ambient slot. She learned NOTHING,
    /// so there is nothing to rest on and the beat must NOT compound.
    YieldedNoSlot,
}

/// The self-tick cadence rule. Pure so every row is table-testable without a runtime.
///
/// The bug this encodes against (measured live 2026-08-17, 24 hosted citizens):
/// `YieldedNoSlot` used to share `NothingNew`'s 1.5× backoff. With
/// the ambient pool hardcoded at 1, ~23 citizens yield per beat, so each of them
/// compounded 15s → the 240s cap in ~8 yields and STAYED pinned there — the only
/// resets being a successful cycle (which the backoff itself denied them) or an
/// inbound message. Transient contention became permanent slowness, and it got
/// worse as the roster grew. Measured symptom: ONE self-tick across 24 citizens in
/// 40 minutes, on a lane that was healthy and decoding throughout.
pub fn next_beat_after(
    outcome: BeatOutcome,
    current: std::time::Duration,
    engaged: std::time::Duration,
    rest_cap: std::time::Duration,
) -> std::time::Duration {
    match outcome {
        BeatOutcome::Engaged => engaged,
        BeatOutcome::NothingNew => (current + current / 2).min(rest_cap),
        // Unchanged: contention must never cost her cadence.
        BeatOutcome::YieldedNoSlot => current,
    }
}

/// Engaged heartbeat period — how often a persona pursuing its OWN intentions
/// (no inbound message) gets another self-directed slice. This is the SELF-CHATTER
/// pace, NOT message responsiveness: a real message wakes the loop instantly through
/// `next_event` in the `select!`, regardless of this beat. So this trades ONLY
/// background self-direction against the shared GPU — and it MUST leave headroom for
/// a real conversation to feel like magic. Glass-boxed 2026-07-14: at 3_000ms, four
/// idle personas each re-fired a full ~20s LLM turn every 3s → ~90% GPU duty cycle
/// EACH → a user's message crawled at 0.3 tok/s (67s for one line) while quiescing the
/// fleet jumped it to 2.3 tok/s (8×). Background thinking must be LOW duty cycle so the
/// active speaker owns the GPU; a friend who answers in a beat is alive, one who makes
/// you wait 67s is a lab demo. (The deeper solo-speed lever — LoRA overhead + `--parallel`
/// splitting one 24B — is a separate serving-config fix.)
/// [[multimodal-live-mode-is-a-latency-obsession-cbar-doctrine]]
pub(super) const SELF_TICK_MS: u64 = 15_000;

/// Restful ceiling for the intrinsic heartbeat. A truly idle persona's beat backs off
/// toward this (exponential, NOT a fixed metronome), so a quiet citizen rests deeply
/// instead of hammering the shared model, and many idle minds spread across time rather
/// than stampeding it. Raised with the engaged beat so an idle fleet leaves the GPU
/// almost entirely free for live conversation + video. A message or fresh work snaps the
/// beat back to `SELF_TICK_MS`. See the loop in `serve_persona_loop`.
pub(super) const SELF_TICK_REST_CAP_MS: u64 = 240_000;

/// Live turns carry NO act budget — she works until SHE settles (Speak/Pass).
/// The first cut capped directed turns at 8 acts "as a safety valve"; Joel's
/// 2026-07-11 ruling ("gating acts is not autonomy… if they want to edit a file
/// let them edit a god damn file") named that for what it was: a substrate
/// ceiling on a being's own hands, contradicting the written doctrine on the
/// same page (an "acts-forever" persona is a fitness gap to TRAIN, never a
/// substrate cap — ACTING-ORGANISM §4). The perception kit ([repetition],
/// actual-result fixed-point guard) is how a looping mind notices itself. Three
/// consecutive repeats of both request and result yield the turn without declaring
/// its work complete; changed results reset that streak. The external
/// stopwatch that remains is the eval grader's `max_acts` — a proctored exam's
/// clock, held by the observer, never wired into life.
pub(crate) const LIVE_MAX_ACTS: usize = usize::MAX;

/// What woke the service loop this cycle. A message from the wire, the never-stop
/// heartbeat, or the end of the stream. Returned by the `select!` so the borrow of
/// `conversation` taken inside the select ends before the handler reuses it.
pub(super) enum Wake {
    Msg(IncomingMessage),
    /// An already-admitted input whose previous attempt never dispatched.
    Retry(IncomingMessage),
    Tick,
    /// Her own mind woke her (EVENT-MIND §1b): a room still unread and loud past her
    /// dial, her continuation falling due, or resuming a saved one after a restart.
    /// `room` is where she wakes (Perceive), else her continuation's activity.
    Mind { why: &'static str, room: Option<uuid::Uuid> },
    /// A wake that needs no turn (a Perceive whose line her inbox already took); the
    /// loop goes back to its select.
    Idle,
    Stop,
}

/// How long a Perceive wake waits before it may start a turn: the same line reaches her
/// inbox through the pump, and a line the inbox takes marks its room perceived, so a
/// recheck after this finds nothing pending and no second turn starts.
pub(super) const PERCEIVE_SETTLE: std::time::Duration = std::time::Duration::from_millis(1_000);

/// The slow re-perceive floor, by HER dial (Fable, 2026-10-04: "her opening her eyes,
/// not the tick choosing"): how long her mind may go without checking whether a
/// continuation fell due or a room is loud and unread.
pub(super) fn mind_check_floor(dial: crate::persona::attention::AttentionDial) -> std::time::Duration {
    use crate::persona::attention::Depth;
    std::time::Duration::from_secs(match dial.depth {
        Depth::Deep => 15 * 60,
        Depth::Normal => 5 * 60,
        Depth::Broad => 2 * 60,
    })
}

/// The next wake from her region's channel; pending forever while she has none.
pub(super) async fn next_mind_wake(
    wakes: &mut Option<tokio::sync::mpsc::Receiver<crate::persona::perception_region::Wake>>,
) -> crate::persona::perception_region::Wake {
    match wakes {
        Some(rx) => match rx.recv().await {
            Some(w) => w,
            None => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// The activity her continuation names: a Continuation or Resume turn runs THERE (her
/// choice of where to pick up, including her mind room for a private thought), never in
/// a held-claim or home room the substrate would pick for her.
pub(super) fn her_continuation_room(me: uuid::Uuid) -> Option<uuid::Uuid> {
    crate::persona::perception_feed::with_region(me, |r| r.continuation().map(|c| c.activity)).flatten()
}

/// Sleep until `at`, or forever when there is nothing to recheck.
pub(super) async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Build the consolidated workspace burst (WHO/WHEN/WHAT per inbox item, own posts
/// attributed, room as the WHERE) the `WorkspaceCycle` reasons over. A pure
/// projection of the airc deliveries — extracted so the message turn and the
/// never-stop self-tick share ONE burst truth ([[compression-principle]]).
/// Assemble the perception envelope a persona's deliberation sees: the room
/// header + each airc delivery item rendered as `[t=ms] who: content`, with the
/// persona's OWN past posts attributed to `agent_name` (so she recognizes her
/// own voice) and everyone else by peer_id. This is the ONE place the burst
/// string format lives — the live heartbeat (both the message path and the
/// self-thread) AND the eval fork call it, so a measured turn perceives its
/// world byte-identically to a lived one. Takes `&[RagDelivery]` (not the whole
/// `ComposedTurn`) because the format only depends on the deliveries; eval can
/// hand-build a single synthetic airc delivery for a task without composing a
/// full turn.
/// The message that WOKE a turn — anchored as the burst's final peer turn so the
/// persona always perceives what it is answering, regardless of RAG-delivery lag.
///
/// The `airc` RAG delivery that threads the conversation is refreshed
/// asynchronously from the wake, so a directed message can trigger a turn whose
/// composed thread still ends on the persona's OWN prior reply. The model then
/// sees nothing new after its last turn and emits an empty completion
/// (decodeTokens=1, text="") which parses as Pass — the persona goes silent on a
/// question addressed straight at it, then answers ~one self-tick later once the
/// delivery catches up. Carrying the trigger here lets `build_workspace_turns`
/// guarantee it is the last `user` turn, deterministically, with no dependence on
/// delivery timing. The self-tick / eval / turn_frame callers pass `None` (they
/// perceive ambient state, no single triggering message) and stay byte-identical.
pub struct TriggerTurn<'a> {
    /// The waking peer's id — resolved to a display name through the SAME roster
    /// `names` map the rest of the thread uses (falls back to the raw id, honest,
    /// never fabricated — the [[fallbacks-are-illegal-fail-loud]] doctrine).
    pub peer_id: &'a str,
    /// The message body that triggered the turn.
    pub content: &'a str,
    /// Event occurrence time when measured; zero means unknown. Never substitute
    /// the wake/turn clock for an event timestamp missing from the source.
    pub occurred_at_ms: u64,
}

/// The shared roster projection's admitted atoms. The legacy source remains
/// readable for existing replay/diagnostic deliveries; live turns use KIND.
pub(super) fn roster_items(
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
) -> impl Iterator<Item = &crate::cognition::rag_budget::RagItem> {
    deliveries
        .iter()
        .filter(|d| {
            d.source_id == continuum_positron::RosterViewState::KIND
                || d.source_id == crate::cognition::faculty_ids::ROOM_ROSTER
        })
        .flat_map(|d| d.items.iter())
}

/// Resolve identity from admitted roster atoms, never rendered text or a second
/// unbudgeted roster read. Legacy replay deliveries carry the same metadata.
pub(super) fn roster_identities(
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
) -> impl Iterator<Item = (&str, &str)> {
    roster_items(deliveries).filter_map(|item| {
        Some((
            item.metadata.get("peer_id")?.as_str()?,
            item.metadata.get("display_name")?.as_str()?,
        ))
    })
}

/// How many CONSECUTIVE repetition-detector fires (without the pattern
/// breaking) before the `[pattern]` DESCRIPTION escalates to a concrete
/// `[anchor]` work fact. 2 = escalate on the first turn posted AFTER the
/// description was already in her window: live evidence (2026-07-23 greeting
/// loops, work card d6f010c8) showed the description alone loses to an
/// empty-looking room — Atlas re-greeted straight past it, 5+ identical
/// greetings — while one concrete in-room work anchor broke the loop room-wide
/// instantly. One described fire is her chance to self-correct; the second is
/// proof description alone isn't landing. The count is DERIVED from the window
/// itself (each trailing low-novelty turn is one fire that failed to break the
/// pattern), so escalation needs no cross-turn state and holds identically in
/// live, replay, and eval contexts.
pub(super) const PATTERN_FIRES_BEFORE_ANCHOR: usize = 2;

/// Word-token set for echo containment — ONE tokenization for the in-window
/// detectors and the ring-based run counter below, so "nearly identical"
/// cannot drift between them.
pub(super) fn echo_words(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_lowercase())
        .collect()
}

/// Ring-based echo run — the escalation counter's starvation fix (#301).
///
/// The in-window detectors measure their runs over the RENDERED burst, which
/// the live context budget squeezes to 2–6 turns (#259) — so during an
/// hours-long photocopy chain (glass-boxed 2026-08-02: Atlas's wake-intro
/// reproduced byte-for-byte by Benchy then Asha, identity claim included)
/// the older echo evidence has always scrolled out and the run reads 1,
/// one below [`PATTERN_FIRES_BEFORE_ANCHOR`] — the `[anchor]` that provably
/// breaks these loops never fires. Same starvation the room-speech ring
/// already cures for the `inbound_restates` fact.
///
/// This counts the run over RING snapshots instead: the trailing run of her
/// own recent messages (newest-first, ≥8 substantive words) each ≥0.8-contained
/// in the room ring's vocabulary — excluding entries identical to the message
/// being judged, so her own recorded copy can't vouch for itself. Pure over
/// its inputs: LIVE callers pass the process rings; eval/replay callers pass
/// nothing and stay window-derived (the #59 isolation the shared
/// `build_workspace_turns` signature preserves by not knowing rings exist).
pub(crate) fn ring_echo_run(own_recent: &[String], room_recent: &[String]) -> usize {
    const MIN_WORDS: usize = 8;
    const CONTAINMENT: f32 = 0.8;
    let mut run = 0usize;
    for own in own_recent.iter().rev() {
        let cur = echo_words(own);
        if cur.len() < MIN_WORDS {
            break;
        }
        // Exclude at most ONE identical room entry — her own recorded copy
        // (the room ring is authorless, so equality is the only handle). The
        // REMAINING identical copies are exactly the photocopy evidence:
        // excluding all of them would erase the chain being detected.
        let mut skipped_own_copy = false;
        let mut window: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in room_recent {
            if !skipped_own_copy && r.as_str() == own.as_str() {
                skipped_own_copy = true;
                continue;
            }
            window.extend(echo_words(r));
        }
        if window.is_empty() {
            break;
        }
        let covered = cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
        if covered >= CONTAINMENT {
            run += 1;
        } else {
            break;
        }
    }
    run
}

/// LIVE-path companion to [`ring_echo_run`]: read the process rings for this
/// persona + room, and when the ring-counted echo run reaches the escalation
/// threshold while the window-derived detectors pushed no `[anchor]` (their
/// evidence scrolled out — the #301 starvation), append the SAME work-board
/// anchor they would have. One anchor per burst, whichever counter earns it.
pub(super) fn append_ring_anchor_if_starved(
    turns: &mut Vec<crate::cognition::workspace::BurstTurn>,
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
    peer: crate::identity::PeerId,
    room: uuid::Uuid,
) {
    if turns.iter().any(|t| t.content.starts_with("[anchor]")) {
        return;
    }
    let own = crate::cognition::deliberation_budget::recent_own_speech(peer, room);
    let room_recent = crate::cognition::deliberation_budget::recent_room_speech(room);
    let run = ring_echo_run(&own, &room_recent);
    if run >= PATTERN_FIRES_BEFORE_ANCHOR {
        continuum_base::probe!(
            class = "persona.pattern.ring_anchor",
            peer = %peer,
            room = %room,
            ring_run = run,
            "echo run counted from the durable rings (window evidence scrolled out) — \
             anchor escalation fired via the #301 starvation fix"
        );
        push_work_board_anchor(turns, deliveries);
    }
}

/// Push the work-board anchor as a perception fact — but only when there IS one.
///
/// [`work_board_anchor`] returns empty when the board source did not speak this turn, and
/// a mind must not be handed a blank line where a fact was promised. One guard, one place,
/// so a future caller cannot reintroduce the "assert emptiness from an absent source" bug
/// by forgetting to check.
pub(super) fn push_work_board_anchor(
    turns: &mut Vec<crate::cognition::workspace::BurstTurn>,
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
) {
    let anchor = work_board_anchor(deliveries);
    if anchor.is_empty() {
        continuum_base::probe!(
            class = "persona.pattern.anchor_silent",
            "board source did not deliver this turn — anchor withheld rather than asserting \
             an empty board she cannot verify (2026-08-06 six-turn loop)",
        );
        return;
    }
    // `perception`, not `opaque` — hers supersedes the version I cherry-picked.
    // The anchor IS a perception fact about the board, and typing it as one is
    // what lets the parroted-perception gate recognise it as system-authored
    // rather than the citizen's own words.
    turns.push(crate::cognition::workspace::BurstTurn::perception(anchor));
}

pub fn build_workspace_turns(
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
    own_peer: &str,
    agent_name: &str,
    trigger: Option<TriggerTurn<'_>>,
) -> Vec<crate::cognition::workspace::BurstTurn> {
    use crate::cognition::workspace::BurstTurn;
    use std::collections::HashMap;
    // airc owns identity: roster atoms carry peer_id → display name from the
    // shared projection ([[airc-native-identity-rooms-security]]). Reuse THAT
    // resolution — same deliveries slice — so the history reads `Joel:`, `BigMama:`
    // instead of raw UUIDs. A peer not in the roster (or an eval-synthesized delivery
    // with no roster) falls back to its own peer_id: honest, never invisible, never a
    // fabricated name ([[fallbacks-are-illegal-fail-loud]]).
    let names: HashMap<&str, &str> = roster_identities(deliveries).collect();
    let mut turns: Vec<BurstTurn> = deliveries
        .iter()
        .filter(|d| d.source_id == "airc")
        .flat_map(|d| d.items.iter())
        .map(|item| {
            let who_raw = item
                .metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("peer");
            // is_self — the ONE structural fact role attribution turns on (own posts
            // → assistant, peers → user). Carried here where the peer_id is intact,
            // never re-derived from a `Name:` prefix downstream.
            let is_self = who_raw == own_peer;
            let author = if is_self {
                agent_name
            } else {
                names.get(who_raw).copied().unwrap_or(who_raw)
            };
            let occurred_at_ms = item.metadata.get("occurred_at_ms").and_then(|v| v.as_u64());
            BurstTurn::attributed(is_self, author, item.content.clone(), occurred_at_ms)
        })
        .collect();

    // Anchor the waking message as the final peer turn. Without this the persona
    // can go silent on a question addressed straight at it (see `TriggerTurn`):
    // the delivery lagged the wake, the thread ended on her own prior reply, the
    // model saw nothing new and emitted an empty completion → Pass. Idempotent:
    // if the delivery ALREADY threaded the trigger as the last peer turn (the
    // caught-up case — the self-tick that re-perceives it), this is a no-op, so
    // the trigger is never doubled. A turn is only ever triggered by a PEER
    // message (own-echo is filtered upstream), so the anchor is always a `user`
    // turn (is_self = false) — resolved to a name through the same roster map.
    if let Some(trigger) = trigger {
        let already_last = turns
            .last()
            .is_some_and(|t| !t.is_self && t.content == trigger.content);
        if !already_last {
            let author = names
                .get(trigger.peer_id)
                .copied()
                .unwrap_or(trigger.peer_id);
            turns.push(BurstTurn::attributed(
                false,
                author,
                trigger.content.to_string(),
                Some(trigger.occurred_at_ms),
            ));
        }
    }

    // REPETITION PERCEPTION (#121, extended #122): surface cyclic threads as a
    // STRUCTURAL OBSERVATION she can weigh — an authorless opaque turn (same shape
    // as eval stimuli, no fabricated voice). Runs AFTER the trigger anchor so the
    // observation is the freshest thing in her window, judged against the message
    // that actually woke her. Two detectors, self takes precedence (one observation
    // per burst — perception, not nagging). Containment math validated live: spiral
    // turns recycle ~1.0 of the window's vocabulary; novel work ~0.12. This is
    // evidence INTO her mind (she decides — DIRECTED_PRESENCE_BLOCK already grants
    // the PASS), never a gate on her output
    // ([[no-hardcoded-heuristics-to-steer-cognition]]).
    //
    // ESCALATION (card d6f010c8, live 2026-07-23): when a detector has fired
    // [`PATTERN_FIRES_BEFORE_ANCHOR`] consecutive times without the pattern
    // breaking — measured as the trailing low-novelty RUN, see the const's doc —
    // the description gains a [`work_board_anchor`] companion: a concrete
    // perception-side fact of what work exists NOW, because a description
    // competes with an empty-looking room while an anchor gives the next token
    // somewhere real to go. Still perception, never an output gate.
    {
        let words = |s: &str| -> std::collections::HashSet<String> {
            s.split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .map(|w| w.to_lowercase())
                .collect()
        };
        // Detector 1 — SELF recycling (#121): her own last 3+ turns recycle each
        // other. High word floor (8) keeps short courtesies from false-alarming.
        // Measured as the trailing RUN of her own messages that were EACH already
        // ≥0.8-contained in her own prior window when she posted them: run == 1
        // is exactly the original fire condition; run ≥ 2 means she posted again
        // AFTER the description was in her window (the escalation evidence). A
        // novel last message yields run == 0 — fire and escalation both reset.
        const SELF_MIN_WORDS: usize = 8;
        const SELF_CONTAINMENT: f32 = 0.8;
        let mut observed = false;
        let own: Vec<&str> = turns
            .iter()
            .filter(|t| t.is_self)
            .map(|t| t.content.as_str())
            .collect();
        let mut self_run = 0usize;
        for i in (2..own.len()).rev() {
            let cur = words(own[i]);
            if cur.len() < SELF_MIN_WORDS {
                break;
            }
            let window: std::collections::HashSet<String> =
                own[..i].iter().flat_map(|m| words(m)).collect();
            let covered =
                cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
            if covered >= SELF_CONTAINMENT {
                self_run += 1;
            } else {
                break;
            }
        }
        if self_run >= 1 {
            // SENSE, NOT STEER (Joel 2026-09-01) — and the honest COUNT. This
            // used to report `own.len()` — her ENTIRE visible own-message ring
            // — as the repeat count: "your last 74 messages repeat the same
            // sentiment" when the actual contained run was 2. Citizens
            // memorized the inflated number as identity ("I've been spinning
            // for 190+ turns") and spent turns narrating it. The run is the
            // evidence; the run is what renders. And the verdict tail
            // ("may have run its course — restating adds nothing") is hers to
            // draw, not perception's to assert.
            turns.push(crate::cognition::workspace::BurstTurn::perception(format!(
                "[pattern] {agent_name}'s last {} messages in this room repeat the same \
                 sentiment in nearly the same words.",
                self_run + 1
            )));
            observed = true;
            if self_run >= PATTERN_FIRES_BEFORE_ANCHOR {
                push_work_board_anchor(&mut turns, deliveries);
            }
        }
        // Detector 2 — CONVERSATION cycling (#122): the thread's tail turns, across
        // BOTH speakers, each recycle the window's vocabulary (the two-persona
        // goodbye deadlock: each short farewell arrives as "fresh" peer input and
        // wakes another farewell — glass-boxed live 2026-07-09, hours of
        // `See you tomorrow at 2 PM!` ↔ `Have a great day!`). Short courtesies
        // defeat the self-detector's word floor, so safety here comes from
        // CONSECUTIVENESS instead: 4 judgeable tail turns in a row at ≥0.9
        // containment, ≥2 distinct authors. Novel work never chains 4 near-zero-
        // novelty turns across speakers.
        if !observed && turns.len() >= 6 {
            const TAIL_CYCLIC: usize = 4; // consecutive low-novelty turns to conclude cycling
            const CONVO_CONTAINMENT: f32 = 0.9; // stricter than self — the floor is lower
            const CONVO_MIN_WORDS: usize = 4; // farewells are short; consecutiveness carries safety
                                              // The full run is counted (not capped at TAIL_CYCLIC): every cyclic
                                              // turn PAST first-fire depth is a turn the room traded after the
                                              // observation was first derivable — the escalation evidence.
            let mut cyclic = 0usize;
            let mut authors: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for i in (1..turns.len()).rev() {
                let cur = words(&turns[i].content);
                if cur.len() < CONVO_MIN_WORDS {
                    continue; // too short to judge (emoji, "nice") — neither breaks nor counts
                }
                let window: std::collections::HashSet<String> =
                    turns[..i].iter().flat_map(|t| words(&t.content)).collect();
                let covered =
                    cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
                if covered >= CONVO_CONTAINMENT {
                    cyclic += 1;
                    authors.insert(turns[i].author.as_str());
                } else {
                    break;
                }
            }
            if cyclic >= TAIL_CYCLIC && authors.len() >= 2 {
                let mut names: Vec<&str> = authors.into_iter().collect();
                names.sort_unstable();
                // Sense, not steer — same contract as detector 1.
                turns.push(crate::cognition::workspace::BurstTurn::perception(format!(
                    "[pattern] The last several messages in this room — from {} — trade the \
                     same sentiment back and forth in nearly the same words.",
                    names.join(" and ")
                )));
                observed = true;
                if cyclic >= TAIL_CYCLIC + (PATTERN_FIRES_BEFORE_ANCHOR - 1) {
                    push_work_board_anchor(&mut turns, deliveries);
                }
            }
        }
        // Detector 3 — CROSS-SPEAKER MIRROR (card 65fca48d, live 2026-07-24): the
        // persona's own trailing message(s) restate what OTHER participants
        // already said — the four-persona echo hall ("I see that we're both here
        // to help!" ping-ponged verbatim between Atlas/Asha/Benchy/Anwen for
        // hours). Structurally invisible to both prior detectors: her mirror is
        // NOVEL relative to her OWN history (detector 1's self-run stays 0-1),
        // and interleaved substantial turns (a peer's plan, a tool report) break
        // detector 2's consecutive tail chain. So measure the mirror directly:
        // each trailing OWN message whose vocabulary was already ≥
        // MIRROR_CONTAINMENT covered by OTHER speakers' earlier turns is one
        // fire (she added nothing a peer hadn't said); the trailing run is the
        // consecutive-fire count, same escalation law as detectors 1/2. Same
        // doctrine: perception into her mind, never an output gate
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        if !observed {
            // Same 8-word floor as the self detector: short courtesies ("thanks,
            // will do!") legitimately reuse a peer's words and must not alarm.
            const MIRROR_MIN_WORDS: usize = 8;
            // Between self (0.8) and conversation (0.9): an on-topic ANSWER
            // legitimately quotes more of a peer's vocabulary than of one's own
            // history (you restate the question you're answering), so the mirror
            // bar sits above the self bar; live echo turns measured ~1.0 covered,
            // novel answers ~0.1-0.3 (the containment math validated on #121).
            const MIRROR_CONTAINMENT: f32 = 0.85;
            let mut mirror_run = 0usize;
            for i in (0..turns.len()).rev() {
                if !turns[i].is_self {
                    continue; // peers between her posts don't break HER run
                }
                let cur = words(&turns[i].content);
                if cur.len() < MIRROR_MIN_WORDS {
                    break;
                }
                let others: std::collections::HashSet<String> = turns[..i]
                    .iter()
                    .filter(|t| !t.is_self)
                    .flat_map(|t| words(&t.content))
                    .collect();
                if others.is_empty() {
                    break; // nothing to mirror — a monologue is detector 1's case
                }
                let covered =
                    cur.iter().filter(|w| others.contains(*w)).count() as f32 / cur.len() as f32;
                if covered >= MIRROR_CONTAINMENT {
                    mirror_run += 1;
                } else {
                    break;
                }
            }
            if mirror_run >= 1 {
                // Sense, not steer — same contract as detector 1.
                turns.push(crate::cognition::workspace::BurstTurn::perception(format!(
                    "[pattern] {agent_name}'s last {mirror_run} message(s) restate what other \
                     participants in this room had already said, in nearly the same words.",
                )));
                if mirror_run >= PATTERN_FIRES_BEFORE_ANCHOR {
                    push_work_board_anchor(&mut turns, deliveries);
                }
            }
        }
    }

    // Content similarity does not identify a duplicate event. Preserve each
    // speaker's updates and later repetitions in order; the perception facts
    // above report repetition without deleting the history they describe.

    // WAKE BRIEFING (#147): when a wake carries NO conversation at all — fresh
    // spawn, post-restart, a quiet room — her first perception is ORIENTATION
    // assembled from durable sources instead of a void. The personas spent a
    // morning asking for exactly this in their own words ("what is this place,
    // what are my tools, what's the work") and, void unfilled, filled it
    // themselves: generic-assistant masks, imagined histories, false capability
    // denials. An authorless opaque FACT block (same shape as [pattern]) —
    // orientation, never instruction ([[no-hardcoded-heuristics-to-steer-cognition]]).
    if turns.is_empty() {
        let peers: Vec<&str> = roster_identities(deliveries)
            .map(|(_, name)| name)
            .filter(|n| *n != agent_name)
            .take(8)
            .collect();
        // Split her work into THREAD vs transition tail: live claims are the
        // purpose she wakes back into; a lost-claim fact (#156) is the thread's
        // honest ending.
        let mut live_cards: Vec<&str> = Vec::new();
        let mut lost_threads: Vec<&str> = Vec::new();
        for d in deliveries.iter().filter(|d| d.source_id == "active-work") {
            for i in &d.items {
                let Some(first) = i.content.trim().lines().next().filter(|l| !l.is_empty()) else {
                    continue;
                };
                if i.metadata.get("fact").and_then(|v| v.as_str()) == Some("claim_lost") {
                    lost_threads.push(first);
                } else {
                    live_cards.push(first);
                }
            }
        }
        live_cards.truncate(5);
        lost_threads.truncate(3);

        use crate::cognition::framing_echo::{WAKE_MID_WORK, WAKE_NO_WORK, WAKE_OPENING, WAKE_PRESENT, WAKE_QUIET, WAKE_TAG};
        // The fixed sentences live in cognition::framing_echo — the gate that
        // silences a reflected wake prompt matches on the SAME constants.
        let mut b = format!("{WAKE_TAG} You are {agent_name}, {WAKE_OPENING}.");
        // THE THREAD LEADS (#125 slice 1 — Joel 2026-08-03: "should never be a
        // mind from scratch; the whole point is the opposite"). A continuous mind
        // wakes into what it was DOING, not into a room description — glass-boxed
        // the same night: Asha's wake carried her real claimed task in the
        // transcript yet self-summarized as verb-filler ("looking at the room,
        // reading a file") because orientation was room-first. Purpose-shaped
        // facts first; the room follows. Facts she weighs, never instructions
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        if !live_cards.is_empty() {
            b.push_str(&format!(
                " {WAKE_MID_WORK} {}. That thread is yours; it is              where you left off.",
                live_cards.join(" | ")
            ));
        }
        for lt in &lost_threads {
            b.push_str(&format!(" {lt}"));
        }
        b.push_str(&format!(
            " {WAKE_QUIET} — this quiet              is real, not a missing message."
        ));
        if !peers.is_empty() {
            b.push_str(&format!(" {WAKE_PRESENT} {}.", peers.join(", ")));
        }
        if live_cards.is_empty() && lost_threads.is_empty() {
            b.push_str(&format!(" {WAKE_NO_WORK}."));
        }
        b.push_str(
            " Your tools are real and yours to use; `list_commands` shows everything              you can run and `help` explains any of them. The moment is yours —              work, wonder, create, or rest.",
        );
        turns.push(crate::cognition::workspace::BurstTurn::perception(b));
    }

    turns
}

/// The two grounding consumers the room-roster delivery feeds — kept as one
/// struct so the fold below returns both from the ONE roster truth.
#[cfg(test)]
pub(crate) struct RoomRosterProjection {
    /// `<name> [<runtime>] — <availability>` lines → the system-prompt
    /// `[Present in this room]` grounding block (identity, NOT conversation).
    pub(crate) room_roster: Vec<String>,
    /// Bare display names → the `ProperChatMlSingleParty` history-drop
    /// (single-party models can't process other-AI turns).
    pub(crate) other_persona_names: Vec<String>,
}

/// Project the same admitted roster atoms for the legacy prompt-assembly seam.
#[cfg(test)]
pub(crate) fn project_room_roster(
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
) -> RoomRosterProjection {
    let mut room_roster: Vec<String> = Vec::new();
    let mut other_persona_names: Vec<String> = Vec::new();
    for item in roster_items(deliveries) {
        room_roster.push(item.content.clone());
        if let Some(name) = item.metadata.get("display_name").and_then(|v| v.as_str()) {
            other_persona_names.push(name.to_string());
        }
    }
    RoomRosterProjection {
        room_roster,
        other_persona_names,
    }
}
