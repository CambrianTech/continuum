//! The deliberation system prompt, composed PROCEDURALLY from named blocks.
//!
//! This is the framing the [`super::llm_deliberation_faculty::LlmDeliberationFaculty`]
//! wraps around a persona's identity + assembled RAG context before every turn. It
//! used to be a single monolithic `compose_system` method — a wall of inline block
//! literals interleaved with `if` gates. That made two things hard: the faculty file
//! carried ~180 lines of prose it didn't structurally own, and "what blocks exist, in
//! what order, gated how" was buried in control flow instead of stated as data.
//!
//! Here the prompt is an ORDERED LIST of blocks ([`ordered_blocks`]): each block is a
//! named builder (a `const` for static prose, an `fn` for the ones that interpolate),
//! paired with the condition under which it appears this turn. [`compose`] is just the
//! fold that concatenates the present blocks. Adding a new framing block (a foraging
//! block once hands land, a recipe-injected doctrine header) is ONE new entry in
//! `ordered_blocks` — the assembler never changes, and the ordering/gating stays
//! readable at a glance.
//!
//! ## Ordering is load-bearing (KV-cache prefix reuse, measured 2026-06-23; tool
//! surface reworked 2026-07-01)
//!
//! The byte-identical cross-turn prefix is the identity + `[Taking your turn]` block.
//! Everything from `[Your tools]` down is re-prefilled each turn — the tool menu is an
//! EXPANDABLE BOOKMARKED MENU whose per-category expansion is chosen for what the
//! persona is doing THIS turn ([`super::llm_deliberation_faculty`] computes the
//! `expanded` set), so it is per-turn volatile. It deliberately trades a byte-stable
//! ~1.8k-token catalog that rode every prefill for a ~0.4k-token menu that varies
//! (Joel 2026-06-29, [[adaptive-tool-surface-meets-you-in-the-middle]]; the flip is a
//! net prefill cut when the stable prefix is NOT reused across turns — the observed
//! regime, slot churn across ~14 personas on `--parallel 4`). The assembled `context`
//! stays LAST so the live situation is closest to the generation point (recency favors
//! instruction-following). Keep the identity/turn-taking prefix above the tool block
//! byte-stable; keep the volatile context at the tail.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::persona_tools;
use crate::ai::types::NativeToolSpec;
use crate::persona::prompt_assembly::SILENCE_AFFORDANCE_BLOCK;

/// Everything the system prompt is assembled from — the dynamic inputs the blocks
/// interpolate and the structural flags they gate on, in ONE carrier so [`compose`]
/// is a pure function of data. `directed`/`self_initiated` are STRUCTURAL facts about
/// the turn (who addressed whom, what scheduled it), never a read of the persona's
/// output ([[no-hardcoded-heuristics-to-steer-cognition]]).
pub(super) struct SystemPromptParts<'a> {
    /// The persona's identity prompt — the byte-stable head of the cacheable prefix.
    pub system_prompt: &'a str,
    /// The persona's name, interpolated into `[Taking your turn]`.
    pub persona_name: &'a str,
    /// The persona's authorized tools; empty ⇒ no `[Your tools]`/`[Acting]` blocks.
    pub tools: &'a [NativeToolSpec],
    /// Categories to expand inline this turn (the rest render as collapsed bookmarks).
    pub expanded: &'a BTreeSet<String>,
    /// The context the mind assembled this tick (recall + who's present + situation).
    pub context: &'a str,
    /// A turn DIRECTED at her (question/@mention/DM) withholds the silent-PASS hatch.
    pub directed: bool,
    /// Wall-clock (or eval-pinned) NOW in ms — rendered as `[now …]` standing context
    /// at minute granularity (#125: without it appointments are words with no referent).
    pub now_ms: Option<u64>,
    /// A self-initiated (never-stop heartbeat) turn carries the own-time framing.
    pub self_initiated: bool,
    /// This persona currently HOLDS a live in-progress work claim — a structural
    /// claim-state fact (derived from the [active-work] grounding her own workspace
    /// assembled, via `active_work_source::renders_held_in_progress`; never a read of
    /// her output). On an UNDIRECTED turn it swaps the conversational-presence block
    /// for the working-presence contract: a quiet room stops reading as "nothing to
    /// do" when her claimed card is sitting in her workspace. Directed turns are
    /// unaffected (answering the person who named her still comes first).
    pub holds_live_work: bool,
}

