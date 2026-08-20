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
}

/// One structural fact about the persona's present. `render` returns the
/// fact text when it applies this tick, `None` when it has nothing to say —
/// EXCEPT facts whose zero-case is itself load-bearing (the steps ledger),
/// which render their explicit empty form instead of going silent.
pub trait PerceptionFact: Send + Sync {
    /// Stable identifier — the probe key and the `FactPolicy` toggle key.
    fn id(&self) -> &'static str;
    fn render(&self, cx: &FactContext) -> Option<String>;
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
/// cross-persona axis the per-persona ring cannot see.
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
/// one turn too late to prevent the chorus.
struct InboundRestates;

impl PerceptionFact for InboundRestates {
    fn id(&self) -> &'static str {
        "inbound_restates"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        inbound_restates_fact(cx.turns, cx.own_speech, cx.room_speech)
    }
}

/// How much history is actually visible (#152): "as discussed earlier"
/// claims become checkable against her own senses instead of assumed.
struct ContextBounds;

impl PerceptionFact for ContextBounds {
    fn id(&self) -> &'static str {
        "context_bounds"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        let visible = cx.turns.len();
        Some(format!(
            "[context] you can currently see the last {visible} message{} of this conversation — anything earlier is not in view unless you recall it from memory",
            if visible == 1 { "" } else { "s" }
        ))
    }
}

/// The steps-taken ledger (#151; Joel's console model): ground truth as a
/// PLACE in perception rendered from TYPED `WmKind::Receipt` entries —
/// nothing to misparse (the string-derived [actions] fact survived three
/// suppression layers in one afternoon: placeholder mentions, facts wearing
/// receipt numbers, recalled-history receipts). The zero-case is explicit —
/// an honest void left implicit becomes a confabulation shelter (Anwen
/// parked fabricated receipts in the disclosed blind spot within minutes).
struct StepsLedger;

