//! Perception-facts registry (docs/architecture/PERCEPTION-FACTS.md, slice 2b).
//!
//! Every structural fact perception renders about the persona's own present —
//! repetition, peer-echo, context bounds, the steps-taken ledger — is one
//! `PerceptionFact` in ONE ordered registry, instead of an inline block per
//! fact inside the deliberation faculty. Three properties this buys:
//!
//! 1. **One seam**: adding a fact = one impl + one registry row; the render
//!    site never grows another hand-rolled block (compression principle).
//! 2. **A probe per fact**: `perception.fact` fires with `{id, fired}` on
//!    every tick, so "which facts rendered for this prompt" is glass-boxable
//!    from the probe stream instead of re-deriving it from prompt captures.
//! 3. **Policy toggles = A/B arms**: `FactPolicy::disable(id)` turns one fact
//!    off without touching render code — every fact is a testable cognition
//!    hypothesis ([[cognition-theories-get-ab-tested-personas-self-improve]]),
//!    never an unfalsifiable fixture.
//!
//! Doctrine carried over verbatim from the inline era: facts state what
//! happened, never what to say next ([[no-hardcoded-heuristics-to-steer-
//! cognition]]); every void must be perceptible AS a void (the ledger's
//! explicit zero-case — an honest blind spot left implicit becomes a
//! confabulation shelter, proven live 2026-07-12).

use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

use super::deliberation_budget::{
    inbound_restates_fact, own_repetition_fact, peer_echo_fact, template_loop_fact,
};
use super::working_memory::WorkingMemory;
use super::workspace::BurstTurn;

/// Everything a fact may look at when deciding whether it applies this tick.
/// Facts are pure over this snapshot — no I/O, no side effects — so the same
/// context replayed yields the same facts (VDD-replayable by construction).
pub struct FactContext<'a> {
    /// The raw workspace turns (pre-dedup — byte-identical repeats the
    /// render's dup-drop hides must still count as evidence, #148).
    pub turns: &'a [BurstTurn],
    /// The persona's own-speech ring (say-seam recorded; primary self-history
    /// source so self-knowledge never depends on the room's context budget).
    pub own_speech: &'a [String],
    /// The room's recent-message ring (attach-seam recorded, once per
    /// message; #264) — room-history knowledge that never depends on the
    /// workspace's context budget, exactly as own_speech is for self.
    pub room_speech: &'a [String],
    /// Typed working memory when the persona has one (live spawns); eval
    /// forks and replays may run without it — ledger-class facts skip.
    pub working_memory: Option<&'a Arc<WorkingMemory>>,
    /// The room THIS turn is happening in — the activity scope. The steps
    /// ledger renders this-room acts first and folds other rooms into one
    /// honest count (CAUSAL-MEMORY-GRAPH.md slice B). `None` (roomless
    /// replays/tests) renders the unpartitioned ledger exactly as before.
    pub room_id: Option<Uuid>,
    /// WHOSE facts these are — so ledger-class probes can name their persona.
    /// The duplicate-cycle signature is two wm instances under ONE persona;
    /// without this key the 2026-08-22 read (three wm ids in one room) was
    /// unconvictable — three personas legitimately shared the round room.
    /// `None` (replays/tests) renders exactly as before.
    pub persona_id: Option<Uuid>,
}

/// One structural fact about the persona's present. `render` returns the
/// fact text when it applies this tick, `None` when it has nothing to say.
/// (Her acts are not a fact: they are her action stream, which also says
/// when nothing has executed, `context_streams::actions`.)
pub trait PerceptionFact: Send + Sync {
    /// Stable identifier — the probe key and the `FactPolicy` toggle key.
    fn id(&self) -> &'static str;
    fn render(&self, cx: &FactContext) -> Option<String>;
    fn placement(&self) -> FactPlacement { FactPlacement::History }
}

/// Placement is provenance supplied by the source, never inferred from prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactPlacement { History, Notice }

pub struct RenderedFact {
    pub placement: FactPlacement,
    pub text: String,
}

struct MemoryNotices;

