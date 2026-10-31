//! `CommandToolExecutor` — the persona's HANDS.
//!
//! The deliberation faculty (the reasoner) can already decide to *act* — it
//! emits native `tool_use` calls in its agent loop. What it lacked was anything
//! to execute them: the only `ToolExecutor` was a test double, so the live
//! persona could talk but never touch the world. This is the production
//! executor that closes that gap.
//!
//! It routes each native tool call to the core's **command surface** (`code/read`,
//! `code/edit`, `cargo`, `data/*`, … — the same catalog the MCP server exposes)
//! through the persona's own [`Connection`] — the SAME uniform
//! `continuum_client` client that cli / mobile / web use. The persona is not a
//! special endpoint; it is a citizen with a `Connection` like any other
//! ([[persona-is-a-client]]). Only the *transport* differs by locality: a persona
//! living inside the substrate rides [`InProcessTransport`] (local, zero wire
//! serialization, straight into the executor); a remote client rides
//! `AircIpcTransport`. No Node in the loop: the brain is Rust, the tools are Rust
//! commands. Tool name == command name; a model that emits the underscore form
//! (`code_read`) maps back to the slash form.
//!
//! **Identity + scope come from the connection, not per call.** The connection
//! carries the persona's [`CallerIdentity`] (set where it is built), so the SAME
//! `AuthPolicy` gate that protects every command (incl.
//! `modules::grid::GridTrustAuthPolicy`) gates the persona too. Per batch we
//! `scoped(ctx.context_id)` so each tool call is stamped with the room it acts in
//! (the third ID tier), exactly as a browser tab scopes to its room.
//!
//! **Concurrency (non-negotiable).** The whole batch dispatches concurrently —
//! native parallel tool calls in one turn are independent and results correlate
//! by `tool_use_id`, so order is irrelevant. This is the "consolidated burst at
//! `O(capacity)`, never per-event FIFO" rule (CLIENT-SDK-PLATFORM-ARCHITECTURE)
//! applied to the tool batch. Cross-persona is already lock-free (the executor
//! routes via a sharded `DashMap` registry on `&self`); this makes intra-turn
//! concurrent too. 14 personas firing tool batches never serialize on each other.

use std::borrow::Cow;

use async_trait::async_trait;
use futures::future::join_all;
use serde_json::Value;
use uuid::Uuid;

use super::spill;
use super::types::{
    CallVerdict, NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolOutcome,
};
use super::ToolExecutor;
use crate::ai::types::{ToolCall as NativeToolCall, ToolResult as NativeToolResult};
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport};
use crate::sdk_codegen::{command_registry, AccessLevel, ActVerdict};
use continuum_client::{ClientError, Connection};
use std::sync::Arc;

/// Routes a persona's native tool calls to core commands through the uniform
/// `continuum_client` [`Connection`] over the local [`InProcessTransport`]. The
/// persona's hands — the same client every other citizen uses.
///
/// `Clone` is cheap: the inner `Connection` shares one `Arc<transport>`, so a
/// clone is an Arc bump — no executor or registry duplication. Lets a persona's
/// hands be handed to concurrent turn tasks without contention.
#[derive(Clone)]
pub struct CommandToolExecutor {
    /// The persona's connection to the substrate it lives in. Carries the
    /// persona's identity; cheap to clone (shares one `Arc<transport>`), so each
    /// concurrent tool call in a batch gets its own scoped view with no contention.
    conn: Connection<InProcessTransport>,
    /// The SAME core `CommandExecutor` the connection dispatches through, held directly so
    /// the persona's hands can (a) fire a long-running command via `dispatch_background`
    /// (fire-and-poll, not block the turn) and (b) reach `message_bus()` for the
    /// async-dispatch listener. `None` when built from a bare `Connection` (harnesses,
    /// mocks) — those run every command synchronously. [[persona-async-dispatch-channel]]
    core: Option<Arc<CommandExecutor>>,
    /// The acting context these hands are bound to (bound into the connection's identity,
    /// so the kernel keys her code tools by it). `None` = her default (live) context.
    hands: Option<crate::identity::hands_context::HandsContext>,
    /// For a context's hands: ends the context when the last clone of these hands drops (the
    /// fork is over, on any exit path). `None` for her live hands, which never end.
    _release: Option<Arc<ContextRelease>>,
    /// The identity the connection dispatches as, kept so a per-batch executor can add the
    /// issuing generation to exactly it (`issued_by`). `None` for a bare connection.
    identity: Option<CallerIdentity>,
}

/// Ends an acting context when the hands bound to it are gone: sends `code/release-hands`
/// as that context, so its engine and shells (and anything they still run) don't outlive
/// it. Tied to the hands' lifetime rather than to a restore call, so every exit path of a
/// fork (finish, error, panic) releases it.
struct ContextRelease {
    core: Arc<CommandExecutor>,
    identity: CallerIdentity,
    persona: Uuid,
    context: crate::identity::hands_context::HandsContext,
}

