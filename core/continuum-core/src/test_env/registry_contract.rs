#![cfg(test)]
//! Whole-product tests over the REAL command/event registry. The registrations are `inventory`
//! items submitted by the commands in this crate; the kernel's own test binary does not link
//! them, so these tests live here, where the whole registry is. Each asserts exactly what it
//! asserted inside the kernel file it came from.

mod provided_provider {
    use crate::runtime::provided_provider::is_provided_command;
    use crate::runtime::InterceptorOutcome;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use async_trait::async_trait;
    use serde_json::Value;
    use crate::runtime::CommandInterceptor;
    use crate::runtime::{CommandResult, ProviderRegistry, ProvidedCommandInterceptor, ProvidedCommandProvider};

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

    // what this catches: a Provided command with NO connected provider fails loud
    // (not a silent decline, not a fabrication) and names both the command and the
    // missing eye-node — the honest browserless-core state.
    #[tokio::test]
    async fn provided_command_with_no_provider_fails_loud() {
        let interceptor = ProvidedCommandInterceptor::new(Arc::new(ProviderRegistry::new()));
        let err = interceptor
            .try_route(
                "perception/observe",
                &json!({ "target": "https://x" }),
                None,
            )
            .await
            .expect_err("no provider connected must surface an error, never a silent decline");
        assert!(
            err.contains("perception/observe"),
            "names the command: {err}"
        );
        assert!(
            err.contains("eye-node"),
            "names the missing adapter kind: {err}"
        );
    }

    // what this catches: the interceptor's `provided` set is really sourced from
    // the live registry — perception/observe and interface/screenshot (both
    // Provided) are in it, and a Bare command is not. Regression-pins the wiring
    // that decides "act vs decline".
    #[test]
    fn provided_set_is_the_real_provided_slice_of_the_registry() {
        // The shared set both dispatch paths consult (interceptor + route_command).
        assert!(is_provided_command("perception/observe"));
        assert!(is_provided_command("interface/screenshot"));
        assert!(
            !is_provided_command("code/read"),
            "a Bare command must NOT be in the Provided set"
        );
    }

    // what this catches: a Provided command WITH a connected provider forwards to
    // it and returns the provider's bare result — the whole point of the seam
    // (a persona's perception/observe reaches an eye-node).
    #[tokio::test]
    async fn routes_a_provided_command_to_its_connected_provider() {
        let registry = Arc::new(ProviderRegistry::new());
        let calls = Arc::new(AtomicUsize::new(0));
        registry.register(
            &["perception/observe"],
            Arc::new(FakeEye {
                calls: calls.clone(),
            }),
        );

        let interceptor = ProvidedCommandInterceptor::new(registry);
        let outcome = interceptor
            .try_route(
                "perception/observe",
                &json!({ "target": "https://x" }),
                None,
            )
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
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "provider must be invoked exactly once"
        );
    }
}

mod registry {
    use crate::runtime::registry::ModuleRegistry;

