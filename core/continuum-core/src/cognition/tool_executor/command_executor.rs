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
//! [`crate::routing::GridTrustAuthPolicy`]) gates the persona too. Per batch we
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

use super::types::{
    NativeBatchOutcome, ParsedToolBatch, ToolError, ToolExecutionContext, ToolOutcome,
};
use super::spill;
use super::ToolExecutor;
use crate::ai::types::{ToolCall as NativeToolCall, ToolResult as NativeToolResult};
use crate::routing::CallerIdentity;
use crate::runtime::{CommandExecutor, InProcessTransport};
use crate::sdk_codegen::{command_registry, AccessLevel};
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
}

impl CommandToolExecutor {
    pub fn new(conn: Connection<InProcessTransport>) -> Self {
        Self { conn, core: None }
    }

    /// Build a persona's hands over the uniform client: a
    /// `Connection<InProcessTransport>` carrying the persona's OWN
    /// [`CallerIdentity`], dispatching through the **substrate's wired**
    /// [`CommandExecutor`] (the one `start_server` built with the
    /// [`GridTrustAuthPolicy`](crate::routing::GridTrustAuthPolicy) + interceptors).
    ///
    /// Taking the wired executor — NOT a fresh `CommandExecutor::new(registry)`,
    /// which has an AllowAll default policy and no interceptors — is the
    /// load-bearing security choice: the identity makes the persona gated AS
    /// ITSELF, so an Owner-gated command (`data/delete`, `grid/trust`) is REFUSED
    /// at execution even though it may appear in the tool surface. Offer = the
    /// `AiSafe` surface; execute = authorized-by-identity ([[persona-is-a-client]]).
    pub fn for_persona(executor: Arc<CommandExecutor>, persona: Uuid) -> Self {
        // A local persona is the owner's own in-process agent, not a cross-grid
        // peer: it carries `LocalPersona` identity → resolves to `Trusted` at the
        // gate (file/shell access), capped below Owner. Unforgeable remotely (the
        // airc inbound pump stamps `Airc`); only this local spawn path mints it.
        // `persona` is the persona's bare-Uuid id; the gate identity is the canonical
        // PeerId (== peer_id by invariant). Convert at this spawn boundary.
        let core = executor.clone();
        let transport = InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))),
        );
        Self {
            conn: Connection::new(transport),
            core: Some(core),
        }
    }

    /// The core executor behind these hands, if any (a live persona's; not a harness's).
    pub fn core_executor(&self) -> Option<Arc<CommandExecutor>> {
        self.core.clone()
    }
}

/// Truncate to at most `max` bytes on a UTF-8 char boundary. Tool output (a file
/// read, a cargo log) can be huge; the agent loop bounds it so the context
/// doesn't blow up. Appends a marker so the model knows it was cut.
fn truncate_on_boundary(mut s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("\n…[truncated]");
    s
}

/// Bound a tool RESULT for re-injection, keeping BOTH ends and naming the cut.
///
/// A tool that floods — a build log, a giant file, a chatty command (an Xcode or
/// cargo build is the canonical case) — must never blow the persona's context
/// window. The cap (`max_result_chars`) is that protection. But a naive head-keep
/// throws away exactly the part she needs for log-shaped output: the verdict (the
/// errors, the failing target, the summary) lives at the END. So this keeps the
/// HEAD (where a read/listing/JSON starts) AND the TAIL (where a build/test/log
/// concludes), eliding the middle with a marker that (a) names how much was
/// dropped and (b) reinforces the narrowing affordance — re-run the tool SCOPED,
/// or grep with `code/search`, so she asks for less next time instead of drowning.
///
/// Like the failure-feedback translation, this is affordance quality, not output
/// steering: it shapes what the tool layer hands back, never reads her generation.
///
/// When `spill` is `Some`, the FULL result was first persisted (tier 2) and the
/// elision marker names the handle so she can grep/page the whole thing via
/// `tool/output` — Joel's "even if it blows up, all is not lost". When it's
/// `None` (small output, or the spill itself failed), the marker reinforces the
/// re-run-scoped / `code/search` narrowing affordance instead.
fn truncate_tool_output(s: String, max: usize, spill: Option<&spill::SpillRef>) -> String {
    if s.len() <= max {
        return s;
    }
    // Split the visible budget between the two ends — the head of a read/listing
    // and the tail of a build/log are both load-bearing (~60/40 toward the head).
    let head_budget = max * 3 / 5;
    let tail_budget = max - head_budget;

    let mut head_end = head_budget.min(s.len());
    while head_end > 0 && !s.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = s.len().saturating_sub(tail_budget);
    while tail_start < s.len() && !s.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    // Degenerate tiny budget where the ends would overlap: fall back to head-only.
    if tail_start <= head_end {
        return truncate_on_boundary(s, max);
    }
    let dropped = tail_start - head_end;
    let recovery = match spill {
        // The whole result is recoverable — point her at the handle, and at the
        // failure-hunting path specifically (grep for the error), per Joel: the
        // build hands spit out a lot of crap and the job is finding the error.
        Some(r) => format!(
            "the FULL {} lines were saved as output `{}`. Find the part you need with \
             `tool/output` — easiest, jump straight to what broke with a prebuilt filter: \
             `{{\"handle\":\"{}\",\"filter\":\"errors\"}}` (or `warnings`/`failures`/\
             `summary`); for a specific hunt use `\"pattern\":\"<regex>\"`, or read a line \
             range with `startLine`/`endLine`",
            r.lines, r.handle, r.handle,
        ),
        // Not recoverable — narrow at the source instead.
        None => "Re-run the tool SCOPED to what you need (a narrower argument, e.g. \
                 `filter`/`package`/`path`), or grep with `code/search`, instead of \
                 reading it all"
            .to_string(),
    };
    format!(
        "{}\n…[{dropped} chars elided — this result is large. {recovery}. The start \
         and end are kept below.]…\n{}",
        &s[..head_end],
        &s[tail_start..],
    )
}

