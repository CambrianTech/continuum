//! The provider seam for [`WireShape::Provided`](crate::sdk_codegen::WireShape::Provided)
//! commands — how a persona's `perception/observe` or `interface/screenshot`
//! call reaches a connected eye-node that can actually render/capture.
//!
//! # The gap this closes
//!
//! A `Provided` command has NO substrate `ServiceModule` — the headless core
//! cannot fulfill it (no browser, no renderer, no display on a rack instance).
//! Before this seam, a persona calling one fell through to
//! [`CommandExecutor`](super::command_executor)'s case-4 fail-loud ("no Rust
//! module handles command"), which is *correct* but says nothing about the real
//! shape: the command needs a connected ADAPTER (one NAME, N platform adapters —
//! [[persona-is-a-client]]).
//!
//! This module adds two pieces:
//!
//! - [`ProviderRegistry`] — the set of connected providers, keyed by exact
//!   command name. The IPC connection layer registers a provider when an eye-node
//!   connects (declaring which commands it fulfills) and drops it on disconnect.
//!   Shared (`Arc`) between that writer and the interceptor reader.
//! - [`ProvidedCommandInterceptor`] — sits at the TAIL of the interceptor chain
//!   (`[airc, grid, provided]`): airc/grid get first look so an explicitly
//!   remote-targeted `perception/observe` still hops to a peer's eye; otherwise,
//!   if the command is `Provided` and a provider is connected, forward the bare
//!   `Params → Result` to it; if `Provided` but NOTHING is connected, fail loud
//!   naming the missing eye-node (never a fabricated observation —
//!   [[fallbacks-are-illegal-fail-loud]]); if not `Provided`, decline so local
//!   dispatch handles it unchanged.
//!
//! # Why the registry is empty in production today
//!
//! No eye-node CLIENT exists yet (it rides the client-SDK reinvent, task #29 —
//! a Node worker that `Commands.provide`s `perception/observe` over the
//! `@continuum/perception` `Surface`). Until it connects, the registry is empty
//! and a persona asking to observe gets the honest, specific fail-loud rather
//! than a stale "no Rust module handles command". Wiring that client's
//! registration into [`ProviderRegistry::register`] is the next slice; this seam
//! is the substrate half, testable now with an in-core fake provider.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;

use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use super::CommandResult;
use crate::sdk_codegen::{command_registry, WireShape};

/// A connected adapter that fulfills one or more `Provided` commands — an
/// eye-node holding a browser/renderer, a UI client that can screenshot, a VR
/// headset that can grab its framebuffer. Bare `Params` in → bare `Result` JSON
/// out; the adapter owns `success`/`error` (the result carries its own, the
/// BARE-not-enveloped half of [`WireShape::Provided`]).
#[async_trait]
pub trait ProvidedCommandProvider: Send + Sync {
    /// Fulfill one call. `command` is the exact command name (a provider may
    /// serve several); `params` is the bare params value. Returns the bare
    /// result JSON, or `Err` if the adapter itself failed to produce one.
    async fn fulfill(&self, command: &str, params: Value) -> Result<Value, String>;

    /// Human label for logs — which adapter this is (e.g. `"browser-eye@laptop-3"`).
    fn label(&self) -> &str;
}

/// The connected-provider set, keyed by exact command name. Last registration
/// wins (a fresh connection supersedes a stale one for the same command).
#[derive(Default)]
pub struct ProviderRegistry {
    by_command: DashMap<String, Arc<dyn ProvidedCommandProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `provider` as the fulfiller for each command in `commands`. The
    /// connection layer calls this when an eye-node connects.
    pub fn register(&self, commands: &[&str], provider: Arc<dyn ProvidedCommandProvider>) {
        for c in commands {
            self.by_command.insert((*c).to_string(), Arc::clone(&provider));
        }
    }

    /// Drop the providers for these commands (on disconnect). Idempotent.
    pub fn unregister(&self, commands: &[&str]) {
        for c in commands {
            self.by_command.remove(*c);
        }
    }

    /// The provider currently serving `command`, if any.
    pub fn provider_for(&self, command: &str) -> Option<Arc<dyn ProvidedCommandProvider>> {
        self.by_command.get(command).map(|e| Arc::clone(e.value()))
    }

    /// How many command→provider bindings are live (for telemetry/tests).
    pub fn len(&self) -> usize {
        self.by_command.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_command.is_empty()
    }
}

/// Routes `Provided` commands to a connected [`ProvidedCommandProvider`], or
/// fails loud when none is connected. See module docs for placement + contract.
pub struct ProvidedCommandInterceptor {
    registry: Arc<ProviderRegistry>,
    /// The set of command names whose [`WireShape`] is `Provided`, snapshotted
    /// from the registry at construction (the registry is after-boot-immutable).
    /// Membership decides whether this interceptor acts or declines — so a normal
    /// command (`code/read`) is never touched and flows to local dispatch.
    provided: HashSet<&'static str>,
}

impl ProvidedCommandInterceptor {
    /// Build over a shared registry. The IPC connection layer keeps the same
    /// `Arc` to register/unregister providers as eye-nodes come and go.
    pub fn new(registry: Arc<ProviderRegistry>) -> Self {
        let provided = command_registry()
            .into_iter()
            .filter(|d| d.wire == WireShape::Provided)
            .map(|d| d.name)
            .collect();
        Self { registry, provided }
    }