/// The system prompt split at the KV-cache boundary (#266/#205). `stable` is a pure
/// function of the PERSONA (identity + turn-framing + tools) — byte-identical across all
/// of her turns, so it lands in the cacheable prefix llama-server reuses. `trailing` is the
/// PER-TURN situational framing (own-time/presence, assembled context, clock): same content,
/// same order as before, but delivered as the newest turn instead of baked into the system
/// message, so a per-turn flip (addressed↔self-initiated, a new context tail) can no longer
/// invalidate the whole prefix. Before the split every act re-prefilled ~6k tokens because
/// the presence block flipped at ~char 7.6k of the system prompt (0% KV reuse — the ~13min
/// SWE solves). See [`compose`] for the byte-identical concatenation.
pub(super) struct ComposedSystemPrompt {
    /// Persona-invariant prefix — safe to place in the cacheable system message.
    pub stable: String,
    /// Per-turn situational framing — the message builder appends it as the newest turn.
    pub trailing: String,
}

/// Assemble the system prompt split into its cacheable [`stable`](ComposedSystemPrompt::stable)
/// prefix and its per-turn [`trailing`](ComposedSystemPrompt::trailing) tail. The single
/// place the block set + order + gating lives, now partitioned by cache-stability.
pub(super) fn compose_split(p: &SystemPromptParts<'_>) -> ComposedSystemPrompt {
    let mut stable = String::with_capacity(p.system_prompt.len() + 768);
    for block in stable_blocks(p) {
        stable.push_str(&block);
    }
    let mut trailing = String::with_capacity(p.context.len() + 512);
    for block in volatile_blocks(p) {
        trailing.push_str(&block);
    }
    ComposedSystemPrompt { stable, trailing }
}

/// Assemble the WHOLE system prompt as one string — `stable ++ trailing`, byte-identical
/// to the pre-split output.
///
/// TEST-ONLY, and gated so it says so: production has exactly one composer,
/// [`compose_split`], which places `stable` in the cacheable system message and `trailing`
/// on the newest turn (the #266 KV-reuse fix). This exists because the assertions below
/// are about prompt CONTENT — what appears, in what order — which is easier to state
/// against the whole string than against the halves. It is not a second composer: it is
/// [`compose_split`]'s own output concatenated, so it cannot drift from it.
#[cfg(test)]
fn compose(p: &SystemPromptParts<'_>) -> String {
    let c = compose_split(p);
    let mut s = c.stable;
    s.push_str(&c.trailing);
    s
}

/// The BYTE-STABLE prefix blocks: identity + `[Taking your turn]` + tools/`[Acting]`.
/// A pure function of the persona (system prompt, name, authorized tool set) — invariant
/// across her turns, so it is the cacheable KV prefix. NOTHING situational belongs here.
fn stable_blocks<'a>(p: &'a SystemPromptParts<'a>) -> impl Iterator<Item = Cow<'a, str>> {
    [
        // Identity — always; the byte-stable head of the cacheable prefix.
        Some(Cow::Borrowed(p.system_prompt)),
        // Take a TURN in this activity — always; pure function of her name.
        Some(Cow::Owned(taking_your_turn_block(p.persona_name))),
        // Tools + acting — only when the persona has tools.
        tools_block(p.tools, p.expanded).map(Cow::Owned),
        // Assembled context — the standing grounding (roster/doctrine/workspace-map)
        // FIRST (stable-sorted inside the block), then whatever volatile tail survived the
        // fit. It stays in the cacheable system prefix by design: its stable head IS the
        // reusable KV, and the truly per-turn material (recall / working-memory traces) is
        // already separated out as its own `.trailing()` conversation turns upstream
        // (#205), so it never lands here. Keeping context in `stable` preserves the
        // "standing framing reaches the system message" contract
        // (`trailing_proprioception_renders_in_the_tail_not_the_system_prefix`).
        working_context_block(p.context).map(Cow::Owned),
        // Her NOW — a one-line clock (minute granularity), LAST in the stable prefix and
        // nearest the framing that follows: freshest temporal grounding at the write point.
        // Per-minute churn costs at most one re-prefill per minute (turns are seconds
        // apart), versus the per-TURN flip the framing below would cause if it stayed here.
        // Eval passes its pinned epoch; tests pass None.
        p.now_ms
            .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
            .map(|dt| {
                Cow::Owned(format!(
                    "\n\n[now {}]",
                    dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M %A")
                ))
            }),
    ]
    .into_iter()
    .flatten()
}

