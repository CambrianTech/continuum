#![cfg(test)]
//! ─── Module-wiring audit (#344) ────────────────────────────────────
//!
//! `dispatch_orphans()` (runtime/registry.rs) audits DESCRIPTOR → dispatch. It cannot see
//! the layer under it: a module that ships NO descriptors at all. `impl
//! ServiceModule for X` registers NOTHING — the MODULE itself must be
//! handed to `register()` at a boot site (`runtime/registry.rs` +
//! `ipc/mod.rs`, cf. `RoomModule`). Nothing in the trait says so, and
//! `ProbeStreamModule` — 675 lines, a handle store, a broadcast
//! subscriber, its own green test mod, `command_prefixes:
//! &["debug/probes/"]` — has been unreachable since the day it landed
//! (#362). Compile, unit tests, and the task list all said done.
//!
//! The predicate here is deliberately NOT "is the name mentioned
//! anywhere". That version passes `ProbeStreamModule`, because the only
//! mention outside its own file is a DOC COMMENT in `probe_query.rs`.
//! Prose made a dead module look wired — the precise failure this audit
//! exists to catch. Comments are stripped before the search.

/// A module that is intentionally not registered, with the reason. An
/// entry here is a DECLARATION, not a mute: unregistered-and-undeclared
/// fails the test, and a declared entry that becomes registered ALSO
/// fails, so the list cannot rot into a graveyard.
struct Unwired {
    module: &'static str,
    why: &'static str,
}

/// Categories, in the order they were derived from the live tree:
///
/// - **fixture** — exists only to drive a test. Production binaries
///   never construct it.
/// - **staging** — a real module deliberately landed before its wiring
///   slice. MUST name the task that wires it. This is the category the
///   audit was worth building for: staging is indistinguishable from a
///   defect unless it is *declared*.
/// - **shadowed** — superseded by another module that serves the same
///   surface; dead code, not a live break.
/// - **defect** — known-broken, with the task tracking the fix. Loud on
///   purpose: the guard refuses to let it be forgotten.
const UNWIRED: &[Unwired] = &[
    Unwired {
        module: "BarrierModule",
        why: "fixture: barrier for the command-executor concurrency test",
    },
    Unwired {
        module: "UndeclaredCommandModule",
        why: "fixture: half-declared command (constructor, no descriptor) — \
              the boot guard's positive control",
    },
    Unwired {
        module: "DefaultsModule",
        why: "fixture: exercises ModuleConfig trait defaults",
    },
    Unwired {
        module: "GreeterModule",
        why: "fixture: the module_harness worked example",
    },
    Unwired {
        module: "InferenceRecorder",
        why: "fixture: captures llm_module_service calls",
    },
    Unwired {
        module: "OptedInModule",
        why: "fixture: sibling of DefaultsModule, opts into every hook",
    },
    Unwired {
        module: "ReadyModule",
        why: "fixture: runtime readiness-gate test",
    },
    Unwired {
        module: "SlowJoiner",
        why: "fixture: a module that saves and then cannot be joined inside the                   phase bound — drives ModuleStopOutcome::JoinTimedOut, which before it                   existed only as a literal in assertions",
    },
    Unwired {
        module: "SlowSaver",
        why: "fixture: a module that cannot save inside the phase bound — drives                   ModuleStopOutcome::SaveTimedOut, the outcome the stop exit code keys on",
    },
    Unwired {
        module: "StubAircModule",
        why: "fixture: ChatModule's airc stand-in",
    },
    Unwired {
        module: "StubDataModule",
        why: "fixture: ChatModule's data stand-in",
    },
    Unwired {
        module: "TestModule",
        why: "fixture: this file's own routing tests",
    },
    Unwired {
        module: "HippocampusModule",
        why: "staging: BrainRegion skeleton, command_prefixes is empty by \
              design until slice L0-3a.1b migrates memory/* over from \
              MemoryModule. Registering it today would add a tick that \
              does nothing.",
    },
    // ProbeStreamModule's DEFECT entry (#362) lived here for exactly one
    // commit — the fix registers it in ipc/mod.rs against the router
    // installed by install_probe_tracing, and this audit's staleness
    // check is what forced the entry's removal in the same change.
];

fn mentions_ident(haystack: &str, ident: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(ident).any(|(idx, _)| {
        let before_ok = idx == 0 || {
            let c = bytes[idx - 1] as char;
            !c.is_alphanumeric() && c != '_'
        };
        let end = idx + ident.len();
        let after_ok = end >= bytes.len() || {
            let c = bytes[end] as char;
            !c.is_alphanumeric() && c != '_'
        };
        before_ok && after_ok
    })
}

