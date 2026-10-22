#![cfg(test)]
    use super::*;
    use std::path::Path;

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

    // what this catches (the review's CRITICAL #4): real ts-rs types carry the
    // `../../../protocol/typescript/...` export escape path; module_of must resolve
    // them to clean importable modules. Earlier demo types (no export_to) hid this.
    #[test]
    fn module_of_strips_the_ts_rs_export_escape_path() {
        assert_eq!(
            module_of(Path::new("../../../protocol/typescript/ai/ModelInfo.ts")),
            "ai/ModelInfo",
            "escape-hatch export_to → clean module relative to the TS root"
        );
        assert_eq!(
            module_of(Path::new(
                "../../../protocol/typescript/inference_llm/InferenceRequest.ts"
            )),
            "inference_llm/InferenceRequest"
        );
        // No export_to (bare derived name) → its stem.
        assert_eq!(module_of(Path::new("PingParams.ts")), "PingParams");
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