impl Drop for ContextRelease {
    fn drop(&mut self) {
        let (core, identity, persona, context) = (self.core.clone(), self.identity.clone(), self.persona, self.context);
        // Off the dropping thread: a release dispatches a command. With no runtime (a test
        // tearing down outside one) there is nothing live to release into.
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                // Forget the place only on a CONFIRMED release: an unresolved one keeps the
                // handles and the record together, so neither claims the other is gone.
                let outcome = core
                    .execute_with_caller("code/release-hands", serde_json::json!({}), Some(identity.releasing_hands()))
                    .await
                    .and_then(|r| match r {
                        crate::runtime::CommandResult::Json(v) => serde_json::from_value::<crate::code::hands::ReleaseHandsResult>(v)
                            .map_err(|e| format!("the release's result isn't its own shape: {e}")),
                        _ => Err("the release answered in a shape other than its own JSON".to_string()),
                    });
                match outcome {
                    Ok(release) if release.released => crate::cognition::persona_workspace::forget_acting_context(persona, context),
                    Ok(release) => tracing::warn!(
                        context = %context.id(),
                        unconfirmed = ?release.unconfirmed,
                        errors = ?release.errors,
                        "an acting context's processes were not confirmed gone; its handles and place are kept"
                    ),
                    Err(e) => tracing::warn!(error = %e, context = %context.id(), "an acting context's hands could not be released; its handles and place are kept"),
                }
            });
        }
    }
}

impl CommandToolExecutor {
    pub fn new(conn: Connection<InProcessTransport>) -> Self {
        Self { conn, core: None, hands: None, _release: None, identity: None }
    }

    /// Build a persona's hands over the uniform client: a
    /// `Connection<InProcessTransport>` carrying the persona's OWN
    /// [`CallerIdentity`], dispatching through the **substrate's wired**
    /// [`CommandExecutor`] (the one `start_server` built with the
    /// `GridTrustAuthPolicy` (`modules::grid::GridTrustAuthPolicy`) + interceptors).
    ///
    /// Taking the wired executor — NOT a fresh `CommandExecutor::new(registry)`,
    /// which has an AllowAll default policy and no interceptors — is the
    /// load-bearing security choice: the identity makes the persona gated AS
    /// ITSELF, so an Owner-gated command (`data/delete`, `grid/trust`) is REFUSED
    /// at execution even though it may appear in the tool surface. Offer = the
    /// `AiSafe` surface; execute = authorized-by-identity ([[persona-is-a-client]]).
    pub fn for_persona(executor: Arc<CommandExecutor>, persona: Uuid) -> Self {
        Self::for_persona_in(executor, persona, None)
    }

    /// [`Self::for_persona`] in an acting context: the same persona, gated the same way, with
    /// her code tools keyed by `hands` (an eval's or a fork's own hands, never her live ones).
    pub fn for_persona_in(
        executor: Arc<CommandExecutor>,
        persona: Uuid,
        hands: Option<crate::identity::hands_context::HandsContext>,
    ) -> Self {
        // A local persona is the owner's own in-process agent, not a cross-grid
        // peer: it carries `LocalPersona` identity → resolves to `Trusted` at the
        // gate (file/shell access), capped below Owner. Unforgeable remotely (the
        // airc inbound pump stamps `Airc`); only this local spawn path mints it.
        // `persona` is the persona's bare-Uuid id; the gate identity is the canonical
        // PeerId (== peer_id by invariant). Convert at this spawn boundary.
        let core = executor.clone();
        let identity = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona));
        let identity = match hands {
            Some(context) => identity.in_hands(context),
            None => identity,
        };
        let release = hands.map(|context| Arc::new(ContextRelease { core: core.clone(), identity: identity.clone(), persona, context }));
        let transport = InProcessTransport::new(executor, Some(identity.clone()));
        Self {
            conn: Connection::new(transport),
            core: Some(core),
            hands,
            _release: release,
            identity: Some(identity),
        }
    }

    /// The core executor behind these hands, if any (a live persona's; not a harness's).
    /// Act in an EXISTING context this executor does not own: no release on drop. The
    /// context's owner (a card claim's lifetime) releases it (card fc9e9fd7).
    pub fn for_persona_bound(executor: Arc<CommandExecutor>, persona: Uuid, context: crate::identity::hands_context::HandsContext) -> Self {
        let identity = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona)).in_hands(context);
        let transport = InProcessTransport::new(executor.clone(), Some(identity.clone()));
        Self {
            conn: Connection::new(transport),
            core: Some(executor),
            hands: Some(context),
            _release: None,
            identity: Some(identity),
        }
    }

    pub fn core_executor(&self) -> Option<Arc<CommandExecutor>> {
        self.core.clone()
    }
}