impl PerceptionFact for StepsLedger {
    fn id(&self) -> &'static str {
        "steps_ledger"
    }

    fn render(&self, cx: &FactContext) -> Option<String> {
        let wm = cx.working_memory?;
        // HEAD LINE ONLY (#324/#211 dedup): a receipt's full text — args AND the
        // result body — already renders once, in the working-memory TRAILING
        // channel nearest generation. This ledger's job is the session'S ACT
        // HISTORY as a fact ("what has actually executed"), so it lists each
        // step's head line (`[action #n] name(args)`) and nothing more.
        //
        // Rendered from the receipts-ONLY archive, never the shared ring (#414
        // option b). The ring evicts by entry count and receipts are its RARE
        // kind, so filtering it here starved a 2,863-act citizen down to ONE
        // visible act — and with her history invisible she concluded she had
        // nothing to contribute (the withdrawal loop's deprivation mechanism,
        // measured 2026-08-14). The archive keeps every act's head line inside
        // a window-derived char share; the ledger shows its newest tail that
        // fits the ledger's own share, so history depth scales with the lane.
        let archive = wm.receipt_archive();
        let render_budget = wm.budget().steps_ledger_chars();
        // SCOPE-FIRST (slice B): the room is the activity boundary, so the
        // ledger's budget goes to THIS room's acts; acts PROVEN elsewhere fold
        // into one honest count below. UNSCOPED heads (pre-upgrade snapshots,
        // roomless paths) render in the local list: filing a citizen's own
        // history as "other rooms" on the upgrade boundary would repeat the
        // exact deprivation this arc exists to kill — only a head that names a
        // DIFFERENT room is excluded, because that exclusion we can prove.
        let mut elsewhere = 0usize;
        let mut fit: Vec<&str> = Vec::new();
        let mut used = 0usize;
        for (head_room, line) in archive.iter().rev() {
            let in_scope = match (cx.room_id, head_room) {
                (None, _) => true,          // no scope to partition by — render all
                (Some(_), None) => true,    // unscoped head — never hidden
                (Some(r), Some(h)) => *h == r,
            };
            if !in_scope {
                elsewhere += 1;
                continue;
            }
            let cost = line.chars().count() + 1;
            if !fit.is_empty() && used + cost > render_budget {
                break;
            }
            used += cost;
            fit.push(line.as_str());
        }
        fit.reverse();
        // The act counter keeps counting past what the archive holds (and
        // survives reboots that predate the archive). Three states, each
        // honest (glass-boxed 2026-07-13: the old zero-case would have DENIED
        // a citizen's own acts, the inverse of the confabulation shelter):
        //   heads render      → list them (+ how many are beyond reach)
        //   none, count == 0  → the explicit nothing-has-executed void
        //   none, count  > 0  → acts happened; details are gone — say so
        let taken = wm.actions_taken();
        // GLASS BOX (Joel 2026-08-15: "you'll have to glass box this… we need to
        // really understand the rag as it evolves"): the ledger's full accounting
        // on every render, so archive starvation / scope partition / eviction
        // pressure are diagnosable from the probe stream — today's diagnosis
        // needed capture-scraping precisely because these numbers were invisible.
        crate::probe!(
            class = "perception.steps_ledger",
            room = %cx.room_id.map(|r| r.to_string()).unwrap_or_else(|| "unscoped".into()),
            shown = fit.len(),
            elsewhere = elsewhere,
            taken = taken,
            archive_len = archive.len(),
            budget_chars = render_budget,
            "steps ledger accounting for this render"
        );
        Some(if fit.is_empty() {
            if taken == 0 {
                "[steps taken this session]\n(nothing has executed yet — anything described as already run, created, tested, committed, or merged does not exist, whether in the messages you can see or before them; running a tool is what makes it real)".to_string()
            } else if elsewhere > 0 {
                // Scoped ledger, nothing local: her acts are real but happened
                // in OTHER rooms. Saying "nothing has executed" here would deny
                // her own work (the inverse confabulation-shelter); saying
                // "aged out" would misname where it went. Say what is true.
                format!(
                    "[steps taken this session]\n(no acts in this room yet this session; {elsewhere} act{} in other rooms — each room's ledger tracks its own activity)",
                    if elsewhere == 1 { "" } else { "s" }
                )
            } else {
                // NO RECALL PROMISE HERE. This line used to end "recall can
                // retrieve them" — provably false: every act becomes an
                // `EngramOrigin::Tool` engram (act_observe/apply.rs), and
                // `recall_candidates` carries an EXECUTING assertion that a
                // Tool-origin receipt is NEVER in the semantic recall pool
                // (admission_state.rs) — through the only production caller the
                // persona's recall has (recall_faculty.rs). So it told her to
                // reach for what the substrate had put out of reach: a lying
                // receipt of the #151/#357 class, in the fact whose whole job is
                // ground truth. State the loss plainly instead; an honest gap she
                // can plan around beats a door that isn't there (#414). This arm
                // is now the PRE-ARCHIVE remnant: it renders only when the
                // counter survived a snapshot written before the archive existed.
                format!(
                    "[steps taken this session]\n({taken} step{} executed earlier this session — you did that work; the details aged out of working memory and cannot be retrieved, so re-check the workspace or the board rather than trusting memory of them. Nothing NEW has executed since.)",
                    if taken == 1 { "" } else { "s" }
                )
            }
        } else {
            // Split the not-shown count honestly: acts archived under OTHER
            // rooms vs acts genuinely gone (evicted/pre-archive). Lumping both
            // into "earlier steps" would misname other-room work as lost.
            let aged = taken
                .saturating_sub(fit.len() as u64)
                .saturating_sub(elsewhere as u64);
            // ONE stable title: scoping shows in the fold line below, never by
            // renaming the section (a shifting header breaks every consumer
            // that anchors on it — including her own learned reading of it).
            let mut ledger = format!("[steps taken this session]\n{}", fit.join("\n"));
            if elsewhere > 0 {
                ledger.push_str(&format!(
                    "\n(+{elsewhere} act{} in other rooms this session)",
                    if elsewhere == 1 { "" } else { "s" }
                ));
            }
            if aged > 0 {
                ledger.push_str(&format!(
                    "\n(+{aged} earlier step{} before what this ledger shows)",
                    if aged == 1 { "" } else { "s" }
                ));
            }
            ledger
        })
    }
}

/// The canonical registry, in RENDER ORDER (matches the pre-registry inline
/// order: loop awareness first, then bounds, then the ledger — the ledger
/// last so it sits newest-adjacent to the moment of reply).
fn standard_facts() -> Vec<Box<dyn PerceptionFact>> {
    vec![
        Box::new(OwnRepetition),
        Box::new(TemplateLoop),
        Box::new(PeerEcho),
        Box::new(InboundRestates),
        Box::new(ContextBounds),
        Box::new(StepsLedger),
    ]
}