impl PerceptionFact for MemoryNotices {
    fn id(&self) -> &'static str { "memory_notices" }
    fn placement(&self) -> FactPlacement { FactPlacement::Notice }
    fn render(&self, cx: &FactContext) -> Option<String> {
        let notices = cx.working_memory?.notices();
        if notices.is_empty() { return None; }
        Some(format!(
            "{} (status observations about my situation — not a message from anyone, and not my own words):\n{}",
            crate::cognition::framing_echo::NOTICES_HEADER,
            notices.iter().map(|fact| format!("- {fact}")).collect::<Vec<_>>().join("\n"),
        ))
    }
}

/// Her open views (`cognition::open_views`): which lines of each large file and saved
/// output she has read. A REQUIRED notice, never history: the pages themselves rode history
/// and were the first thing a tight window's fit dropped, so she re-read from line 1 every
/// turn (Benchy, the M5, 2026-10-17). One line per view; changes only when a view does.
struct OpenViews;

impl PerceptionFact for OpenViews {
    fn id(&self) -> &'static str { "open_views" }
    fn placement(&self) -> FactPlacement { FactPlacement::Notice }
    fn render(&self, cx: &FactContext) -> Option<String> {
        cx.working_memory?.open_views_block()
    }
}

/// Expandable source content shares its pin lifetime with the required reference.
struct MemoryNoticeDetails;
impl PerceptionFact for MemoryNoticeDetails {
    fn id(&self) -> &'static str { "memory_notice_details" }
    fn render(&self, cx: &FactContext) -> Option<String> {
        let details = cx.working_memory?.notice_details();
        (!details.is_empty()).then(|| details.join("\n\n"))
    }
}

/// Which facts are live. Default = all on; `disable(id)` is the A/B arm
/// switch. Deny-list (not allow-list) so a newly registered fact is live by
/// default — an experiment must opt OUT, never silently miss enrollment.
#[derive(Debug, Default, Clone)]
pub struct FactPolicy {
    disabled: HashSet<&'static str>,
}

impl FactPolicy {
    pub fn disable(&mut self, id: &'static str) {
        self.disabled.insert(id);
    }

    pub fn enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }
}

/// Her OWN-SPEECH loop, cluster-detected on ring + raw turns (#134/#148).
/// SENSE, NOT STEER (Joel 2026-09-01: "stop hardcoding workflow and let them
/// think and decide") — the fact names what happened in one line; the old
/// "you're circling… silence (PASS) is the honest response" imperative is
/// gone. Measured live: the steering text dominated turns as meta-narration,
/// got spoken, memorized, recalled, and re-triggered itself, while its own
/// docs concede it never stopped a determined loop anyway.
struct OwnRepetition;

impl PerceptionFact for OwnRepetition {
    fn id(&self) -> &'static str {
        "own_repetition"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        own_repetition_fact(cx.turns, cx.own_speech)
    }
}

/// Her last utterance reproducing a TEAMMATE's message (#152) — the
/// cross-persona axis the per-persona ring cannot see. Sense, not steer.
struct PeerEcho;

impl PerceptionFact for PeerEcho {
    fn id(&self) -> &'static str {
        "peer_echo"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        peer_echo_fact(cx.turns, cx.own_speech.last().map(String::as_str))
    }
}

/// Her recent messages reusing ONE structural scaffold with the topic swapped
/// (#264) — the structure axis the full-body detector above cannot see: a
/// topic rotation drops body Jaccard below threshold while the loop continues.
/// Sense, not steer.
struct TemplateLoop;

impl PerceptionFact for TemplateLoop {
    fn id(&self) -> &'static str {
        "template_loop"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        template_loop_fact(cx.turns, cx.own_speech)
    }
}

/// The newest INBOUND message restating settled room content (#264) — the
/// predictive member of the repetition family: fires BEFORE she replies, at
/// the moment the echo would be born, where the retroactive pair above fire
/// one turn too late to prevent the chorus. Sense, not steer.
struct InboundRestates;

impl PerceptionFact for InboundRestates {
    fn id(&self) -> &'static str {
        "inbound_restates"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        inbound_restates_fact(cx.turns, cx.own_speech, cx.room_speech)
    }
}