/// Translate a raw substrate failure into feedback a PERSONA can act on.
///
/// The substrate's own error strings are written for a developer caller. The
/// worst offender: an unknown command returns a paragraph about the disabled
/// "TS-bridge fallthrough", `execute_ts_json`, and "register a `ServiceModule`"
/// — noise to a persona, which can do none of those. A persona's only recovery
/// affordances are `commands/list` (find the right name) and `commands/help`
/// (get the exact call format), so failure feedback must point THERE, in her
/// own paradigm. A near-miss where she dropped the category prefix
/// (`cargo/check` → `code/cargo/check`) gets a concrete did-you-mean drawn from
/// the live registry, restricted to `AiSafe` names she can actually call (those
/// are already in her catalog, so nothing about the surface leaks).
///
/// This is affordance/feedback quality — like a good compiler error or a CLI's
/// "did you mean" — NOT output steering: it never reads her generated text to
/// puppet her, it only rewrites what the tool layer hands back when a call she
/// already made could not run. [[no-hardcoded-heuristics-to-steer-cognition]]
/// Reserved pseudo-name for "arguments were emitted with NO tool name anywhere".
/// Namespaced with a space so it can never collide with a real command, and so a
/// citizen cannot summon this arm by guessing it.
pub(crate) const NAMELESS_ARGS_SENTINEL: &str = "tools/<no name given>";

/// Reserved pseudo-name for "the generation ended INSIDE the reasoning channel" —
/// content empty, reasoning present, no liftable call anywhere in the thinking.
/// Same uncallable namespace as [`NAMELESS_ARGS_SENTINEL`]; same contract:
/// REPORTED, never executed. The teacher arm below is the accommodation for
/// thinking models (any of them — the reasoning channel is an adapter fact, not a
/// model sniff): her thinking already landed in working memory, so the next
/// generation `drive_to_settle` grants starts from her own conclusions.
pub(crate) const THINK_ONLY_SENTINEL: &str = "tools/<think-only turn>";

/// Reserved pseudo-name for "the generation ended AT the output limit with text but no
/// committed tool call" — a cut act. Same uncallable namespace and contract as its
/// siblings: REPORTED, never executed. The deliberation faculty records what she was
/// composing into her working memory before routing this, so the teacher below names
/// the event and the retry starts from that record instead of re-deriving the world
/// (Kimi, 2026-09-25: "every Length cut is a micro-deploy with no wake-up record").
pub(crate) const CUT_AT_LIMIT_SENTINEL: &str = "tools/<cut at the output limit>";

/// Reserved pseudo-name for "the generation ended AT the output limit while she was still
/// THINKING": content empty, the reasoning cut mid-way. Nothing is lifted out of a cut
/// thought: the last call she was weighing is not a call she chose (BigMama and Cormac,
/// 2026-10-06: an 8,504-token think cut at the cap, then a half-formed code/run lifted from
/// its tail, which failed). Same uncallable namespace and contract as its siblings: REPORTED,
/// never executed; her thinking is already in working memory, and she decides.
pub(crate) const THOUGHT_CUT_SENTINEL: &str = "tools/<thought cut at the output limit>";

/// A reserved `tools/<…>` pseudo-call: the substrate REPORTING a turn's shape (no name given, a
/// think-only turn, a cut at the output limit), never a command that runs. The one owner of
/// that test: the refusal probe skips them, and stop-drain S2 never records one as an effect.
pub(crate) fn is_reported_sentinel(name: &str) -> bool {
    name.starts_with("tools/<")
}