    // what this catches: the registered-but-undispatchable class (#309 final layer —
    // work/list shipped advertised in help/suggestions/the tool offer while dispatch
    // refused it, live 2026-08-03). dispatch_orphans must read BOTH sides: with no
    // module registered, every dep-holding descriptor is an orphan (proves it reads
    // the descriptor registry, and that stateless self-registering commands are
    // exempt by construction); registering the work module clears ALL SEVEN work
    // verbs (proves objects clear orphans — and pins that commands() carries every
    // verb the descriptors advertise, the exact two-line gap that shipped).
    // what this catches: the audit counting a `Provided` command as undispatchable. Such a
    // command has NO ServiceModule by construction — an adapter is its server — so it can
    // never appear in `command_objects` or a prefix route, and no amount of module
    // registration will ever clear it. Counting it made the boot ERROR permanently noisy
    // (3 orphans, 2 of them healthy adapter-served commands) AND made the documented
    // promotion to a boot refusal unreachable, since the count could not reach zero.
    //
    // Asserted on a registry with NOTHING registered — the worst case for the audit, where
    // every module-served command IS an orphan. If a Provided name survives even there, the
    // filter is gone. Reverting the `wire != Provided` filter puts interface/screenshot back
    // in the list and this goes red.
    #[test]
    fn a_provided_command_is_never_an_orphan_because_it_has_no_module_to_be_missing_from() {
        let registry = ModuleRegistry::new();
        let orphans = registry.dispatch_orphans();

        // Every Provided descriptor in the registry, by shape — not a hand-kept name list,
        // so a newly-added adapter command inherits the guarantee.
        let provided: Vec<&'static str> = crate::sdk_codegen::command_registry()
            .into_iter()
            .filter(|d| d.wire == crate::sdk_codegen::WireShape::Provided)
            .map(|d| d.name)
            .collect();
        assert!(
            !provided.is_empty(),
            "positive control: the registry must contain at least one Provided command \
             (interface/screenshot), or this test proves nothing"
        );
        for name in provided {
            assert!(
                !orphans.contains(&name),
                "{name} is adapter-served (WireShape::Provided) — it has no module to be \
                 missing from, so the dispatch audit must not report it. orphans: {orphans:?}"
            );
        }
    }
}

mod recording_module {
    use crate::runtime::{CommandResult, Runtime};
    use async_trait::async_trait;
    use std::sync::Arc;

    // what this catches: the SOCKET route (`route_command`, used by uu / IPC / MCP)
    // reaches a connected Provided-command provider through the SAME
    // `provider_registry` the in-process persona route uses via the interceptor —
    // no dispatch path can gain (or lack) a capability the other has. Both call the
    // ONE `route_provided`. Regression-pins the "one infrastructure, every caller"
    // invariant for Provided commands (perception/observe) — the exact divergence
    // that let benchmarks take a different command path before.
    #[tokio::test]
    async fn route_command_reaches_the_provider_registry_for_a_provided_command() {
        use crate::runtime::ProvidedCommandProvider;
        use serde_json::json;

        struct FakeEye;
        #[async_trait]
        impl ProvidedCommandProvider for FakeEye {
            async fn fulfill(
                &self,
                command: &str,
                params: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Ok(json!({ "success": true, "command": command, "echoedTarget": params["target"] }))
            }
            fn label(&self) -> &str {
                "fake-eye"
            }
        }

        let runtime = Runtime::new();

        // No eye-node connected → the socket route fails loud, named (not "Unknown
        // command", not a fabricated observation).
        match runtime
            .route_command("perception/observe", json!({ "target": "https://x" }), None)
            .await
        {
            Some(Err(e)) => {
                assert!(e.contains("perception/observe"), "names the command: {e}");
                assert!(e.contains("eye-node"), "names the missing adapter: {e}");
            }
            _ => panic!("a Provided command with no provider must fail loud on the socket route"),
        }

        // Bind a provider on the SAME registry the IPC layer binds eye-nodes into.
        runtime
            .provider_registry()
            .register(&["perception/observe"], Arc::new(FakeEye));

        match runtime
            .route_command("perception/observe", json!({ "target": "https://x" }), None)
            .await
        {
            Some(Ok(CommandResult::Json(v))) => {
                assert_eq!(v["success"], true);
                assert_eq!(
                    v["echoedTarget"], "https://x",
                    "forwarded params reached the eye-node"
                );
            }
            _ => {
                panic!("the socket route must forward a Provided command to its connected provider")
            }
        }
    }
}

mod conformance {
    use crate::sdk_codegen::conformance::*;