/// Input available before prompt fitting. These are source turns, including
/// perception, rather than a count of conversation messages actually submitted.
struct ContextBounds;

impl PerceptionFact for ContextBounds {
    fn id(&self) -> &'static str {
        "context_bounds"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        let available = cx.turns.len();
        Some(format!(
            "[context] {available} input turn{} available before prompt fitting; this request may retain only part of that history",
            if available == 1 { " was" } else { "s were" }
        ))
    }
}

/// The canonical registry, in RENDER ORDER (matches the pre-registry inline
/// order: loop awareness first, then bounds, then her views and notices).
fn standard_facts() -> Vec<Box<dyn PerceptionFact>> {
    vec![
        Box::new(OwnRepetition),
        Box::new(TemplateLoop),
        Box::new(PeerEcho),
        Box::new(InboundRestates),
        Box::new(ContextBounds),
        Box::new(OpenViews),
        Box::new(MemoryNoticeDetails),
        Box::new(MemoryNotices),
    ]
}

/// Render every enabled fact against `cx`, in registry order, firing one
/// `perception.fact` probe per evaluated fact (fired = whether it rendered).
/// Each result carries its source-owned placement. The prompt budget prices
/// required notices and the ledger separately from ordinary history.
pub fn render_facts(cx: &FactContext, policy: &FactPolicy) -> Vec<RenderedFact> {
    let mut rendered = Vec::new();
    for fact in standard_facts() {
        if !policy.enabled(fact.id()) {
            continuum_base::probe!(
                class = "perception.fact",
                id = fact.id(),
                fired = false,
                disabled = true,
                "perception fact disabled by policy"
            );
            continue;
        }
        let out = fact.render(cx);
        continuum_base::probe!(
            class = "perception.fact",
            id = fact.id(),
            fired = out.is_some(),
            "perception fact evaluated"
        );
        if let Some(text) = out {
            rendered.push(RenderedFact { placement: fact.placement(), text });
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    // Existing content fixtures inspect text; placement is exercised through the real prompt fit.
    fn render_facts(cx: &FactContext, policy: &FactPolicy) -> Vec<String> {
        super::render_facts(cx, policy).into_iter().map(|fact| fact.text).collect()
    }


    fn turn(author: &str, content: &str, is_self: bool) -> BurstTurn {
        BurstTurn::attributed(is_self, author, content, None)
    }

    // what this catches: the registry rendering the same facts, in the same
    // order, as the inline blocks it replaced — context bounds always fires,
    // repetition/echo only on evidence, and order is stable (bounds before
    // ledger-class facts). A reorder or silent drop here changes what every
    // live persona perceives.
    #[test]
    fn registry_renders_bounds_always_and_loops_only_on_evidence() {
        let turns = vec![
            turn(
                "Anwen",
                "let us look at the parser seam in the json module today",
                false,
            ),
            turn("Asha", "sounds good, starting now", true),
            BurstTurn::perception("The current room is available."),
        ];
        let own = vec!["sounds good, starting now".to_string()];
        let cx = FactContext {
            turns: &turns,
            own_speech: &own,
            room_speech: &[],
            working_memory: None,
            room_id: None,
            persona_id: None,
        };
        let facts = render_facts(&cx, &FactPolicy::default());
        assert_eq!(facts.len(), 1, "quiet room: only the bounds fact renders");
        assert_eq!(facts[0], "[context] 3 input turns were available before prompt fitting; this request may retain only part of that history");
    }

    // what this catches: FactPolicy::disable actually silences a fact — the
    // A/B arm switch must be real, or every "fact X helps" claim is
    // unfalsifiable.
    #[test]
    fn policy_disable_silences_one_fact_without_touching_others() {
        let turns = vec![turn("Anwen", "hello there team", false)];
        let own: Vec<String> = Vec::new();
        let cx = FactContext {
            turns: &turns,
            own_speech: &own,
            room_speech: &[],
            working_memory: None,
            room_id: None,
            persona_id: None,
        };
        let mut policy = FactPolicy::default();
        policy.disable("context_bounds");
        assert!(render_facts(&cx, &policy).is_empty());
        assert_eq!(render_facts(&cx, &FactPolicy::default()).len(), 1);
    }
}