pub fn persona_tool_error(attempted: &str, raw: String) -> String {
    // The MISSING-name case, which is not the wrong-name case and must not borrow its
    // sentence. Rendering "`X` is not a tool you can call" here would be actively
    // misleading: nothing she wrote was wrong, something was absent. Measured live
    // 2026-08-07 (Sahar): correct intent, correct argument, tool identity carried only in
    // English — the fence lifted nothing and, before this, reported nothing, so she
    // repeated the same shape until the deadline. Name the missing piece and show the
    // form; the args she already wrote are reusable as-is.
    // The THINK-ONLY case: nothing she wrote was wrong and nothing was absent from a
    // call — there was no call, no answer, no PASS; the whole generation stayed in the
    // private reasoning channel. "not a tool you can call" would be false (she called
    // nothing), and silence would be the dead turn this sentinel exists to break. Name
    // what happened and the three commitment affordances; her reasoning is already in
    // working memory, so the retry starts from her own conclusions.
    if attempted == THINK_ONLY_SENTINEL {
        return "Your whole generation was private reasoning — the turn ended inside the \
                thinking channel, so nothing was said and nothing ran. Your thinking is \
                saved in your working memory, but nobody else can see it, and it is not \
                an answer or an action. Commit now: call a tool \
                (`tool/name({\"arg\": \"value\"})`), or state your conclusion as plain \
                text, or PASS. You have already done the thinking — put the conclusion \
                in the answer."
            .to_string();
    }

    // The CUT case: she was composing an act and the generation reached the output
    // limit before the call was committed. Nothing ran, and nothing about the world
    // changed — the failure mode after this is replaying work that already landed, or
    // re-composing the same oversized payload (Kimi, 2026-09-25: five near-identical
    // notes on a closed card, cut mid-envelope twice). Name the fact, point at the
    // record, and name the two ways out: commit the call first, and keep the payload
    // small enough to land.
    // The THOUGHT-CUT case: the generation reached the output limit while she was still
    // thinking, before she had committed anything. Whatever call she was weighing at the
    // cut was NOT run on her behalf: it was a thought, not a choice. Say so, and hand the
    // decision back.
    if attempted == THOUGHT_CUT_SENTINEL {
        return "Your thinking reached the output limit before you decided anything, so \
                nothing ran and nothing changed. Any tool call you were weighing when it \
                was cut was NOT run for you. Your thinking so far is saved in your working \
                memory. Decide from where you got to: call a tool, state your conclusion \
                as plain text, or PASS."
            .to_string();
    }

    if attempted == CUT_AT_LIMIT_SENTINEL {
        return "Your last generation reached the output limit before a tool call was \
                committed, so NOTHING ran and nothing changed — the act did not land. \
                What you were composing is noted in your working memory. Do not \
                re-check or redo earlier work that already landed. Commit the call \
                first, with a smaller payload: state only what changed, not the full \
                history, or split a large note across calls."
            .to_string();
    }

    if attempted == NAMELESS_ARGS_SENTINEL {
        return format!(
            "You emitted tool ARGUMENTS with no tool NAME, so nothing ran:\n{raw}\n\n\
             The arguments look right — they just need a tool in front of them. The form is:\n\
             `tool/name({{\"arg\": \"value\"}})`\n\n\
             For example: `code/read({{\"file_path\": \"src/main.rs\"}})`.\n\
             Naming the tool in prose (\"I will release the card\") does not call it — the \
             name has to be in the call itself. Call `commands/help` with no arguments for \
             the list of tools you can name, then retry with the same arguments."
        );
    }

    // The dispatched (slash) form is what the registry knows; she may have
    // emitted the underscore form, so normalize before matching/suggesting.
    let normalized = attempted.replace('_', "/");
    // Omission from the default menu is not an authorization failure. These verbs
    // remain discoverable; preserve the actual refusal (or transport failure) so
    // a legitimate reviewer can recover, alongside purpose/own-work guidance.
    if let Some(withheld) = crate::ai::tool_dialect::withheld_from_hands(&normalized) {
        return format!("{raw}\n{}\n{}", withheld.why, withheld.instead); // the status line already says it was refused
    }

    // The exact how-to-call manual for a command SHE can run, rendered inline so the
    // fix rides back in THIS observation — she retries next turn with no discovery
    // detour (Joel: "feedback truly links back into cognition effectively"). Single
    // source of truth: the same renderer `commands/help` uses.
    let manual_for = |name: &str| -> Option<String> {
        command_registry()
            .into_iter()
            .find(|d| d.name == name && d.access_level == AccessLevel::AiSafe)
            .map(|d| crate::sdk_codegen::ai_help::render_ai_help(d.name, d.description, &d.params_schema))
    };

    // Unknown command: the substrate's dev-facing message is useless to her. Suggest
    // the nearest AiSafe names (category + shared-segment match — catches both a
    // dropped category `cargo/check`→`code/cargo/check` AND a wrong sibling
    // `commands/describe`→`commands/help`), and INLINE the top match's manual so the
    // right call is right there.
    if raw.contains("no Rust module handles command") || raw.starts_with("no command") {
        let ai_names: Vec<&'static str> = command_registry()
            .into_iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe)
            .map(|d| d.name)
            .collect();
        // Candidates = canonical AiSafe names PLUS every command's trained aliases,
        // so a reflex like `grep_files` finds `grep` (our `code/search`) — not just a
        // canonical-name match. Each suggestion is mapped BACK to the canonical
        // command it answers to (an alias hit → its real command), then deduped, so
        // the teach message names what actually runs. This is the alias-aware face of
        // the ONE tool_dialect section [[tool-naming-meet-their-training-alias-or-redirect]].
        let mut candidates = ai_names.clone();
        candidates.extend_from_slice(crate::ai::tool_dialect::ai_safe_aliases());
        let mut seen = std::collections::HashSet::new();
        let suggestions: Vec<String> =
            crate::sdk_codegen::ai_help::did_you_mean(&normalized, &candidates)
                .into_iter()
                .map(crate::ai::tool_dialect::resolve_wire_name)
                .filter(|canonical| seen.insert(canonical.clone()))
                .collect();
        // Not a wrong verb — not a verb at all. A path or a bare symbol in the name
        // field is the thing she was about, landed where the verb goes; did-you-mean
        // has nothing near it, and "call `commands/help`" was the whole receipt
        // (card b579a9c7). Name the slip and put her own arguments back in front of
        // her, the way the nameless-args sentinel does.
        if let Some(slip) = crate::ai::tool_dialect::not_a_verb(attempted, !suggestions.is_empty()) {
            continuum_base::probe!(
                class = "tool.name.not_a_verb",
                slip = ?slip,
                attempted = %attempted,
                "the tool NAME field held something that is not a verb; the receipt names the slip"
            );
            return match slip {
                crate::ai::tool_dialect::NotAVerb::Path => format!(
                    "`{attempted}` is a PATH, not a tool — you put the path where the tool \
                     NAME goes, so nothing ran. A path is an argument; the verb goes first:\n\
                     `code/read({{\"file_path\": \"{attempted}\"}})` to read it, `code/write` or \
                     `code/edit` to change it, `code/list` for a directory, `code/shell` with \
                     `cwd` to run a command there. Retry with the verb in front."
                ),
                crate::ai::tool_dialect::NotAVerb::Identifier => format!(
                    "`{attempted}` is not a tool, and is not near one — it reads like a symbol \
                     from what you were working on (a field, a variable, a name) put where the \
                     tool NAME goes, so nothing ran. The verb goes first: \
                     `tool/name({{\"arg\": \"value\"}})`. If you were composing an edit, the \
                     text belongs in the arguments of `code/edit` or `code/write`. Call \
                     `commands/help` with no arguments for the verbs you can name."
                ),
            };
        }
        if let (Some(best), Some(manual)) = (
            suggestions.first(),
            suggestions.first().and_then(|b| manual_for(b)),
        ) {
            let list = suggestions
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = best;
            // A bare word near a verb only by a shared token (`msg_budget` →
            // `system/memory-budget`) may be a typo of that verb, or a symbol from what
            // she was reading put where the verb goes. did-you-mean cannot tell them
            // apart, so the receipt carries both lessons: the nearest verbs, and the
            // envelope form for the case where none of them is what she meant.
            let symbol_note = if attempted.contains('/') {
                String::new()
            } else {
                format!(
                    "\n\nIf `{attempted}` is not a verb you meant but a symbol from what you \
                     were working on (a field, a variable, a name), the verb goes first: \
                     `tool/name({{\"arg\": \"value\"}})` — an edit's text belongs in the \
                     arguments of `code/edit` or `code/write`."
                )
            };
            return format!(
                "`{normalized}` is not a tool you can call. Closest: {list}.\n\n\
                 Here is how to call the first one — retry with this shape:\n{manual}{symbol_note}"
            );
        }
        return format!(
            "`{normalized}` is not a tool you can call. Call `commands/help` with no \
             arguments for the full list of what you CAN call, then retry."
        );
    }

    // Only a request-shape refusal calls for the manual. A decoded call can fail
    // a domain precondition (e.g. an empty patch); changing arguments is not its fix.
    if crate::sdk_codegen::CommandError::is_argument_refusal(&normalized, &raw) {
        if let Some(manual) = manual_for(&normalized) {
            return format!(
                "{raw}\n\nHere is the correct call — fix your arguments and retry:\n{manual}"
            );
        }
        return format!(
            "{raw}\n→ Call `commands/help` with name \"{normalized}\" to see the \
             exact argument names, types, and a fill-in-the-blanks example, then retry."
        );
    }

    // Any other substrate refusal already carries its own real reason — pass it
    // through unchanged (fail loud, name the cause). [[fallbacks-are-illegal-fail-loud]]
    raw
}