    // what this catches (#163): a NATIVE tool that ships undiscoverable (no/short
    // description), unnameable (a caps/space a model can't emit), or unlearnable
    // (a non-object params schema) — the AI-usability floor. This is the living,
    // self-updating replacement for the hand-authored tool-defect list: a new
    // tool that violates the floor fails HERE the moment it lands, and the message
    // names the exact tool + fix. The audit runs over the live registry, so it can
    // never drift the way a checked-in list does.
    #[test]
    fn ai_safe_tools_meet_the_ai_usability_floor() {
        // Non-vacuity: the audit must actually have examined the surface. A green
        // result on zero tools would be a silently-broken gate, not conformance.
        let audited = audited_tool_count();
        assert!(
            audited >= 40,
            "conformance audit examined only {audited} AiSafe tools — expected the full \
             surface (~150). The registry filter is broken; the gate is checking nothing."
        );

        let violations = audit_tool_conformance();
        assert!(
            violations.is_empty(),
            "{} of {audited} AiSafe tool(s) fail the AI-usability floor — a persona can reach \
             them by name but cannot reliably use them:\n{}",
            violations.len(),
            violations
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

mod events {
    use crate::sdk_codegen::events::*;

    // what this catches: the typed string-free EventApi — on<Class>/emit<Class>
    // per event, class string only in the body, payload/handlers typed off EventMap.
    #[test]
    fn event_api_emits_typed_string_free_accessors() {
        let registry = event_registry();
        let out = generate_event_api(&registry, "./EventMap", "../Events");

        assert!(
            out.contains(
                "onContractProposed(handlers: EventHandlers<'contract:proposed'>, \
                 opts?: SubscribeOptions<'contract:proposed'>): Subscription {"
            ),
            "typed subscribe accessor, class camelCased:\n{out}"
        );
        assert!(
            out.contains(
                "emitContractProposed(payload: EventMap['contract:proposed']): Promise<void> {"
            ),
            "typed emit accessor:\n{out}"
        );
        assert!(
            out.contains("return this.events.subscribe('contract:proposed', handlers, opts);"),
            "class string lives once, in the body:\n{out}"
        );
        assert!(out.contains("import { Events } from '../Events';"));
        assert!(!out.contains("../../../"), "no escape path leaks");
    }

    // what this catches: the registry self-assembles from register_event! sites
    // (inventory) crate-wide, sorted for deterministic output.
    #[test]
    fn event_registry_is_auto_discovered_and_sorted() {
        let classes: Vec<&str> = event_registry().iter().map(|e| e.class).collect();
        assert!(classes.contains(&"contract:proposed"));
        assert!(classes.contains(&"contract:paid"));
        let mut sorted = classes.clone();
        sorted.sort();
        assert_eq!(classes, sorted, "registry sorted by class");
    }

    // what this catches: events are Rust-sourced end-to-end like commands — the
    // EventMap generates from registered EventSpecs against REAL ts-rs payloads
    // (the contract events), imports resolve to clean modules (no escape paths).
    #[test]
    fn generates_event_map_from_the_real_registry() {
        let registry = event_registry();
        assert!(!registry.is_empty(), "contract events are registered");
        let out = generate_event_map(&registry, "@protocol");

        assert!(
            out.contains("'contract:proposed': ContractProposedPayload;"),
            "real event keyed by class with its ts-rs payload:\n{out}"
        );
        assert!(out.contains("import type {"), "imports emitted");
        assert!(
            !out.contains("../../../"),
            "no ts-rs export escape path leaks"
        );
        assert!(
            out.contains("from '@protocol/contracts/"),
            "payload imports from its clean module under the TS root:\n{out}"
        );
        assert!(out.contains("export interface EventMap"));
        assert!(out.contains("export type EventClass = keyof EventMap;"));
    }
}

mod sdk_tests {
    use crate::sdk_codegen::*;

    // what this catches: EVERY registered command's descriptor must build without
    // panicking — i.e. its Params and Result are named TS types, not inline
    // primitives (`Vec<T>`, `Option<T>`, bare scalars). A single inline-primitive
    // output panics the whole `command_registry()` walk (regression for the
    // `cognition/semantic-search-tools` / `inbox/drain-frame` / `vision-describe`
    // sweep — a bare `Vec`/`Option` output that poisoned `commands/list`).
    #[test]
    fn every_registered_command_descriptor_builds() {
        // command_registry() itself panics on the first offender; walk the raw
        // registrations so a failure names ALL offenders, not just the first.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut offenders = Vec::new();
        for reg in inventory::iter::<CommandRegistration>() {
            if let Err(e) = std::panic::catch_unwind(reg.descriptor_fn) {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string());
                offenders.push(msg);
            }
        }
        std::panic::set_hook(prev);
        assert!(
            offenders.is_empty(),
            "{} command(s) have inline-primitive Params/Result — wrap in a named struct:\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }

    // what this catches: the FIRST universal AI-tool conformance invariant
    // (Joel 2026-07-13: "make it part of the tool base so a tool with xyz problem
    // CAN'T exist"). A tool a model cannot understand is a tool that will be
    // mis-called — the whole tool-defect ledger (silent no-op #159, glob-in-path
    // #160, corrupted-id #161) is the model reaching for a tool it couldn't read.
    // The base invariant, enforced over the WHOLE registry so no individual tool
    // has to remember and a new tool inherits it for free: every AI-facing (AiSafe)
    // command is SELF-DESCRIBING — a non-trivial, model-usable description. This is
    // the static seed of the harness; the BEHAVIORAL invariants (fuzzy-id
    // resolution, glob tolerance, fail-loud-with-suggestion — the #159/#160/#161
    // class) attach next as per-tool exams the generator scaffolds and a runner
    // executes here (and, later, in CI — "require the tools to work for AIs").
    // A non-compliant tool fails THIS test — it cannot be merged, so it cannot exist.
    #[test]
    fn every_ai_facing_command_is_self_describing() {
        // context-budget-exempt: a MINIMUM doc-length a command must meet to pass conformance — a quality bar on authored text, not a bound on anything the model reads
        const MIN_DESCRIPTION_CHARS: usize = 20;
        let offenders: Vec<String> = command_registry()
            .into_iter()
            .filter(|d| d.access_level == AccessLevel::AiSafe)
            .filter_map(|d| {
                let desc = d.description.trim();
                if desc.chars().count() < MIN_DESCRIPTION_CHARS {
                    Some(format!(
                        "  {} — description is {} chars (min {}): {:?}",
                        d.name,
                        desc.chars().count(),
                        MIN_DESCRIPTION_CHARS,
                        desc
                    ))
                } else {
                    None
                }
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "{} AI-facing command(s) are not self-describing — a model cannot use a \
             tool it cannot read (the root of the tool-defect ledger). Give each a \
             concrete, model-usable DESCRIPTION:\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }

    // what this catches (Joel's goal): the generated ACCESSOR API gives a typed
    // method per command with the command string baked into the BODY — call sites
    // are string-free and strongly typed straight from the Rust definition. Each
    // shape gets the faithful signature; the param/result types are the same ones
    // the CommandMap models, so a Rust type change propagates here too.
    #[test]
    fn command_api_emits_typed_string_free_accessors() {
        let registry = command_registry();
        let out = generate_command_api(&registry, "@protocol", "./Commands");

        // Enveloped: typed accessor, wrapped signature, string only in the body.
        // Post-migration the enveloped exemplar is the handle-minting inference open.
        assert!(
            out.contains(
                "aiInferenceOpen(params: CommandRequest<OpenParams>): \
                 Promise<CommandResponse<OpenResult>> {"
            ),
            "enveloped accessor is typed both ends:\n{out}"
        );
        // Bare: typed accessor, bare signature. `chat/send` migrated to Bare, so its
        // accessor is now bare both ends — no envelope.
        assert!(
            out.contains("chatSend(params: ChatSendWireParams): Promise<ChatSendResult> {"),
            "migrated chat/send accessor is bare both ends:\n{out}"
        );
        assert!(
            out.contains(
                "inferenceLlmRequest(params: InferenceRequest): Promise<InferenceResponse> {"
            ),
            "bare accessor is typed both ends, no envelope:\n{out}"
        );
        // Provided + multi-segment path camelCasing.
        assert!(
            out.contains(
                "interfaceScreenshot(params: ScreenshotParams): Promise<ScreenshotResult> {"
            ),
            "provided accessor present, path camelCased:\n{out}"
        );
        assert!(
            out.contains("aiInferenceOpen(params: CommandRequest<OpenParams>):"),
            "deep path camelCased (ai/inference/open → aiInferenceOpen):\n{out}"
        );

        // The command STRING appears only inside method bodies (execute call), and
        // the accessor delegates to the typed Commands facade.
        assert!(
            out.contains("return this.commands.execute('chat/send', params);"),
            "string key lives once, inside the body:\n{out}"
        );
        // No leaked escape paths; the facade is imported.
        assert!(!out.contains("../../../"), "no ts-rs escape path leaks");
        assert!(out.contains("import { Commands } from './Commands';"));
    }

    // what this catches: a real command round-trips to a descriptor at the type
    // level, carrying name + access + its transitive nested deps as importable refs.
    #[test]
    fn descriptor_captures_real_command_and_nested_deps() {
        let d = command_registry()
            .into_iter()
            .find(|d| d.name == "inference/llm/request")
            .expect("inference/llm/request registered");
        assert_eq!(d.access_level, AccessLevel::AiSafe);
        assert_eq!(d.params.name, "InferenceRequest");
        assert_eq!(d.result.name, "InferenceResponse");
        // InferenceResponse nests InferenceComplete + FirstTokenEmitted — the
        // transitive closure must capture them as importable refs.
        assert!(
            d.type_refs.iter().any(|t| t.name == "InferenceComplete"),
            "nested dependency captured (transitive closure)"
        );
    }

    // what this catches: the Rust-source → TS CommandMap generation end-to-end,
    // validated against REAL registered commands (no fictional fixtures). Their
    // types carry the production export_to form, so this exercises module_of on
    // real data: the generated imports must resolve (NO `../../../` escapes) and
    // reference the single-source modules under the TS root.
    #[test]
    fn generates_command_map_from_the_real_registry() {
        let registry = command_registry();
        assert!(
            !registry.is_empty(),
            "real commands are registered (the sampling)"
        );
        let out = generate_command_map(&registry, "@protocol");

        // CRITICAL: imports resolve — module_of stripped the export escape path.
        assert!(out.contains("import type {"), "imports emitted");
        assert!(
            !out.contains("../../../"),
            "no ts-rs export escape path leaks into the generated imports"
        );
        assert!(out.contains("export interface CommandMap"));
        assert!(out.contains("export type CommandName = keyof CommandMap;"));
        assert!(out.starts_with("// GENERATED from the Rust command registry"));
    }

    // what this catches: a Bare/Provided-only registry must NOT import or use the
    // envelope generics — the wrapping is conditional on an Enveloped command, so a
    // surface with none stays envelope-free.
    #[test]
    fn non_enveloped_registry_omits_envelope_imports() {
        let bare_and_provided: Vec<CommandDescriptor> = command_registry()
            .into_iter()
            .filter(|d| !d.wire.is_enveloped())
            .collect();
        assert!(
            bare_and_provided
                .iter()
                .any(|d| d.wire == WireShape::Provided)
                && bare_and_provided.iter().any(|d| d.wire == WireShape::Bare),
            "the sampling has both a Bare and a Provided command"
        );
        let out = generate_command_map(&bare_and_provided, "@protocol");
        // Check imports + usages, not the explanatory JSDoc preamble (which names
        // the generics regardless of shape).
        assert!(
            !out.contains("import type { CommandRequest }")
                && !out.contains("import type { CommandResponse }"),
            "no envelope import when nothing is Enveloped:\n{out}"
        );
        assert!(
            !out.contains("params: CommandRequest<") && !out.contains("result: CommandResponse<"),
            "no envelope wrapping in the map entries when nothing is Enveloped:\n{out}"
        );
    }

    // what this catches: the registry self-assembles from register_command! sites
    // (inventory) crate-wide — the real inference command appears though nothing
    // lists it centrally — and is sorted for deterministic output.
    #[test]
    fn registry_is_auto_discovered_with_no_central_list() {
        let names: Vec<&str> = command_registry().iter().map(|d| d.name).collect();
        assert!(
            names.contains(&"inference/llm/request"),
            "a real command, registered at its own site, is auto-discovered"
        );
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(
            names, sorted,
            "registry sorted by name (deterministic output)"
        );
    }

    // what this catches (the heart of the slice): the THREE wire shapes each
    // produce the FAITHFUL type from one CommandMap, modeling exactly what the real
    // handler exchanges (the lie the prior pass shipped was modeling a bare handler
    // as enveloped). The sampling spans all three:
    //   - Bare      (inference/llm/request, chat/send): handler returns bare types
    //   - Provided  (interface/screenshot):  adapter exchanges bare types
    //   - Enveloped (ai/inference/open): handle-minting handler rides the envelope
    #[test]
    fn wire_shape_decides_envelope_wrapping_faithfully() {
        let registry = command_registry();
        let out = generate_command_map(&registry, "@protocol");

        // Bare substrate command — bare both sides (NOT wrapped). This is the
        // assertion that would have caught the prior lie.
        assert!(
            out.contains(
                "'inference/llm/request': { params: InferenceRequest; result: InferenceResponse }"
            ),
            "Bare command must NOT be enveloped (it returns bare):\n{out}"
        );
        assert!(
            !out.contains("CommandRequest<InferenceRequest>")
                && !out.contains("CommandResponse<InferenceResponse>"),
            "the envelope must NOT wrap the bare inference command"
        );

        // A provided adapter must expose its real input contract to model tools,
        // not merely have a correct TypeScript name (live regression: PR #4626).
        let screenshot = registry.iter().find(|d| d.name == "interface/screenshot").unwrap();
        let properties = screenshot.params_schema["properties"].as_object().expect("capture parameters must be discoverable");
        for field in ["querySelector", "format", "quality", "width", "height", "scale", "destination", "filename"] {
            assert!(properties.contains_key(field), "missing screenshot input: {field}");
        }
        assert!(screenshot.params_schema["required"].as_array().is_none_or(|fields| fields.is_empty()), "whole-surface capture still accepts no arguments");

        // Provided adapter command — bare both sides.
        assert!(
            out.contains(
                "'interface/screenshot': { params: ScreenshotParams; result: ScreenshotResult }"
            ),
            "Provided command must stay bare:\n{out}"
        );

        // `chat/send` migrated to the typed DynCommand registry, so its
        // `ActionCommand` blanket-impls `WireShape::Bare` — bare both sides, NOT
        // enveloped. This pins the migration: the moment chat/send regresses back to
        // an envelope wrapper, this fails.
        assert!(
            // Params renamed ChatSendParams → ChatSendWireParams (2026-08-31: senderId
            // went optional at the wire; the kernel keeps the resolved struct).
            out.contains("'chat/send': { params: ChatSendWireParams; result: ChatSendResult }"),
            "migrated chat/send must be Bare (no envelope):\n{out}"
        );
        assert!(
            !out.contains("CommandRequest<ChatSendWireParams>")
                && !out.contains("CommandResponse<ChatSendResult>"),
            "the envelope must NOT wrap the migrated bare chat/send"
        );

        // Enveloped substrate command — wrapped both sides (faithful: the handler
        // really emits CommandResponse + parses CommandRequest). Post-migration the
        // remaining Enveloped surface is the handle-minting inference family.
        assert!(
            out.contains(
                "'ai/inference/open': { params: CommandRequest<OpenParams>; \
                 result: CommandResponse<OpenResult> }"
            ),
            "Enveloped handle-minting command must be wrapped:\n{out}"
        );

        // The envelope generics are imported once (Enveloped commands exist), and
        // those modules carry HandleRef themselves so the map needn't.
        assert!(
            out.contains("import type { CommandRequest } from '@protocol/runtime/CommandRequest';"),
            "envelope request generic imported from its single-source module:\n{out}"
        );
        assert!(
            out.contains(
                "import type { CommandResponse } from '@protocol/runtime/CommandResponse';"
            ),
            "envelope response generic imported:\n{out}"
        );
    }
}