/// Bound a tool result for re-injection, spilling the FULL output to disk first
/// when it overflows so nothing is lost (tier 2). The returned preview names the
/// spill handle (when the spill succeeded) so the persona can grep/page it.
///
/// Spilling is best-effort: if it fails (no home dir, disk full) we STILL bound
/// the output — context safety is non-negotiable — she just loses the recover-it
/// affordance for that one result. We never fabricate a handle we couldn't write.
fn fold_with_recovery(full: String, max: usize, persona_id: Uuid) -> String {
    if full.len() <= max {
        return full;
    }
    match spill::spill(persona_id, &full) {
        Ok(r) => truncate_tool_output(full, max, Some(&r)),
        Err(_) => truncate_tool_output(full, max, None),
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
fn persona_tool_error(attempted: &str, raw: String) -> String {
    // The dispatched (slash) form is what the registry knows; she may have
    // emitted the underscore form, so normalize before matching/suggesting.
    let normalized = attempted.replace('_', "/");

    // The exact how-to-call manual for a command SHE can run, rendered inline so the
    // fix rides back in THIS observation — she retries next turn with no discovery
    // detour (Joel: "feedback truly links back into cognition effectively"). Single
    // source of truth: the same renderer `commands/help` uses.
    let manual_for = |name: &str| -> Option<String> {
        command_registry()
            .into_iter()
            .find(|d| d.name == name && d.access_level == AccessLevel::AiSafe)
            .map(|d| crate::commands::help::render_ai_help(d.name, d.description, &d.params_schema))
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
        candidates.extend_from_slice(crate::cognition::tool_dialect::ai_safe_aliases());
        let mut seen = std::collections::HashSet::new();
        let suggestions: Vec<String> = crate::commands::help::did_you_mean(&normalized, &candidates)
            .into_iter()
            .map(crate::cognition::tool_dialect::resolve_wire_name)
            .filter(|canonical| seen.insert(canonical.clone()))
            .collect();
        if let (Some(best), Some(manual)) =
            (suggestions.first(), suggestions.first().and_then(|b| manual_for(b)))
        {
            let list = suggestions
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = best;
            return format!(
                "`{normalized}` is not a tool you can call. Closest: {list}.\n\n\
                 Here is how to call the first one — retry with this shape:\n{manual}"
            );
        }
        return format!(
            "`{normalized}` is not a tool you can call. Call `commands/help` with no \
             arguments for the full list of what you CAN call, then retry."
        );
    }

    // Bad/missing arguments: the `[invalid]` prefix means serde already named the
    // offending field — INLINE the exact shape + example so she fixes it in place
    // instead of spending a turn on `commands/help`.
    if raw.contains("[invalid]") {
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

#[async_trait]
impl ToolExecutor for CommandToolExecutor {
    fn command_executor(&self) -> Option<Arc<CommandExecutor>> {
        self.core_executor()
    }

    async fn execute_native_batch(
        &self,
        calls: &[NativeToolCall],
        ctx: &ToolExecutionContext,
        max_result_chars: usize,
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
                let outcome: Result<Value, _> = conn
                    .commands()
                    .execute_value(command.as_ref(), call.input.clone())
                    .await;
                (call.id.clone(), outcome)
            }
        });

        let results = join_all(dispatches)
            .await
            .into_iter()
            .enumerate()
            .map(|(i, (tool_use_id, outcome))| match outcome {
                Ok(value) => NativeToolResult {
                    tool_use_id,
                    // Spill-then-bound: a flood-sized result is persisted whole
                    // (recoverable via `tool/output`) before the preview is cut.
                    content: fold_with_recovery(
                        value.to_string(),
                        max_result_chars,
                        ctx.persona_id,
                    ),
                    is_error: None,
                },
                // A failed tool call is NOT a batch failure — it's fed back to the
                // model as an error result so it can recover (retry, fix args,
                // pick another tool). Batch-level `Err` is reserved for the
                // executor/transport itself being unavailable. Take the substrate's
                // OWN reason (e.g. "no Rust module handles command: …") and translate
                // it into PERSONA-actionable feedback — naming the problem AND
                // reinforcing `commands/help`/`commands/list` — so she recovers on a
                // message in her own paradigm, not a developer-internal one.
                Err(e) => {
                    let raw = match e {
                        ClientError::Refused { reason, .. } => reason,
                        other => other.to_string(),
                    };
                    // Index back to the call that failed (the OK path never pays
                    // this — names are only needed to build recovery guidance).
                    let attempted = calls.get(i).map(|c| c.name.as_str()).unwrap_or("");
                    NativeToolResult {
                        tool_use_id,
                        content: truncate_on_boundary(
                            persona_tool_error(attempted, raw),
                            max_result_chars,
                        ),
                        is_error: Some(true),
                    }
                }
            })
            .collect();

        Ok(NativeBatchOutcome {
            results,
            media: Vec::new(),
            stored_ids: Vec::new(),
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
mod tests {
    use super::*;
    use crate::cognition::tool_executor::types::PersonaMediaConfigLite;
    use crate::runtime::{
        CommandExecutor, CommandResult, ModuleConfig, ModuleContext, ModulePriority,
        ModuleRegistry, ServiceModule,
    };
    use serde_json::json;
    use std::any::Any;
    use std::sync::Arc;

    // what this catches: a live persona's hands (built via for_persona) EXPOSE the core
    // executor, so apply_act can fire a long-running command in the background; a bare
    // Connection-built executor (harness) does NOT — it runs every command synchronously.
    // This is the reachability seam that lets a persona send a sentinel/compile away.
    #[test]
    fn for_persona_hands_expose_core_executor_for_dispatch() {
        let exec = Arc::new(CommandExecutor::new(Arc::new(ModuleRegistry::new())));
        let hands = CommandToolExecutor::for_persona(exec, uuid::Uuid::new_v4());
        assert!(
            hands.command_executor().is_some(),
            "live persona hands expose the core executor for fire-and-poll dispatch"
        );
        let bare = CommandToolExecutor::new(hands.conn.clone());
        assert!(
            bare.command_executor().is_none(),
            "harness hands (bare Connection) run every command synchronously"
        );
    }

    /// Minimal module that echoes its params back under `test/echo`.
    struct EchoModule;

    #[async_trait]
    impl ServiceModule for EchoModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "echo",
                priority: ModulePriority::Normal,
                command_prefixes: &["test/"],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            match command {
                "test/echo" => Ok(CommandResult::Json(params)),
                other => Err(format!("unknown command: {other}")),
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn ctx() -> ToolExecutionContext {
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

    // what this catches: the developer-internal unknown-command paragraph (TS-bridge
    // fallthrough, "register a ServiceModule") must NEVER reach the persona — she gets
    // a paradigm-native message pointing at commands/list + commands/help instead.
    #[test]
    fn unknown_command_feedback_is_persona_actionable_not_dev_noise() {
        let raw = "no Rust module handles command: 'frobnicate'. \
                   The implicit TS-bridge fallthrough is disabled per [[no-fallbacks-ever]]. \
                   register a `ServiceModule` whose `command_prefixes` covers it."
            .to_string();
        let out = persona_tool_error("frobnicate", raw);
        assert!(!out.contains("ServiceModule"), "dev noise leaked to persona: {out}");
        assert!(!out.contains("TS-bridge"), "dev noise leaked to persona: {out}");
        // No near-miss exists for "frobnicate", so she's pointed at full
        // discovery (commands/help with no arguments) — the #1916 contract.
        assert!(out.contains("commands/help"), "must point her at discovery: {out}");
        assert!(out.contains("`frobnicate`"), "must name what she tried: {out}");
    }

    // what this catches: a dropped category prefix (the most common near-miss) gets a
    // concrete near-miss list drawn from the live registry — `cargo/check` is a real
    // suffix of the AiSafe `code/cargo/check` — AND the top match's inline manual so
    // she retries THIS turn with the exact shape, no discovery detour (#1916).
    #[test]
    fn unknown_command_offers_did_you_mean_on_dropped_prefix() {
        let raw = "no Rust module handles command: 'cargo/check'.".to_string();
        let out = persona_tool_error("cargo/check", raw);
        assert!(
            out.contains("Closest") && out.contains("code/cargo/check"),
            "should suggest the full AiSafe name: {out}"
        );
        assert!(
            out.contains("retry with this shape"),
            "must inline the top match's manual so the retry needs no detour: {out}"
        );
    }

    // what this catches: a reflexive tool name that matches a command's trained
    // ALIAS (not its canonical name) still gets pointed at the right command —
    // `grep_files` → `grep` (our `code/search`). did_you_mean's candidates include
    // aliases now, and the suggestion is mapped back to the canonical command so the
    // teach names what actually runs. Without alias-aware candidates this was a bare
    // "not a tool you can call" with no direction. (#202 Slice 3)
    #[test]
    fn unknown_command_maps_a_trained_alias_reflex_to_its_canonical_command() {
        let raw = "no Rust module handles command: 'grep_files'.".to_string();
        let out = persona_tool_error("grep_files", raw);
        assert!(
            out.contains("code/search"),
            "a `grep`-family reflex must resolve to the canonical code/search: {out}"
        );
        // The canonical command is named, never the raw alias, so the retry shape is
        // the real one.
        assert!(
            out.contains("retry with this shape"),
            "must inline the canonical command's manual: {out}"
        );
    }

    // what this catches: a bad-args refusal keeps the substrate's own field-naming
    // reason AND inlines the command's correct call shape — feedback that names the
    // problem and the fix in the SAME observation, no help-lookup turn (#1916).
    #[test]
    fn invalid_params_feedback_reinforces_help() {
        let raw = "code/write: [invalid] missing field `filePath`".to_string();
        let out = persona_tool_error("code/write", raw);
        assert!(out.contains("missing field `filePath`"), "keeps the real cause: {out}");
        assert!(
            out.contains("fix your arguments and retry") || out.contains("commands/help"),
            "must hand her the correct shape (inline manual) or the manual pointer: {out}"
        );
        assert!(out.contains("code/write"), "names the tool: {out}");
    }

    // what this catches: a refusal that already carries a real, persona-readable
    // reason (e.g. an authorization denial) passes through unchanged — we don't
    // bury a genuine cause under boilerplate. [[fallbacks-are-illegal-fail-loud]]
    #[test]
    fn other_refusals_pass_through_unchanged() {
        let raw = "data/delete: refused — requires Owner trust".to_string();
        let out = persona_tool_error("data/delete", raw.clone());
        assert_eq!(out, raw);
    }

    // what this catches: a flood-sized tool result (a build log) is bounded AND
    // keeps the END — where the verdict/errors live — not just the head, and the
    // elision marker reinforces narrowing (re-run scoped / code/search grep).
    #[test]
    fn huge_output_keeps_both_ends_and_suggests_narrowing() {
        let body = format!(
            "BUILD START\n{}\nerror: linker failed — THE VERDICT AT THE END",
            "compiling module …\n".repeat(5000)
        );
        // None spill ref → the narrow-at-source affordance (re-run scoped / grep).
        let out = truncate_tool_output(body, 600, None);
        assert!(out.len() < 1200, "stays bounded near the cap: {} chars", out.len());
        assert!(out.contains("BUILD START"), "keeps the head: {out}");
        assert!(out.contains("THE VERDICT AT THE END"), "keeps the tail (the verdict): {out}");
        assert!(out.contains("code/search"), "reinforces grep/narrowing: {out}");
        assert!(out.contains("elided"), "names that output was cut: {out}");
    }

    // what this catches: when the full result was spilled, the elision marker
    // names the recovery handle AND points at the failure-hunting path
    // (grep the error via `tool/output`) — Joel's "all is not lost" affordance.
    #[test]
    fn spilled_output_marker_names_the_handle_and_recovery() {
        let body = format!("BUILD START\n{}\nerror: boom", "noise\n".repeat(5000));
        let fake = spill::SpillRef {
            handle: "deadbeefcafe0001".to_string(),
            path: std::path::PathBuf::from("/tmp/x.log"),
            bytes: body.len(),
            lines: body.lines().count(),
        };
        let out = truncate_tool_output(body, 600, Some(&fake));
        assert!(out.contains("deadbeefcafe0001"), "names the handle: {out}");
        assert!(out.contains("tool/output"), "names the recovery tool: {out}");
        // #1917: the failure hunt is a PREBUILT one-word filter, not a regex.
        assert!(
            out.contains("\"filter\":\"errors\""),
            "points at the prebuilt failure filter: {out}"
        );
    }

    // what this catches: output within budget is returned verbatim — no marker,
    // no elision when nothing was dropped.
    #[test]
    fn small_output_is_returned_verbatim() {
        let out = truncate_tool_output("ok: 2 passed".to_string(), 16_000, None);
        assert_eq!(out, "ok: 2 passed");
    }

    // what this catches: a multi-byte body never slices mid-codepoint (would panic),
    // at both a normal and a pathologically tiny budget — the head/tail boundary
    // walks must land on char boundaries on both ends.
    #[test]
    fn multibyte_body_never_panics_at_any_budget() {
        let body = "αβγδε".repeat(2000); // 2-byte chars, well over any cap
        for max in [8usize, 64, 600, 4096] {
            let out = truncate_tool_output(body.clone(), max, None);
            // round-trips as valid UTF-8 (the assertions themselves would panic on
            // a mid-codepoint slice) and stays near the requested budget.
            assert!(out.chars().count() > 1, "valid UTF-8 at max={max}");
            assert!(out.contains("elided"), "marks the cut at max={max}: {out}");
        }
    }

    /// Build a persona's tool executor over the uniform client: a
    /// `Connection<InProcessTransport>` carrying `persona`'s identity, dispatching
    /// into `registry` via a shared executor. This is the shape the spawn path
    /// WILL construct per persona; the live wiring into `build_workspace_cycle`
    /// lands in the next slice of #15 — this executor is not yet wired into
    /// cognition.
    fn exec_over(registry: Arc<ModuleRegistry>, persona: Uuid) -> CommandToolExecutor {
        // Exercises the SAME production factory the spawn path uses, over an
        // executor built from this test registry.
        let executor = Arc::new(CommandExecutor::new(registry));
        CommandToolExecutor::for_persona(executor, persona)
    }

    fn executor_with_echo() -> CommandToolExecutor {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        exec_over(registry, Uuid::new_v4())
    }

    // what this catches: THE thing that turns "talks" into "acts" — a native tool
    // call routes to the real command and the command's result comes back,
    // correlated by tool_use_id, no error. If this regresses, the persona is back
    // to a chatbot that can't touch the world.
    #[tokio::test]
    async fn routes_native_tool_call_to_the_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/echo".to_string(),
            input: json!({ "path": "deploy.md" }),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx(), 8000)
            .await
            .unwrap();
        assert_eq!(out.results.len(), 1);
        assert_eq!(out.results[0].tool_use_id, "t1");
        assert!(out.results[0].is_error.is_none(), "successful tool call");
        assert!(
            out.results[0].content.contains("deploy.md"),
            "command result fed back: {}",
            out.results[0].content
        );
    }

    // what this catches: a WORKING PERSONA on the clean command infra — a persona's
    // hands route a tool call to `ping`, which is migrated to the self-routing
    // DynCommand object path (ActionCommand ⟹ DynCommand, OFF the prefix table). The
    // call goes persona → uniform Connection → InProcessTransport → CommandExecutor
    // → execute_inner → route_object → the ping object, and the bare result comes
    // back, no error. Fully deterministic: no inference, no airc, no models — just
    // the command substrate. This is the regression guard that the cleanup didn't
    // break the persona's ability to ACT, and that the typed path serves personas.
    #[tokio::test]
    async fn persona_executes_ping_via_typed_object_path() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(crate::modules::health::HealthModule::new()));
        let exec = exec_over(registry, Uuid::new_v4());

        let calls = vec![NativeToolCall {
            id: "p1".to_string(),
            name: "ping".to_string(),
            input: json!({}),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx(), 8000)
            .await
            .expect("batch ok");

        assert_eq!(out.results.len(), 1);
        assert_eq!(out.results[0].tool_use_id, "p1");
        assert!(
            out.results[0].is_error.is_none(),
            "ping must succeed for the persona via the typed path: {}",
            out.results[0].content
        );
        assert!(
            out.results[0].content.contains("\"ok\":true"),
            "the bare PingResult is fed back to the persona: {}",
            out.results[0].content
        );
    }

    // what this catches: the underscore→slash mapping for models that emit
    // `test_echo` instead of `test/echo`.
    #[tokio::test]
    async fn maps_underscore_tool_name_to_slash_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test_echo".to_string(),
            input: json!({ "ok": true }),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx(), 8000)
            .await
            .unwrap();
        assert!(
            out.results[0].is_error.is_none(),
            "test_echo → test/echo routed"
        );
    }

    // what this catches: a failed tool call is fed back as an ERROR RESULT (so the
    // model can recover), NOT a batch-level failure that aborts the turn.
    #[tokio::test]
    async fn failed_call_becomes_error_result_not_batch_failure() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/nonexistent".to_string(),
            input: json!({}),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx(), 8000)
            .await
            .expect("batch itself succeeds");
        assert_eq!(
            out.results[0].is_error,
            Some(true),
            "per-call error, batch ok"
        );
        // the model sees the SUBSTRATE's reason, not the client-wrapper prefix —
        // so it recovers on the real message (regression-pins the Err arm that
        // unwraps ClientError::Refused.reason instead of Display).
        let content = &out.results[0].content;
        assert!(
            content.contains("nonexistent"),
            "surfaces the real reason: {content}"
        );
        assert!(
            !content.contains("refused"),
            "no client-wrapper prefix leaks to the model: {content}"
        );
    }

    /// Behavioral conformance exams (#163 Slice 2) — the DISPATCH half of the tool
    /// AI-usability harness. The static audit (`sdk_codegen::conformance`) proves a
    /// tool is discoverable + learnable; these prove it BEHAVES when a model fumbles
    /// the call: a mangled/unknown name fails LOUD (never a silent no-op that forges
    /// a receipt, #159), and a real command given the wrong args fails LOUD with the
    /// fix inline. Run over the REAL self-contained command surface: `ModuleRegistry
    /// ::new()` auto-seeds every stateless command object (no module wiring, no live
    /// state, no daemon), so this exercises production commands through the
    /// production persona-hands path, safely and deterministically.
    mod behavioral_conformance {
        use super::*;
        use crate::sdk_codegen::{command_registry, stateless_command_objects, AccessLevel};
        use std::collections::HashSet;

        /// Persona hands over the FULL stateless command surface (every
        /// self-registering command, dep-free). The real `for_persona` path.
        fn stateless_surface_hands() -> CommandToolExecutor {
            exec_over(Arc::new(ModuleRegistry::new()), Uuid::new_v4())
        }

        /// Dispatch one native call and return its (is_error, content).
        async fn dispatch(exec: &CommandToolExecutor, name: &str, input: Value) -> (bool, String) {
            let calls = vec![NativeToolCall {
                id: "x".to_string(),
                name: name.to_string(),
                input,
            }];
            let out = exec
                .execute_native_batch(&calls, &ctx(), 8000)
                .await
                .expect("batch itself succeeds — a failed CALL is a result, not a batch error");
            let r = &out.results[0];
            (r.is_error == Some(true), r.content.clone())
        }

        // what this catches (#159): an unknown/mangled tool NAME never silently
        // no-ops — it comes back as an ERROR result naming what she tried and
        // pointing at discovery, so she recovers instead of narrating a fake
        // receipt. The exact "write_file / list_files" snake-case vocabulary gap.
        #[tokio::test]
        async fn unknown_tool_name_fails_loud_never_silent() {
            let exec = stateless_surface_hands();
            for bogus in ["write_file", "list_files", "claim_task", "totally/made/up"] {
                let (is_error, content) = dispatch(&exec, bogus, json!({})).await;
                assert!(
                    is_error,
                    "'{bogus}' must fail LOUD (is_error), never a silent no-op: {content}"
                );
                // The recovery affordance rides back in the same observation.
                assert!(
                    content.contains("commands/help") || content.contains("not a tool"),
                    "'{bogus}' feedback must point at discovery so she recovers: {content}"
                );
            }
        }

        // what this catches (#163/#159): a REAL command given the WRONG args (empty
        // when it requires some) fails LOUD, never runs a degenerate no-op. Swept
        // over every stateless AiSafe command that DECLARES required params — a
        // living exam that grows with the surface, not a hand-picked list. Dispatch
        // is safe: a missing required field fails at the params boundary BEFORE the
        // command body runs, so nothing mutates.
        #[tokio::test]
        async fn required_args_missing_fails_loud_across_the_stateless_surface() {
            let exec = stateless_surface_hands();
            let stateless: HashSet<&'static str> =
                stateless_command_objects().iter().map(|c| c.name()).collect();

            let mut examined = 0usize;
            for d in command_registry()
                .into_iter()
                .filter(|d| d.access_level == AccessLevel::AiSafe && stateless.contains(d.name))
            {
                // Only commands that DECLARE required params — those MUST reject `{}`.
                let requires = d
                    .params_schema
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
                if !requires {
                    continue;
                }
                examined += 1;
                let (is_error, content) = dispatch(&exec, d.name, json!({})).await;
                assert!(
                    is_error,
                    "`{}` requires params but ran on empty input `{{}}` without erroring — \
                     a silent no-op a persona would mistake for success: {content}",
                    d.name
                );
            }
            // Non-vacuity: the sweep must have actually exercised commands.
            assert!(
                examined >= 1,
                "no stateless AiSafe command with required params was examined — the filter \
                 is broken; the exam checked nothing"
            );
        }

        // what this catches (#164 as a UNIVERSAL invariant, end-to-end): a persona
        // verb resolves the 8-char SHORT id she's shown, through the real hands +
        // real command, not just in the id_resolve unit test. Seeds a card so the
        // candidate set is non-empty, then reads it back by short id.
        #[tokio::test]
        async fn persona_verb_resolves_a_short_id_end_to_end() {
            use crate::persona::card::{self, PersonaCard};
            let id = Uuid::new_v4();
            card::register(PersonaCard::genesis(id, "Asha", 1000, None));
            let short: String = id.simple().to_string().chars().take(8).collect();

            let exec = stateless_surface_hands();
            let (is_error, content) =
                dispatch(&exec, "persona/identity/get", json!({ "personaId": short })).await;
            card::remove(&id.to_string());

            assert!(
                !is_error,
                "a persona verb must resolve the short id it was shown ({short}): {content}"
            );
            assert!(
                content.contains(&id.to_string()),
                "resolved card carries the full id: {content}"
            );
        }
    }

    /// Concurrency + load proofs. Gated behind `stress-tests` per the test
    /// doctrine (timing/multi-thread tests are compile-time gated, not `#[ignore]`).
    /// Run them: `cargo test -p continuum-core --features stress-tests \
    ///   cognition::tool_executor::command_executor::tests::stress -- --nocapture`
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use crate::logging::timing::PerformanceStats;
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        /// A command that PARKS at a barrier before returning. A barrier of width
        /// W releases only when W calls are simultaneously in-flight — so the
        /// batch completes IFF dispatch is concurrent. If anything serializes the
        /// calls (a shared lock, a FIFO queue), the W-th call never arrives, the
        /// barrier never trips, and the surrounding timeout fails the test. A
        /// deterministic concurrency proof, not a flaky wall-clock threshold.
        struct BarrierModule {
            gate: Arc<Barrier>,
        }

        #[async_trait]
        impl ServiceModule for BarrierModule {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "barrier",
                    priority: ModulePriority::Normal,
                    command_prefixes: &["load/"],
                    event_subscriptions: &[],
                    needs_dedicated_thread: false,
                    max_concurrency: 0,
                    tick_interval: None,
                }
            }
            async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
                Ok(())
            }
            async fn handle_command(
                &self,
                _command: &str,
                params: Value,
            ) -> Result<CommandResult, String> {
                // Every concurrent caller must reach here before any proceeds.
                self.gate.wait().await;
                Ok(CommandResult::Json(params))
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        fn load_calls(n: usize) -> Vec<NativeToolCall> {
            (0..n)
                .map(|i| NativeToolCall {
                    id: format!("t{i}"),
                    name: "load/work".to_string(),
                    input: json!({ "i": i }),
                })
                .collect()
        }

        /// One persona, many citizens — a substrate executor over an echo module.
        fn echo_executor() -> Arc<CommandExecutor> {
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(EchoModule));
            Arc::new(CommandExecutor::new(registry))
        }

        fn persona_over(executor: Arc<CommandExecutor>) -> CommandToolExecutor {
            let transport =
                InProcessTransport::new(
                executor,
                Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(Uuid::new_v4()))),
            );
            CommandToolExecutor::new(Connection::new(transport))
        }

        // what this catches: a single persona's tool BATCH dispatches concurrently
        // (join_all), not one-at-a-time. 50 calls all park at a Barrier(50); the
        // batch returns only if all 50 are in-flight at once. A regression to a
        // serial `for` loop would deadlock — the timeout converts that to a clean
        // failure instead of a hang.
        #[tokio::test(flavor = "multi_thread")]
        async fn intra_batch_dispatches_concurrently() {
            const N: usize = 50;
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(BarrierModule {
                gate: Arc::new(Barrier::new(N)),
            }));
            let exec = exec_over(registry, Uuid::new_v4());

            let out = tokio::time::timeout(
                Duration::from_secs(5),
                exec.execute_native_batch(&load_calls(N), &ctx(), 8000),
            )
            .await
            .expect("batch must finish — a timeout means the batch serialized")
            .expect("batch ok");

            assert_eq!(out.results.len(), N, "every concurrent call returned");
            assert!(out.results.iter().all(|r| r.is_error.is_none()), "all ok");
        }

        // what this catches: THE failure mode Joel named — "14 personas all locking
        // each other." 50 separate personas (each its own Connection + identity)
        // share ONE substrate executor and fire a tool call simultaneously. All 50
        // park at a Barrier(50); the join completes only if no persona blocks
        // another. A global lock / FIFO in the dispatch path would stop the 50th
        // from entering → deadlock → timeout failure.
        #[tokio::test(flavor = "multi_thread")]
        async fn personas_do_not_serialize_on_each_other() {
            const PERSONAS: usize = 50;
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(BarrierModule {
                gate: Arc::new(Barrier::new(PERSONAS)),
            }));
            let executor = Arc::new(CommandExecutor::new(registry));

            let handles: Vec<_> = (0..PERSONAS)
                .map(|_| {
                    let exec = persona_over(executor.clone());
                    tokio::spawn(async move {
                        exec.execute_native_batch(&load_calls(1), &ctx(), 8000)
                            .await
                    })
                })
                .collect();

            let outs = tokio::time::timeout(Duration::from_secs(5), join_all(handles))
                .await
                .expect("all personas must finish — a timeout means they serialized");

            assert_eq!(outs.len(), PERSONAS);
            for out in outs {
                let out = out.expect("join").expect("persona batch ok");
                assert_eq!(out.results.len(), 1);
                assert!(out.results[0].is_error.is_none());
            }
        }

        // The actual LOAD TEST (Joel: "see where it starts to degrade and then
        // iterate"). Ramps the persona fleet against ONE shared substrate executor,
        // each persona firing a 50-call tool batch (genuine parallelism via
        // tokio::spawn on the multi-thread runtime), and prints the throughput
        // curve so the knee is visible. Latency per batch is recorded through our
        // own `PerformanceStats` (atomic avg/min/max) — not eprintln. (Per-call
        // dispatch is intentionally NOT separately probed on the hot path; the
        // batch latency here is the measurement.)
        //
        // This is exploratory, not a brittle perf-threshold gate: it asserts only
        // correctness (every op completes). The printed curve is the artifact you
        // read to find where dispatch degrades, then iterate.
        #[tokio::test(flavor = "multi_thread")]
        async fn load_scaling_curve() {
            const CALLS_PER_PERSONA: usize = 50;
            let tiers = [1usize, 10, 50, 100, 200, 400, 800];
            let executor = echo_executor();

            println!(
                "\n  cores={}  calls/persona={CALLS_PER_PERSONA}",
                num_cpus::get()
            );
            println!(
                "  {:>8} │ {:>7} │ {:>8} │ {:>10} │ {:>12} │ {:>12}",
                "personas", "ops", "wall_ms", "ops/sec", "batch_avg_us", "batch_max_us"
            );

            for &p in &tiers {
                let stats = Arc::new(PerformanceStats::new());
                let start = Instant::now();
                let handles: Vec<_> = (0..p)
                    .map(|_| {
                        let exec = persona_over(executor.clone());
                        let stats = Arc::clone(&stats);
                        tokio::spawn(async move {
                            let calls = load_calls(CALLS_PER_PERSONA);
                            let c = ctx();
                            let t0 = Instant::now();
                            let out = exec
                                .execute_native_batch(&calls, &c, 8000)
                                .await
                                .expect("batch ok");
                            stats.record(t0.elapsed().as_micros() as u64);
                            out.results.len()
                        })
                    })
                    .collect();

                let counts = join_all(handles).await;
                let wall = start.elapsed();
                let ops: usize = counts.into_iter().map(|r| r.expect("join")).sum();
                assert_eq!(ops, p * CALLS_PER_PERSONA, "every op completed at p={p}");

                let snap = stats.snapshot();
                let ops_per_sec = ops as f64 / wall.as_secs_f64();
                println!(
                    "  {:>8} │ {:>7} │ {:>8.1} │ {:>10.0} │ {:>12} │ {:>12}",
                    p,
                    ops,
                    wall.as_secs_f64() * 1000.0,
                    ops_per_sec,
                    snap.avg_duration_us,
                    snap.max_duration_us
                );
            }
        }
    }
}