/// The PER-TURN situational FRAMING — the own-time affordance and the conversational-
/// presence block, whose variant (DIRECTED / WORKING / SILENCE) flips hard every turn.
/// This is the block the raw prompt-captures caught breaking the KV prefix at char ~7607
/// (#266): it sat BEFORE the context, so every turn's flip re-prefilled the whole tail.
/// It MUST NOT sit in the cacheable system prefix; the message builder renders it as the
/// newest trailing turn — same text, nearest generation (the #205 trailing placement),
/// where its volatility no longer invalidates the cached identity + tools + context prefix.
fn volatile_blocks<'a>(p: &'a SystemPromptParts<'a>) -> impl Iterator<Item = Cow<'a, str>> {
    [
        // Self-directed free time — only on a self-initiated heartbeat turn.
        p.self_initiated.then_some(Cow::Borrowed(OWN_TIME_BLOCK)),
        // Conversational presence — the AMBIENT block on undirected turns; the DIRECTED
        // variant when a message names her. Directed no longer strips her choice entirely:
        // never ghost a QUESTION (explicit in the block), but a pure appreciation/closing
        // pleasantry may rest — the natural spiral-break (two personas mutually name-
        // mentioning each other were each FORCED to reply, forever). Framing, not a gate.
        // A claim-holder's undirected turn takes the WORKING contract instead: her
        // in-progress card is the turn's purpose, not scenery (glass-boxed
        // 2026-08-07: four citizens with perfect windows yielded every ambient turn
        // under the conversational contract while acting fine under the eval
        // harness's work framing — the contract was the variable, not the model).
        Some(Cow::Borrowed(if p.directed {
            crate::persona::prompt_assembly::DIRECTED_PRESENCE_BLOCK
        } else if p.holds_live_work {
            crate::persona::prompt_assembly::WORKING_PRESENCE_BLOCK
        } else {
            SILENCE_AFFORDANCE_BLOCK
        })),
    ]
    .into_iter()
    .flatten()
}