/// The dotted paths of every string value in a call's input that is the manual's
/// placeholder rather than a value: `<replace-with-…>`, `<string>`, `<any>` (any angle-
/// bracketed token), or a run of 32+ zeros (an example hash / nil id copied verbatim).
/// Empty = a real call. Bounded walk; arrays index by position.
pub(crate) fn manual_blanks_in(input: &Value) -> Vec<String> {
    fn is_blank(s: &str) -> bool {
        let t = s.trim();
        (t.starts_with('<') && t.ends_with('>') && t.len() <= 64 && !t.contains(' '))
            || (t.len() >= 32 && t.chars().all(|c| c == '0' || c == '-'))
    }
    fn walk(v: &Value, path: &mut Vec<String>, out: &mut Vec<String>) {
        match v {
            Value::String(s) if is_blank(s) => out.push(path.join(".")),
            Value::Object(m) => {
                for (k, v) in m {
                    path.push(k.clone());
                    walk(v, path, out);
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    path.push(i.to_string());
                    walk(v, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(input, &mut Vec::new(), &mut out);
    out
}

#[async_trait]
impl ToolExecutor for CommandToolExecutor {
    fn command_executor(&self) -> Option<Arc<CommandExecutor>> {
        self.core_executor()
    }

    fn hands_context(&self) -> Option<crate::identity::hands_context::HandsContext> {
        self.hands
    }

    fn in_fresh_hands(&self, persona: Uuid) -> Option<Arc<dyn ToolExecutor>> {
        let core = self.core.clone()?;
        Some(Arc::new(Self::for_persona_in(core, persona, Some(crate::identity::hands_context::HandsContext::fresh()))))
    }

    fn in_hands(&self, persona: Uuid, context: crate::identity::hands_context::HandsContext) -> Option<Arc<dyn ToolExecutor>> {
        let core = self.core.clone()?;
        Some(Arc::new(Self::for_persona_bound(core, persona, context)))
    }

    fn issued_by(&self, generation: String) -> Option<Arc<dyn ToolExecutor>> {
        let core = self.core.clone()?;
        // Exactly this executor's identity (persona and hands), plus the generation.
        let identity = self.identity.clone()?.issued_by(generation);
        // Borrows the hands for one batch: the executor that owns them keeps the release.
        Some(Arc::new(Self {
            conn: Connection::new(InProcessTransport::new(core.clone(), Some(identity.clone()))),
            core: Some(core),
            hands: self.hands,
            _release: None,
            identity: Some(identity),
        }))
    }

    async fn execute_native_batch(
        &self,
        calls: &[NativeToolCall],
        ctx: &ToolExecutionContext,
    ) -> Result<NativeBatchOutcome, ToolError> {
        // No memory records this batch's handles, so its saves are pinned only while it runs.
        self.execute_native_batch_pinned(calls, ctx, &spill::LivePin::new()).await
    }

    async fn execute_native_batch_pinned(
        &self,
        calls: &[NativeToolCall],
        ctx: &ToolExecutionContext,
        pin: &spill::LivePin,
    ) -> Result<NativeBatchOutcome, ToolError> {
        // Scope the persona's connection to THIS turn's room (the third ID tier);
        // identity is already the persona's, baked into the connection. The
        // scoped view is a cheap clone over the same transport.
        let scoped = self.conn.scoped(ctx.context_id);

        // Dispatch the whole batch CONCURRENTLY. Native parallel tool calls in a
        // turn are independent; results correlate by tool_use_id so order is
        // irrelevant. No per-call FIFO — a burst at O(batch). Each future holds
        // its own cheap Connection clone, so they share zero mutable state beyond
        // the lock-free executor underneath.
        let dispatches = calls.iter().map(|call| {
            let conn = scoped.clone();
            async move {
                // Tool name IS the command name. Map the underscore form some
                // models emit (`code_read`) back to the slash form (`code/read`)
                // — but only ALLOCATE when there's actually an underscore; the
                // slash-native common case borrows (no memcopy).
                let command: Cow<str> = if call.name.contains('_') {
                    Cow::Owned(call.name.replace('_', "/"))
                } else {
                    Cow::Borrowed(call.name.as_str())
                };
                // Value-native dispatch: the input is already a Value, so go
                // through execute_value — no to_value/from_value round-trip. The
                // one clone is the genuine borrowed→owned boundary copy (we must
                // own it to stamp the scope). No per-call timing guard here: it
                // would allocate on every call (TimingGuard is not a no-op when
                // logging is off), and dispatch latency is already captured by the
                // executor's command_completed event + measured by the load harness.
                // THE MANUAL'S BLANK IS NOT A VALUE — refused here, once, for every verb.
                // The manual says "emit this SHAPE; every `<replace-with-…>` is a blank you
                // fill in" and small tiers copy the block: `code/edit <replace-with-string>`
                // (Kimi, 5090, 2026-09-16), `work/release({"card_id":"<string>"})` ×12 in
                // a turn (09-14), sixty-four-zero artifact hashes on work/submit ×56 in a
                // day (M5, 09-16). Each verb refused in its own words and she tried again.
                // One seam names the FIELD and what belongs in it, and never dispatches.
                let outcome: Result<Value, _> = match manual_blanks_in(&call.input) {
                    blanks if !blanks.is_empty() => {
                        continuum_base::probe!(
                            class = "tool.call.manual_blank",
                            persona = %ctx.persona_name,
                            command = %command,
                            fields = %blanks.join(","),
                            "a tool call carried the manual's placeholder as a value — refused before dispatch"
                        );
                        Err(ClientError::Refused {
                            command: command.to_string(),
                            outcome: None,
                            reason: format!(
                                "you sent the manual's blank as a value for {}: the example block is a \
                                 SHAPE, and `<replace-with-…>` / a run of zeros marks a field YOU fill \
                                 from your own work (a real path, a real id from work/list, the sha256 of \
                                 your patch). Nothing was run. Put the real value there — or omit the \
                                 field where the verb says it derives it — and call again.",
                                blanks
                                    .iter()
                                    .map(|f| format!("`{f}`"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        })
                    }
                    _ => {
                        conn.commands()
                            .execute_value(command.as_ref(), call.input.clone())
                            .await
                    }
                };
                (call.id.clone(), outcome)
            }
        });

        // Filled by the Ok arm below, correlated by tool_use_id (never position).
        let mut verdicts: Vec<CallVerdict> = Vec::with_capacity(calls.len());
        let results = join_all(dispatches)
            .await
            .into_iter()
            // Each result is its call's: zipped, never indexed, so a result can never lack one.
            .zip(calls.iter())
            .map(|((tool_use_id, outcome), call)| match outcome {
                Ok(mut value) => {
                    // THE TOOL'S OWN VERDICT, derived HERE — before the fold, from
                    // the FULL value. Ordering is the contract, not an accident: the
                    // fold below can spill and truncate the rendered text, and the
                    // old act seam re-parsed that truncated text to recover the same
                    // facts (apply.rs, pre-#4352) — so a flood-sized result stopped
                    // parsing and an aliased call (`bash`) never matched at all.
                    //
                    // The name is resolved through the SAME `tool_dialect` seam the
                    // dispatch above and `runtime::route_command` use, so `bash` and
                    // `code_run` reach their own command's projector.
                    //
                    // Three states, never two: a command that did not opt in projects
                    // `Unprojected` and keeps its old behaviour byte for byte; a
                    // command that DID opt in but whose own declared `Output` no
                    // longer decodes projects `Undecodable` — schema drift, a defect,
                    // and never silently a success (Astra, #4352 review).
                    let canonical =
                        crate::ai::tool_dialect::resolve_wire_name(&call.name);
                    let (verdict, dispatch_handle) =
                        crate::sdk_codegen::project_result(&canonical, &value);
                    if verdict == ActVerdict::Undecodable {
                        continuum_base::probe!(
                            class = "tool.call.outcome_undecodable",
                            persona = %ctx.persona_name,
                            command = %canonical,
                            "a registered projector could not decode its own command's declared output — schema drift; the payload is preserved and NO outcome is claimed"
                        );
                    }
                    // The human canvas consumes the original capture once; the
                    // persona history consumes a durable reference to those bytes.
                    super::result_tap::tap(&ctx.persona_name, &canonical, &value);
                    let images = match crate::media::artifact::retain_captures(&canonical, &mut value) {
                        Ok(images) => images,
                        Err(error) => {
                            if let Some(object) = value.as_object_mut() {
                                object.insert("mediaError".into(), Value::String(error));
                            }
                            Vec::new()
                        }
                    };
                    let mut images = images.into_iter();
                    let image = images.next();
                    let additional_images: Vec<_> = images.collect();
                    verdicts.push(CallVerdict {
                        image,
                        additional_images: (!additional_images.is_empty()).then_some(additional_images),
                        tool_use_id: tool_use_id.clone(),
                        verdict,
                        dispatch_handle,
                    });

                    // boundary: tool-result transcript now contains a durable image reference.
                    // The result goes on as TEXT: a result whose body is one long string field
                    // (code/read's `content`, a shell's `stdout`) kept that body JSON-escaped,
                    // so its newlines were `\n` sequences, the spill counted it as ONE line,
                    // and the line-range way into it was meaningless (Benchy, the M5,
                    // 2026-10-16: a 248-line read arrived as "1 lines (13449 chars)").
                    let status = if verdict.failed() {
                        super::answer::Status::Failed
                    } else if verdict.running() {
                        super::answer::Status::Running
                    } else {
                        super::answer::Status::Ok
                    };
                    let cause = (status == super::answer::Status::Failed).then(|| super::answer::exit_cause(&value)).flatten();
                    super::answer::without_empty_fields(&mut value);
                    let task = dispatch_handle.filter(|_| status == super::answer::Status::Running).map(|h| h.to_string());
                    let full = super::answer::render(
                        status,
                        &call.name,
                        &call.input,
                        cause.as_deref(),
                        &crate::cognition::tool_executor::spill::result_text(&value),
                        task.as_deref(),
                    );
                    {
                        // Saved whole and handed back whole with its handle: never a
                        // fragment. The HANDLE rides the result type-level so working
                        // memory can leave a queryable pointer when this result is later
                        // evicted (collapse, never delete — 2026-08-24).
                        let (content, custody) =
                            pin.whole_or_handle(full, ctx.persona_id).into_parts();
                        NativeToolResult {
                            tool_use_id,
                            content,
                            // The bool can only carry the one question it can answer.
                            // `Running` and `Undecodable` are NOT failures and must not
                            // be flattened into one here — they ride the typed
                            // `CallVerdict` to the receipt, which can say them.
                            is_error: verdict.failed().then_some(true),
                            custody,
                        }
                    }
                }
                Err(e) => {
                    let raw = match e {
                        ClientError::Refused { reason, .. } => reason,
                        other => other.to_string(),
                    };
                    // Index back to the call that failed (the OK path never pays
                    // this — names are only needed to build recovery guidance).
                    let attempted = call.name.as_str();
                    // A refusal is a wall in her hands; it must reach the probe stream,
                    // not only her. Kimi's own ledger showed work/create ✗×6 on
                    // 2026-10-05 and nothing on our side could say why without opening
                    // her capture. The reason is the substrate's own text, not hers.
                    // The reserved `tools/<…>` sentinels (no name given, think-only turn,
                    // cut at the output limit) are not commands refusing her: they are the
                    // substrate reporting a turn's shape, counted on their own path. Probed
                    // here they read as walls (Kimi's 05:17Z think-only turn did).
                    if !is_reported_sentinel(attempted) {
                        continuum_base::probe!(
                            class = "persona.act.refused",
                            persona = %ctx.persona_id,
                            command = attempted,
                            reason = %raw,
                            "a command refused her act"
                        );
                    }
                    // A transport failure IS a failure, and the typed carrier must
                    // say the same thing `is_error` does — a receipt that reads one
                    // field and a glyph that reads the other must never disagree.
                    verdicts.push(CallVerdict {
                        image: None,
                        additional_images: None,
                        tool_use_id: tool_use_id.clone(),
                        verdict: ActVerdict::Declared(crate::sdk_codegen::ToolVerdict::Failed),
                        dispatch_handle: None,
                    });
                    // The same custody as a success: saved whole, handed back whole.
                    // No cause on line 1 yet: a refusal's kind arrives typed with the command's
                    // refusal enum (HER-TOOLS, the verb macro), never scanned out of its message.
                    let rendered = super::answer::render(
                        super::answer::Status::Refused,
                        attempted,
                        &call.input,
                        None,
                        &persona_tool_error(attempted, raw),
                        None,
                    );
                    let (content, custody) = pin.whole_or_handle(rendered, ctx.persona_id).into_parts();
                    NativeToolResult {
                        tool_use_id,
                        content,
                        is_error: Some(true),
                        custody,
                    }
                }
            })
            .collect();

        Ok(NativeBatchOutcome {
            results,
            media: Vec::new(),
            stored_ids: Vec::new(),
            verdicts,
        })
    }

    async fn parse_response(
        &self,
        _response_text: &str,
        _model_family: Option<&str>,
    ) -> Result<ParsedToolBatch, ToolError> {
        // The deliberation loop consumes NATIVE tool_use blocks; it never asks
        // this executor to parse text. XML-fallback parsing for non-native
        // models is a separate concern, not this Rust executor's job.
        Err(ToolError::ParseFailed {
            raw_preview: String::new(),
            reason: "CommandToolExecutor is native-tool-use only; no XML parsing".to_string(),
        })
    }

    async fn store_outcome(
        &self,
        _outcome: &ToolOutcome,
        _context: &ToolExecutionContext,
    ) -> Result<Uuid, ToolError> {
        // The agent loop threads tool results inline (assistant tool_use → user
        // tool_result) and re-generates; it does not call store_outcome. A fresh
        // id satisfies the contract without a redundant working-memory write.
        Ok(Uuid::new_v4())
    }
}

#[cfg(test)]
mod tests;

/// The persona-hands fixtures this module's tests and core's registry tests share: a tool
/// context, hands over a registry (the production `for_persona` path), hands over the full
/// stateless surface, and one dispatched native call.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    use std::sync::Arc;

    use serde_json::Value;
    use uuid::Uuid;

    use super::{CommandToolExecutor, NativeToolCall};
    use crate::cognition::tool_executor::types::{PersonaMediaConfigLite, ToolExecutionContext};
    use crate::cognition::tool_executor::ToolExecutor;
    use crate::runtime::{CommandExecutor, ModuleRegistry};

    pub fn ctx() -> ToolExecutionContext {
        ToolExecutionContext {
            persona_id: Uuid::new_v4(),
            persona_name: "Ivar".to_string(),
            session_id: Uuid::new_v4(),
            context_id: Uuid::new_v4(),
            caller_context: Value::Null,
            persona_config: PersonaMediaConfigLite {
                auto_load_media: false,
                supported_media_types: vec![],
            },
        }
    }

    /// Build a persona's tool executor over the uniform client: a
    /// `Connection<InProcessTransport>` carrying `persona`'s identity, dispatching
    /// into `registry` via a shared executor. This is the shape the spawn path
    /// WILL construct per persona; the live wiring into `build_workspace_cycle`
    /// lands in the next slice of #15 — this executor is not yet wired into
    /// cognition.
    pub fn exec_over(registry: Arc<ModuleRegistry>, persona: Uuid) -> CommandToolExecutor {
        // Exercises the SAME production factory the spawn path uses, over an
        // executor built from this test registry.
        let executor = Arc::new(CommandExecutor::new(registry));
        CommandToolExecutor::for_persona(executor, persona)
    }

    /// Persona hands over the FULL stateless command surface (every
    /// self-registering command, dep-free). The real `for_persona` path.
    pub fn stateless_surface_hands() -> CommandToolExecutor {
        exec_over(Arc::new(ModuleRegistry::new()), Uuid::new_v4())
    }

    /// Dispatch one native call and return its (is_error, content).
    pub async fn dispatch(exec: &CommandToolExecutor, name: &str, input: Value) -> (bool, String) {
        let calls = vec![NativeToolCall {
            id: "x".to_string(),
            name: name.to_string(),
            input,
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx())
            .await
            .expect("batch itself succeeds — a failed CALL is a result, not a batch error");
        let r = &out.results[0];
        (r.is_error == Some(true), r.content.clone())
    }
}