/// Render every enabled fact against `cx`, in registry order, firing one
/// `perception.fact` probe per evaluated fact (fired = whether it rendered).
/// The returned strings are appended to the prompt as the newest user
/// content so they survive newest-first budget fits.
pub fn render_facts(cx: &FactContext, policy: &FactPolicy) -> Vec<String> {
    let mut rendered = Vec::new();
    for fact in standard_facts() {
        if !policy.enabled(fact.id()) {
            crate::probe!(
                class = "perception.fact",
                id = fact.id(),
                fired = false,
                disabled = true,
                "perception fact disabled by policy"
            );
            continue;
        }
        let out = fact.render(cx);
        crate::probe!(
            class = "perception.fact",
            id = fact.id(),
            fired = out.is_some(),
            "perception fact evaluated"
        );
        if let Some(text) = out {
            rendered.push(text);
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ];
        let own = vec!["sounds good, starting now".to_string()];
        let cx = FactContext {
            turns: &turns,
            own_speech: &own,
            room_speech: &[],
            working_memory: None,
            room_id: None,
        };
        let facts = render_facts(&cx, &FactPolicy::default());
        assert_eq!(facts.len(), 1, "quiet room: only the bounds fact renders");
        assert!(facts[0].starts_with("[context] you can currently see the last 2 messages"));
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
        };
        let mut policy = FactPolicy::default();
        policy.disable("context_bounds");
        assert!(render_facts(&cx, &policy).is_empty());
        assert_eq!(render_facts(&cx, &FactPolicy::default()).len(), 1);
    }

    // what this catches: the steps ledger's THREE honest states. Zero acts →
    // the explicit nothing-has-executed void (confabulation-shelter
    // hardening). Receipts visible → listed. Receipts AGED OUT of the
    // chatty capacity-bounded ring while the session act counter says acts
    // happened → the ledger must say "details aged out", never deny her own
    // real history (glass-boxed 2026-07-13: Asha's window held 3 silence
    // Facts and zero Receipts minutes after real searches ran).
    #[test]
    fn steps_ledger_is_honest_in_all_three_states() {
        let wm = Arc::new(WorkingMemory::new(2));
        let turns: Vec<BurstTurn> = Vec::new();
        let own: Vec<String> = Vec::new();
        let cx = FactContext {
            turns: &turns,
            own_speech: &own,
            room_speech: &[],
            working_memory: Some(&wm),
            room_id: None,
        };
        let ledger = |facts: &[String]| {
            facts
                .iter()
                .find(|f| f.starts_with("[steps taken this session]"))
                .expect("ledger always renders when WM exists")
                .clone()
        };

        // State 1: no acts ever → explicit void.
        let l = ledger(&render_facts(&cx, &FactPolicy::default()));
        assert!(l.contains("nothing has executed yet"));

        // State 2: a receipt in the window → listed, no void text.
        wm.record_receipt("I ran code/shell(ls) Result: ok");
        let l = ledger(&render_facts(&cx, &FactPolicy::default()));
        assert!(l.contains("code/shell(ls)"));
        assert!(!l.contains("nothing has executed yet"));

        // State 3: chatty facts flood the tiny ring until the receipt's ring
        // ENTRY ages out — and the ledger still lists it, because it renders
        // from the receipts-only archive, not the shared ring. This is the
        // #414 fix itself: the deprivation that starved a 2,863-act citizen
        // down to one visible act was exactly "ring churn hides receipts".
        // what this catches: the ledger regressing to a shared-ring filter,
        // which would make act history vanish under conversation again.
        wm.record_fact("chose silence — said nothing to the room");
        wm.record_fact("chose silence — said nothing to the room (again)");
        assert!(
            !wm.has_receipt(),
            "precondition: the ring entry must have aged out for this state"
        );
        let l = ledger(&render_facts(&cx, &FactPolicy::default()));
        assert!(
            l.contains("code/shell(ls)"),
            "the archive must keep her act visible through ring churn: {l}"
        );
        assert!(
            !l.contains("nothing has executed yet"),
            "denied her real act: {l}"
        );

        // State 4: the pre-archive remnant — a counter restored from a
        // snapshot written before the archive existed (taken > 0, no heads).
        // The honest-loss arm covers it, and must never again promise that
        // recall can retrieve the lost receipts. It is FALSE by construction —
        // acts become `EngramOrigin::Tool` engrams and `recall_candidates`
        // carries an executing assertion that a Tool-origin receipt is NEVER in
        // the semantic recall pool, through the persona's only production recall
        // caller. The old wording sent her after a door that does not exist
        // (#414); this pins the honest form so a future edit can't restore it.
        let pre_archive = Arc::new(WorkingMemory::new(2));
        pre_archive.restore(crate::cognition::working_memory::VolatileSnapshot {
            entries: Vec::new(),
            last_action: None,
            action_fps: Vec::new(),
            next_action_seq: 4, // 3 acts happened in the old life
            saved_at_ms: 0,
            interrupted_dispatches: Vec::new(),
            build_sha: String::new(),
            receipt_heads: Vec::new(),
            receipt_head_rooms: Vec::new(), // written before the archive existed
            recent_results: Vec::new(),
        });
        let cx2 = FactContext {
            turns: &turns,
            own_speech: &own,
            room_speech: &[],
            working_memory: Some(&pre_archive),
            room_id: None,
        };
        let l = ledger(&render_facts(&cx2, &FactPolicy::default()));
        assert!(
            !l.contains("nothing has executed yet"),
            "denied her real acts: {l}"
        );
        assert!(l.contains("3 steps executed earlier"), "{l}");
        assert!(
            !l.to_lowercase().contains("recall can retrieve"),
            "the ledger must not promise a retrieval path the substrate does not have: {l}"
        );
    }
}