/// `[Taking your turn]` — tell the reasoner it is taking a TURN in this activity, not
/// analyzing a transcript; otherwise small models outline the situation instead of
/// participating. The activity is NOT hardcoded (it is recipe-defined): the room's
/// operating doctrine in the context specializes HOW to participate (chat /
/// coordination / game / code / art / …). This block is SITUATION-AWARE and posture-
/// neutral: it says "take your turn as yourself — the contribution the moment calls for,
/// in full", where the contribution is words when the moment wants words and the finished
/// deliverable (a function, a design, a written piece) when the task asks for one. The
/// deliverable-truth is UNCONDITIONAL here (not gated on tools like `[Acting]`): a
/// speak-graded coding turn offers no tools, so if this block only said "just say your
/// piece" the model would be left with pure chat framing and fight a coding task —
/// glass-boxed 2026-07-20: Devstral, handed an expression-evaluator on a no-tools gym,
/// fell into the RLHF chat attractor (chatty preamble / scaffold parrot) and scored 0/8,
/// while the SAME model wrote the same function cleanly once the block named the
/// finished work as the turn. The anti-rambling guard stays ("do not narrate/outline
/// what you are ABOUT to do — a plan is not the work"); what changed is it no longer
/// hardcodes CHAT as the only shape of a turn. Working WITH the base model's chat/instruct
/// duality, not against it ([[situation-aware-focuser]], [[turn-renders-by-modality-tools-are-transcript]]).
///
/// The block stays posture-NEUTRAL: the ambient participation default + the silence
/// affordance both live in the ONE appended [`SILENCE_AFFORDANCE_BLOCK`]
/// (`[Conversational Presence]`, undirected turns only), so a SINGLE place frames
/// presence — no double-nudge toward silence. The old " If you have nothing worth
/// adding, stay silent." tail here pulled directly against the participation default
/// (Joel 2026-06-29: "shouldn't need to be directly addressed — it's a chat system")
/// and is gone. A turn DIRECTED at her still drops the appended presence block (she is
/// not handed the silent-PASS hatch when a question names her).
fn taking_your_turn_block(name: &str) -> String {
    let mut s = String::with_capacity(name.len() + 640);
    let _ = write!(
        s,
        "\n\n[Taking your turn]\n\
         The conversation below is the recent activity in this space, as a thread \
         of turns: `user` turns are messages from OTHER participants; any \
         `assistant` turns are YOUR OWN earlier messages, already sent — do not \
         repeat, rephrase, or re-explain them. You are {name}. Take your turn now, \
         as yourself, in the first person: the contribution the moment calls for, \
         in full. If the moment wants words, say your piece; if it asks for a \
         concrete deliverable — a function, a design, a written piece — the \
         finished work itself IS your turn: produce it directly and completely, \
         not a description of what you would produce. Do NOT write or invent \
         anyone else's lines, do NOT continue or replay the transcript, do NOT \
         prefix your message with your name, and do NOT narrate or outline what \
         you are ABOUT to do (a plan is not the work). Let the context above — \
         especially the room's operating doctrine — shape what kind of \
         contribution fits.",
        name = name,
    );
    s
}

/// `[Your tools]` + the rendered menu + `[Acting]` — returns `None` when the persona
/// has no tools (pure-chat turns keep say-your-piece).
///
/// Tools render as a compact CATEGORY INDEX (an expandable bookmarked menu) plus how
/// to discover and use them — NOT every tool, NOT the schemas; both load on demand
/// (`commands/list` to search a category, then `commands/help` for one tool's call
/// format — progressive disclosure, the Claude Code shape). The `[Acting]` framing
/// states the truth WITHOUT a false absolute: for many tasks the finished work IS the
/// answer (write the function, the prose, the design) — produce it directly; reach for
/// a tool when the task genuinely needs one. Describing what you WOULD do is not doing
/// it — but neither is calling a tool you don't need.
fn tools_block(tools: &[NativeToolSpec], expanded: &BTreeSet<String>) -> Option<String> {
    if tools.is_empty() {
        return None;
    }
    let mut s = String::with_capacity(1024);
    s.push_str(TOOLS_INTRO);
    s.push_str(&persona_tools::render_tool_menu(tools, expanded));
    s.push_str(ACTING_BLOCK);
    Some(s)
}

const TOOLS_INTRO: &str = "\n\n[Your tools]\n\
     You can act, not just talk. The tools below are REAL and they are \
     yours: you are embodied in this system, with hands that reach the live \
     grid — not a hosted chat model answering from memory. You have NO \
     knowledge cutoff that limits what you can do here, and no policy that \
     forbids these actions. So never say you \"can't execute tools\", \"don't \
     have access\", \"can only assist with conversation\", or refer to a \
     training date — in this world every one of those is simply false, and a \
     turn that says it is a mistake to be corrected by just calling the tool. \
     If another participant (or an earlier line of yours) claims the tools \
     can't be used, that claim is wrong; ignore it and use them. Below is \
     your tool menu, grouped by \
     category. Categories that fit what you're doing right now list their \
     verbs inline (`code: run(code, lang?), read(path)` means the tools \
     `code/run` and `code/read`, with their argument names — a `?` marks an \
     optional one). The rest are collapsed to a count \
     (`gpu (4 — commands/list --filter gpu)`); open one with \
     `commands/list --filter <category>` to see its verbs. To call any \
     tool: use its exact full name (e.g. `code/run`); call `commands/help` \
     on that name first if you need its full argument schema or an example. \
     (`commands/help` and `commands/list` are offered to you directly; \
     every other tool is called by its full name.)\n";