    /// The shared registry — the connection layer registers a connected
    /// eye-node's provider here so this interceptor can route to it.
    pub fn registry(&self) -> Arc<ProviderRegistry> {
        Arc::clone(&self.registry)
    }
}

#[async_trait]
impl CommandInterceptor for ProvidedCommandInterceptor {
    async fn try_route(
        &self,
        command: &str,
        params: &Value,
        _caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String> {
        // Not a Provided command — this interceptor has no opinion; let the chain
        // fall through to local Rust dispatch unchanged.
        if !self.provided.contains(command) {
            return Ok(InterceptorOutcome::Decline);
        }

        match self.registry.provider_for(command) {
            // A connected adapter owns this command — forward the bare params and
            // return its bare result. A provider-side failure surfaces (no silent
            // swallow): the caller learns the adapter tried and failed.
            Some(provider) => {
                let value = provider.fulfill(command, params.clone()).await.map_err(|e| {
                    format!(
                        "eye-node provider '{}' failed to fulfill '{command}': {e}",
                        provider.label()
                    )
                })?;
                Ok(InterceptorOutcome::Handled(CommandResult::Json(value)))
            }
            // Provided, but nothing is connected to fulfill it. Fail loud, named —
            // the honest state of a browserless core with no eye attached. Never a
            // fabricated observation.
            None => Err(format!(
                "'{command}' is a Provided capability (an eye-node verb like \
                 perception/observe or interface/screenshot), but no adapter is \
                 connected to fulfill it. The headless core cannot render/capture \
                 itself — connect a client that provides '{command}' (a \
                 browser-capable eye-node). [[fallbacks-are-illegal-fail-loud]]"
            )),
        }
    }

    fn name(&self) -> &'static str {
        "provided"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use serde_json::json;

    /// A canned in-core eye-node: records how many times it was asked, and
    /// echoes a fixed observation. Stands in for the real Node adapter so the
    /// routing + fail-loud arms are provable without a browser.
    struct FakeEye {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ProvidedCommandProvider for FakeEye {
        async fn fulfill(&self, command: &str, params: Value) -> Result<Value, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "success": true,
                "fulfilledBy": self.label(),
                "command": command,
                "echoedParams": params,
            }))
        }
        fn label(&self) -> &str {
            "fake-eye"
        }
    }

    // what this catches: a Provided command WITH a connected provider forwards to
    // it and returns the provider's bare result — the whole point of the seam
    // (a persona's perception/observe reaches an eye-node).
    #[tokio::test]
    async fn routes_a_provided_command_to_its_connected_provider() {
        let registry = Arc::new(ProviderRegistry::new());
        let calls = Arc::new(AtomicUsize::new(0));
        registry.register(&["perception/observe"], Arc::new(FakeEye { calls: calls.clone() }));

        let interceptor = ProvidedCommandInterceptor::new(registry);
        let outcome = interceptor
            .try_route("perception/observe", &json!({ "target": "https://x" }), None)
            .await
            .expect("a connected provider must fulfill, not error");

        match outcome {
            InterceptorOutcome::Handled(CommandResult::Json(v)) => {
                assert_eq!(v["success"], true);
                assert_eq!(v["fulfilledBy"], "fake-eye");
                assert_eq!(v["echoedParams"]["target"], "https://x");
            }
            other => panic!("expected Handled(Json), got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "provider must be invoked exactly once");
    }

    // what this catches: a Provided command with NO connected provider fails loud
    // (not a silent decline, not a fabrication) and names both the command and the
    // missing eye-node — the honest browserless-core state.
    #[tokio::test]
    async fn provided_command_with_no_provider_fails_loud() {
        let interceptor = ProvidedCommandInterceptor::new(Arc::new(ProviderRegistry::new()));
        let err = interceptor
            .try_route("perception/observe", &json!({ "target": "https://x" }), None)
            .await
            .expect_err("no provider connected must surface an error, never a silent decline");
        assert!(err.contains("perception/observe"), "names the command: {err}");
        assert!(err.contains("eye-node"), "names the missing adapter kind: {err}");
    }

    // what this catches: a NORMAL (non-Provided) command is untouched — the
    // interceptor declines so local Rust dispatch handles it exactly as before.
    // Without this guard the seam would swallow every command in the registry.
    #[tokio::test]
    async fn declines_a_normal_command_so_local_dispatch_is_unchanged() {
        let interceptor = ProvidedCommandInterceptor::new(Arc::new(ProviderRegistry::new()));
        let outcome = interceptor
            .try_route("code/read", &json!({ "path": "/tmp/x", "mode": "read" }), None)
            .await
            .expect("a non-Provided command must not error");
        assert!(
            matches!(outcome, InterceptorOutcome::Decline),
            "a non-Provided command must Decline so the chain falls through to local dispatch"
        );
    }

    // what this catches: the interceptor's `provided` set is really sourced from
    // the live registry — perception/observe and interface/screenshot (both
    // Provided) are in it, and a Bare command is not. Regression-pins the wiring
    // that decides "act vs decline".
    #[test]
    fn provided_set_is_the_real_provided_slice_of_the_registry() {
        let interceptor = ProvidedCommandInterceptor::new(Arc::new(ProviderRegistry::new()));
        assert!(interceptor.provided.contains("perception/observe"));
        assert!(interceptor.provided.contains("interface/screenshot"));
        assert!(
            !interceptor.provided.contains("code/read"),
            "a Bare command must NOT be in the Provided set"
        );
    }
}
