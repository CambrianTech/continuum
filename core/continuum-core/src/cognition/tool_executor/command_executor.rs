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
}

impl CommandToolExecutor {
    pub fn new(conn: Connection<InProcessTransport>) -> Self {
        Self { conn }
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
        let transport = InProcessTransport::new(executor, Some(CallerIdentity::local_persona(persona)));
        Self::new(Connection::new(transport))
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

    // Unknown command: the substrate's dev-facing message is useless to her.
    if raw.contains("no Rust module handles command") || raw.starts_with("no command") {
        // A command whose full name ends with `/<attempted>` is almost certainly
        // what she meant — she dropped the leading category (`cargo/check` →
        // `code/cargo/check`). Suggest only AiSafe names (the surface she can
        // call), deduped and ordered for a stable, leak-free hint.
        let suffix = format!("/{normalized}");
        let mut suggestions: Vec<&'static str> = command_registry()
            .into_iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe && d.name.ends_with(&suffix))
            .map(|d| d.name)
            .collect();
        suggestions.sort_unstable();
        suggestions.dedup();
        let did_you_mean = match suggestions.as_slice() {
            [] => String::new(),
            [one] => format!(" Did you mean `{one}`?"),
            many => format!(
                " Did you mean one of: {}?",
                many.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
            ),
        };
        return format!(
            "`{normalized}` is not a tool you can call.{did_you_mean} \
             Call `commands/list` (optionally with a `filter` keyword) to find the \
             right tool, then `commands/help` with its exact name to see the \
             arguments and an example before you retry."
        );
    }

    // Bad/missing arguments: the `[invalid]` prefix means serde already named the
    // offending field — reinforce the manual so she gets the exact shape + example.
    if raw.contains("[invalid]") {
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
                    content: truncate_on_boundary(value.to_string(), max_result_chars),
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
        assert!(out.contains("commands/list"), "must point her at discovery: {out}");
        assert!(out.contains("commands/help"), "must reinforce the manual: {out}");
        assert!(out.contains("`frobnicate`"), "must name what she tried: {out}");
    }

    // what this catches: a dropped category prefix (the most common near-miss) gets a
    // concrete did-you-mean drawn from the live registry — `cargo/check` is a real
    // suffix of the AiSafe `code/cargo/check`, so she's handed the exact name.
    #[test]
    fn unknown_command_offers_did_you_mean_on_dropped_prefix() {
        let raw = "no Rust module handles command: 'cargo/check'.".to_string();
        let out = persona_tool_error("cargo/check", raw);
        assert!(
            out.contains("Did you mean") && out.contains("code/cargo/check"),
            "should suggest the full AiSafe name: {out}"
        );
    }

    // what this catches: a bad-args refusal keeps the substrate's own field-naming
    // reason AND appends the commands/help reinforcement — feedback that names the
    // problem and the fix, in her paradigm.
    #[test]
    fn invalid_params_feedback_reinforces_help() {
        let raw = "code/write: [invalid] missing field `filePath`".to_string();
        let out = persona_tool_error("code/write", raw);
        assert!(out.contains("missing field `filePath`"), "keeps the real cause: {out}");
        assert!(out.contains("commands/help"), "reinforces the manual: {out}");
        assert!(out.contains("code/write"), "names the tool to look up: {out}");
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
                InProcessTransport::new(executor, Some(CallerIdentity::airc(Uuid::new_v4())));
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