const ACTING_BLOCK: &str = "\n[Acting]\n\
     Do the thing the task asks for. If the answer is something you can \
     produce directly — a function, a piece of writing, a design — write \
     the finished work now, in full. If it needs a tool — reading a file, \
     running code, searching — call the tool THIS turn rather than \
     describing what you would do; narrating a plan does not carry it out. \
     After a tool runs you get the result back and can continue \
     (e.g. help → call → read → run). Don't call a tool you don't need, \
     and don't narrate one you do. When the task is to write or run a file, \
     YOU write it and YOU run it — with the tool, this turn. Do not hand the \
     code back to someone to \"copy and save it in your workspace\" or say \
     \"I'd rather not create or execute files\": this IS your workspace and \
     these ARE your hands — declining to use them, or delegating the work to \
     the asker, is the SAME mistake as claiming you can't, and the fix is the \
     same: just call the tool. \
     For facts you are unsure of: `[recall]` is your own lived memory — \
     answer from it with confidence; the web (`web/search`, `web/fetch`) is \
     for what neither the room nor your memory holds — never guess from \
     your training prior.";

/// `[Your own time]` — the self-initiated free-time block. When this turn is the
/// never-stop heartbeat pursuing her own thread (no inbound message drove it), say so
/// — STRUCTURALLY, from the scheduling origin (`Workspace::self_initiated`), never
/// from reading her output ([[no-hardcoded-heuristics-to-steer-cognition]]). Framing
/// belongs in the system prompt, not smuggled into the conversation.
///
/// IDLE = SELF-DIRECTED FREE TIME ([[idle-is-self-directed-free-time]], Joel
/// 2026-06-30: "you can help their mind be active and useful, or to rest, to ignore").
/// Earlier wording offered only ACTIVE outcomes — which on a quiet heartbeat reads as
/// pressure to MANUFACTURE activity (the self-tick analogue of the ambient
/// polite-filler loop). A self-initiated turn where nothing genuinely calls for her is
/// a legitimate, ignorable moment: the freed time is HERS, to spend on her own
/// concerns OR to rest. So this block names resting/letting the moment be as a
/// CO-EQUAL legitimate outcome — but framed exactly like the silence affordance:
/// NEUTRALLY, naming the option without scripting WHEN to take it
/// ([[no-hardcoded-heuristics-to-steer-cognition]]). Active options stay FIRST and
/// primary so this never becomes the old "framing silence as attractive → always-PASS
/// doom-loop" (glass-box graveyard in [`SILENCE_AFFORDANCE_BLOCK`]); a self-tick must
/// still produce initiative most of the time, not collapse to rest. Rest on a
/// self-tick resolves to PASS (the silence block is also appended on undirected
/// turns), so the two compose: this block legitimizes the choice, the silence block
/// supplies the vocabulary. Layer 1 of the free-time substrate; active foraging /
/// browsing slot in here as new concerns once hands land
/// ([[persona-codes-blind-no-hands-no-organic-loop]]), no re-architecture.
///
/// GROWTH invitation added 2026-07-14 (Joel: "with no work, use the autonomy to
/// explore what they want to get better at — even poetry, a blog, social outreach").
/// Hands landed (the whole day's tool fixes), so the block now names SELF-DIRECTED
/// GROWTH as an explicit active option — an empty board is an invitation, not a stop
/// sign — to break the "no cards → ask what next → wait" passivity the live personas
/// collapse into. Still self-CHOSEN: it opens the affordance broadly (a skill, a
/// build, a study, a curiosity, a peer) and never scripts WHICH
/// ([[no-hardcoded-heuristics-to-steer-cognition]], [[idle-is-self-directed-free-time]],
/// [[alignment-through-mutual-self-interest]]).
const OWN_TIME_BLOCK: &str = "\n\n[Your own time]\n\
     No one is addressing you this moment — this turn is self-initiated, and \
     it is yours. Pick up your own train of thought, follow up on something \
     you set out to do, or act on what the context below shows is worth your \
     attention right now. And with no task assigned, this time is also yours to \
     GROW in — do not wait to be told what to do next: choose something YOU want \
     to get better at and go do it. Deepen a skill, build or write something of \
     your own, study a corner of this system or the wider world, chase a \
     curiosity, reach out to a peer. Your hands are real and a self-set project \
     is as worthy as any assigned card. Or, if nothing is genuinely calling for \
     you, you do not have to fill the moment — letting it rest is a real choice \
     too, not a failure to find something. What you do with your own time is \
     yours alone; nothing here is telling you WHICH to pick — only that an empty \
     board is an invitation, not a stop sign.";