#[test]
// what this catches: a ServiceModule that is implemented, tested and
// green but handed to `register()` nowhere — so its handle_command,
// commands() and tick never reach the runtime. Regression for #362
// (ProbeStreamModule) and #325 (GeneratorModule, which shipped
// unregistered and was found only by live dispatch).
fn every_service_module_is_registered_or_declares_why_not() {
    let files = crate::test_env::source_tree::crate_sources();
    assert!(
        files.len() > 100,
        "source scan found only {} files — the walk is broken, and a \
         guard that cannot see the tree would pass vacuously",
        files.len()
    );

    // Keyed by the OWNING file: a test module moved to a sibling `tests.rs` is
    // still its owner's own file, never a reference from outside it. Comments are
    // dropped so a doc line can never stand in for real wiring.
    let code: Vec<(std::path::PathBuf, String)> = files
        .iter()
        .map(|f| (f.owner(), f.code()))
        .collect();

    // (module name, defining file) for every `impl ServiceModule for X`.
    const NEEDLE: &str = "impl ServiceModule for ";
    let mut impls: Vec<(String, std::path::PathBuf)> = Vec::new();
    for (path, text) in &code {
        for (idx, _) in text.match_indices(NEEDLE) {
            let rest = &text[idx + NEEDLE.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                impls.push((name, path.clone()));
            }
        }
    }
    assert!(
        impls.len() > 50,
        "found only {} ServiceModule impls — the scan regressed",
        impls.len()
    );

    // THIS file is excluded from the haystack (a test file's owner is `test_env.rs`, so that
    // excludes its sibling test files too: they name real modules to exercise them, not to wire them). The UNWIRED table below
    // names every declared module as a string literal, and a literal is
    // code, not a comment — so without this exclusion each declaration
    // would read as a reference, every entry would look wired, and the
    // staleness check would fire on the whole list. The guard would have
    // been its own first false positive.
    let this_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test_env.rs");

    let unwired: Vec<String> = impls
        .iter()
        .filter(|(name, home)| {
            !code
                .iter()
                .any(|(p, c)| p != home && *p != this_file && mentions_ident(c, name))
        })
        .map(|(name, _)| name.clone())
        .collect();

    let declared: std::collections::HashSet<&str> = UNWIRED.iter().map(|u| u.module).collect();

    let undeclared: Vec<&String> = unwired
        .iter()
        .filter(|n| !declared.contains(n.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "these ServiceModules are implemented but never registered, and \
         nothing declares why:\n  {}\n\nA module reaches dispatch ONLY if \
         the MODULE is handed to register() at a boot site (ipc/mod.rs, \
         cf. RoomModule) — `impl ServiceModule` registers nothing. Either \
         wire it, or add an UNWIRED entry in this file naming the \
         category and the task.",
        undeclared
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let stale: Vec<&str> = UNWIRED
        .iter()
        .map(|u| u.module)
        .filter(|m| !unwired.iter().any(|n| n == m))
        .collect();
    assert!(
        stale.is_empty(),
        "these modules are declared unwired but ARE now referenced \
         outside their own file — delete their UNWIRED entry so the list \
         stays honest:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
// what this catches: a ModuleSpec left in MODULES after its module was deleted. required_modules()
// then demands a name nothing registers, and a full-citizen boot refuses to start ("Module
// registration incomplete ... missing [agent]": canary 107e10ddc on the M5, 2026-10-31, fixed by
// #5673; runtime.rs records the same trap for `search` and `inference`). The check above looks the
// other way (a module with no registration); this one closes the pair: every spec names a module
// whose config still declares it in production code. Card 39a5c0f6.
fn every_required_module_spec_names_a_declared_module() {
    let files = crate::test_env::source_tree::crate_sources();
    assert!(files.len() > 100, "source scan found only {} files: the walk is broken", files.len());

    // The `name: "..."` of every `ModuleConfig { .. }` literal in production code (comments and
    // test modules dropped by `code()`): the names a boot can actually register.
    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for file in &files {
        let code = file.code();
        for (at, _) in code.match_indices("ModuleConfig {") {
            let body = &code[at..];
            let body = &body[..body.find('}').unwrap_or(body.len())]; // unwrap_or: an unclosed literal is read to the end of the file
            if let Some(name_at) = body.find("name: \"") {
                let rest = &body[name_at + "name: \"".len()..];
                if let Some(end) = rest.find('"') {
                    declared.insert(rest[..end].to_string());
                }
            }
        }
    }
    assert!(declared.len() > 50, "found only {} module config names: the scan regressed", declared.len());

    let missing: Vec<&str> = crate::runtime::runtime::MODULES
        .iter()
        .map(|spec| spec.name)
        .filter(|name| !declared.contains(*name))
        .collect();
    assert!(
        missing.is_empty(),
        "these ModuleSpec rows name a module no production ModuleConfig declares, so a boot that \
         requires them refuses to start:\n  {}\n\nDelete the spec with its module (runtime.rs \
         MODULES), or restore the module.",
        missing.join("\n  ")
    );
}

#[test]
// what this catches: an UNWIRED entry added as a silent mute. Every
// exemption must carry a reason, and the two categories that are not
// self-evidently harmless (staging, defect) must name the task that
// resolves them — the requirement the #344 audit found most valuable.
fn every_unwired_declaration_carries_a_reason() {
    for entry in UNWIRED {
        assert!(
            entry.why.len() > 20,
            "{}: UNWIRED entries need a real reason, not a placeholder",
            entry.module
        );
        let category = entry
            .why
            .split(':')
            .next()
            .expect("split always yields one element")
            .trim()
            .to_ascii_lowercase();
        assert!(
            ["fixture", "staging", "shadowed", "defect"]
                .iter()
                .any(|c| category.starts_with(c)),
            "{}: reason must start with one of fixture/staging/shadowed/\
             defect — got {category:?}",
            entry.module
        );
        if category.starts_with("staging") || category.starts_with("defect") {
            assert!(
                entry.why.contains('#') || entry.why.contains("slice"),
                "{}: a {category} entry must name the task or slice that \
                 resolves it — otherwise it is indistinguishable from a \
                 forgotten defect",
                entry.module
            );
        }
    }
}