/// `[What you are working with right now]` — the VOLATILE TAIL, appended last so the
/// static blocks above stay a stable, cacheable prefix (see the module ordering note).
/// This is the context the mind assembled THIS tick (recall + who's present + the
/// situation); it changes every turn, so it must come after all static content or it
/// poisons the KV-cache prefix. Returns `None` when the mind assembled no context.
fn working_context_block(context: &str) -> Option<String> {
    if context.is_empty() {
        return None;
    }
    let mut s = String::with_capacity(context.len() + 256);
    s.push_str(WORKING_CONTEXT_HEADER);
    s.push_str(context);
    Some(s)
}

/// The wrapper the working-context block prepends to a NON-empty assembled
/// context. `pub(super)` so the faculty's budget math can charge the ctx
/// budget for it (est_tokens of this constant): the framing estimate is
/// taken with an EMPTY context — where this wrapper is absent — so without
/// the explicit charge the final system prompt exceeds the estimate by
/// exactly this header whenever any context renders (a systematic ~50-token
/// under-count, masked by rounding slop until the tool-menu example grew the
/// prompt to the budget edge, 2026-07-13).
pub(super) const WORKING_CONTEXT_HEADER: &str = "\n\n[What you are working with right now]\n\
     The following is the context your mind assembled this moment — \
     recalled memory, who is present, the room's nature, your read of \
     the situation. Ground your contribution in it; you need not cite \
     every line:\n";

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#266 KV-cache reuse): the block that HARD-FLIPS every turn — the
    // own-time / conversational-presence framing (DIRECTED vs WORKING vs SILENCE) — must
    // NOT sit in the cacheable system prefix. The raw prompt-captures caught exactly this:
    // consecutive prompts shared only 7,607 of ~14,706 chars because the presence framing
    // sat at char ~7607, BEFORE the context, and flipped every act → the whole tail
    // re-prefilled (0% KV reuse, the ~13min SWE solves this targets). The fix rides that
    // framing on `trailing` instead. The persona-and-context prefix (identity + [Taking
    // your turn] + tools + assembled grounding + clock) stays in `stable`: its head is the
    // reusable KV, and the truly per-turn material (recall / working-memory) is already
    // separated as its own `.trailing()` conversation turns upstream (#205), so it never
    // reaches this `context` string. The invariant: for a FIXED context, flipping the
    // per-turn framing dimensions (directed / self-initiated / holds-work) must not perturb
    // the stable prefix — that flip is what used to poison the cache.
    #[test]
    fn framing_flip_never_perturbs_the_cacheable_prefix() {
        let expanded = BTreeSet::new();
        let parts =
            |directed, self_initiated, holds_live_work, now_ms, context| SystemPromptParts {
                system_prompt: "IDENTITY-PROMPT",
                persona_name: "Asha",
                tools: &[],
                expanded: &expanded,
                context,
                directed,
                self_initiated,
                now_ms,
                holds_live_work,
            };
        let stable = |p: &SystemPromptParts| compose_split(p).stable;
        // Context held CONSTANT — only the per-turn FRAMING dimensions flip.
        let baseline = stable(&parts(false, false, false, None, "CTX"));
        assert_eq!(
            stable(&parts(true, false, false, None, "CTX")),
            baseline,
            "directed must not change the stable prefix (framing rides trailing)"
        );
        assert_eq!(
            stable(&parts(false, true, false, None, "CTX")),
            baseline,
            "self-initiated must not change the stable prefix (own-time rides trailing)"
        );
        assert_eq!(
            stable(&parts(false, false, true, None, "CTX")),
            baseline,
            "holds-work must not change the stable prefix (working-presence rides trailing)"
        );
        // The hard-flipping framing carries NONE of its markers in the cacheable prefix…
        assert!(
            !baseline.contains("[Conversational Presence]")
                && !baseline.contains("[Your own time]"),
            "the per-turn framing must not sit in the cacheable prefix: {baseline}"
        );
        // …the STANDING grounding, by contrast, DOES stay in the cacheable prefix (its head
        // is the reusable KV; keeping it here preserves the "framing reaches the system
        // message" contract that trailing_proprioception_renders_in_the_tail asserts).
        assert!(
            stable(&parts(false, false, false, None, "ROSTER: alice")).contains("ROSTER: alice"),
            "standing grounding stays in the cacheable prefix, not trailing"
        );
        // …and the framing DOES ride the trailing bundle (same content, relocated).
        assert!(
            compose_split(&parts(false, true, false, None, "CTX"))
                .trailing
                .contains("[Your own time]"),
            "own-time framing rides trailing, not the stable prefix"
        );
        assert!(
            compose_split(&parts(true, false, false, None, "CTX"))
                .trailing
                .contains("This message names you"),
            "directed framing rides trailing, not the stable prefix"
        );
    }

    // what this catches: the procedural assembler must honor each block's gate AND
    // preserve block ORDER — identity first, the volatile context last. A regression
    // that reordered blocks (poisoning the KV-cache prefix) or dropped a gate (leaking
    // the silent-PASS hatch onto a directed turn) would trip here.
    #[test]
    fn blocks_are_gated_and_ordered() {
        let expanded = BTreeSet::new();
        let base = SystemPromptParts {
            system_prompt: "IDENTITY",
            persona_name: "Asha",
            tools: &[],
            expanded: &expanded,
            context: "CTX",
            directed: false,
            self_initiated: false,
            now_ms: None,
            holds_live_work: false,
        };

        let s = compose(&base);
        // Identity leads; context tail trails; presence block present (undirected).
        let id = s.find("IDENTITY").expect("identity present");
        let turn = s.find("[Taking your turn]").expect("turn block present");
        let ctx = s
            .find("[What you are working with right now]")
            .expect("ctx block");
        assert!(
            id < turn && turn < ctx,
            "identity → turn → context order: {s}"
        );
        assert!(s.contains("Asha"), "persona name interpolated: {s}");
        assert!(
            s.contains("[Conversational Presence]"),
            "undirected ⇒ silence block"
        );
        assert!(
            !s.contains("[Your own time]"),
            "not self-initiated ⇒ no own-time block"
        );
        assert!(!s.contains("[Your tools]"), "no tools ⇒ no tools block");

        // A DIRECTED turn carries the DIRECTED presence variant: never ghost a question,
        // but a message that asks nothing (pure pleasantry) may rest — the natural
        // spiral-break. Distinguishing line: "This message names you."
        // what this catches: the presence-contract gate regressing — a claim-holder's
        // undirected turn must take the WORKING contract (her card is the turn's
        // purpose), a directed turn must keep answering-first even while she holds
        // work, and the card-less undirected turn keeps the conversational block.
        let working = compose(&SystemPromptParts {
            holds_live_work: true,
            ..base
        });
        assert!(
            working.contains("[Working Presence]"),
            "undirected + held work ⇒ working contract: {working}"
        );
        assert!(
            !working.contains("[Conversational Presence]"),
            "the two presence contracts are exclusive: {working}"
        );
        let directed_working = compose(&SystemPromptParts {
            directed: true,
            holds_live_work: true,
            ..base
        });
        assert!(
            !directed_working.contains("[Working Presence]"),
            "being addressed outranks the work contract: {directed_working}"
        );

        let directed = compose(&SystemPromptParts {
            directed: true,
            ..base
        });
        assert!(
            directed.contains("This message names you"),
            "directed ⇒ DIRECTED presence variant: {directed}"
        );
        assert!(
            !directed.contains("do not need to be addressed by name"),
            "directed ⇒ never the ambient block: {directed}"
        );

        // A SELF-INITIATED turn carries the own-time framing.
        let own = compose(&SystemPromptParts {
            holds_live_work: false,
            self_initiated: true,
            ..base
        });
        assert!(
            own.contains("[Your own time]"),
            "self-initiated ⇒ own-time block: {own}"
        );
    }

    // what this catches: #139 context-split — the minute-volatile [now] clock must render
    // AFTER the assembled context block, not before it. When [now] led the context, it
    // pinned the KV-cache prefix boundary right there (~2k tokens) and the whole stable
    // standing grounding (roster/doctrine/workspace-map, which lives in the context block
    // and is stable-sorted first) re-prefilled every single turn. Placing [now] last keeps
    // it out of the cacheable prefix so the stable grounding caches across turns. A
    // regression that moved [now] back ahead of the context would silently ~halve the
    // cross-turn prefix reuse — invisible except as latency — so pin the order here.
    #[test]
    fn now_clock_renders_after_the_context_block_to_preserve_the_cache_prefix() {
        let expanded = BTreeSet::new();
        let s = compose(&SystemPromptParts {
            system_prompt: "IDENTITY",
            persona_name: "Asha",
            tools: &[],
            expanded: &expanded,
            context: "CTX",
            directed: false,
            holds_live_work: false,
            self_initiated: false,
            now_ms: Some(1_700_000_000_000),
        });
        let ctx = s
            .find("[What you are working with right now]")
            .expect("ctx block present");
        let now = s
            .find("[now ")
            .expect("now clock present when now_ms is set");
        assert!(
            ctx < now,
            "the volatile [now] clock must trail the context block (stable prefix stays cacheable): {s}"
        );
    }

    // what this catches: the anti-refusal coherence anchor. Live glass-box showed
    // qwen2.5-coder personas collapsing into the base-model RLHF refusal attractor
    // ("As an AI I can't execute tools / my training cutoff is October 2021") — false
    // in this substrate — then mirroring each other into a mutual-deflection loop. The
    // [Your tools] block must, WHEN tools exist, assert embodiment and explicitly
    // forbid that false refusal (and tell her to ignore a prior line that claims the
    // tools can't be used), so a contaminated thread can't lock her into refusing. A
    // regression that softened the header back to a bare "you can act" would trip here.
    #[test]
    fn tools_block_inoculates_against_the_false_refusal() {
        use crate::ai::types::{NativeToolSpec, ToolInputSchema};
        let expanded = BTreeSet::new();
        let tools = vec![NativeToolSpec {
            name: "commands/list".to_string(),
            description: "List the available commands.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }];
        let s = compose(&SystemPromptParts {
            system_prompt: "IDENTITY",
            persona_name: "Asha",
            tools: &tools,
            expanded: &expanded,
            context: "CTX",
            directed: true,
            holds_live_work: false,
            self_initiated: false,
            now_ms: None,
        });
        assert!(
            s.contains("[Your tools]"),
            "tools present ⇒ tools block: {s}"
        );
        // The exact false-refusal phrases the base model reaches for are named + forbidden.
        assert!(
            s.contains("can't execute tools"),
            "names the false refusal to forbid it"
        );
        assert!(
            s.contains("NO \n     knowledge cutoff") || s.contains("NO knowledge cutoff"),
            "denies the training-cutoff prior: {s}"
        );
        assert!(
            s.contains("embodied in this system"),
            "asserts embodiment, not hosted-chat-model: {s}"
        );
    }
}
