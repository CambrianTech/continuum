//! Writing the generated TypeScript SDK to disk — the step that makes the
//! generator LIVE instead of test-only output.
//!
//! Per the chosen model (vendor): the SDK stays self-contained — it does NOT
//! reach into `protocol/typescript`. Instead the generator COPIES the transitive
//! closure of ts-rs wire types each registered command needs into the SDK's own
//! `generated/wire/` tree, then emits the `CommandMap` + the typed `CommandApi`
//! importing them locally. So a downstream app `npm install`s one self-contained
//! `@continuum/sdk-typescript` with no cross-package wiring, yet the types are
//! still single-sourced from Rust (the copy is mechanical, never hand-edited).
//!
//! The vendored tree MIRRORS the protocol layout (`wire/chat/ChatSendParams.ts`,
//! `wire/runtime/CommandRequest.ts`), so every relative import inside a copied
//! file (`./ScreenshotFormat`, `./HandleRef`) still resolves. The copier follows
//! those imports transitively, so the closure is complete and self-consistent
//! regardless of how deep the type graph goes.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::events::{generate_event_api, generate_event_map, EventDescriptor};
use super::{generate_command_api, generate_command_map, CommandDescriptor};

/// Where the vendored wire types land inside the SDK's generated dir, and the
/// import base the emitted map/api use to reach them.
const WIRE_SUBDIR: &str = "wire";

/// Write the full TypeScript SDK surface (BOTH primitives) into `out_dir` (the
/// SDK's `generated/` directory), vendoring every wire type from `protocol_dir`.
///
/// Produces:
/// - `out_dir/wire/**` — the transitive closure of ts-rs `.ts` types the commands
///   AND events reference, copied from `protocol_dir`, structure preserved.
/// - `out_dir/CommandMap.ts` + `out_dir/CommandApi.ts` — the command surface.
/// - `out_dir/EventMap.ts` + `out_dir/EventApi.ts` — the event surface.
///
/// Pure mechanical projection of the registries + the ts-rs output — no hand
/// editing, so a Rust type/event change reflows the whole SDK on regeneration.
pub fn write_typescript_sdk(
    commands: &[CommandDescriptor],
    events: &[EventDescriptor],
    protocol_dir: &Path,
    out_dir: &Path,
) -> io::Result<()> {
    // 1. Seed module set: every type each command references (transitive type
    //    closure already captured in `type_refs`) + the envelope generics when any
    //    command is Enveloped (HandleRef rides in via their own imports) + each
    //    event's payload (its own transitive imports are followed during vendoring).
    let mut seeds: BTreeSet<String> = BTreeSet::new();
    for c in commands {
        for t in &c.type_refs {
            seeds.insert(t.module.clone());
        }
    }
    if commands.iter().any(|c| c.wire.is_enveloped()) {
        seeds.insert("runtime/CommandRequest".to_string());
        seeds.insert("runtime/CommandResponse".to_string());
    }
    for e in events {
        seeds.insert(e.payload.module.clone());
    }
    // The thin-client WS transport envelope (task #29): always vendored, because
    // it's infrastructure the `WebSocketTransport` speaks directly — NOT derived
    // from any single command's params. Seeding it here pulls in
    // AircCommandRequest/AircCommandResponse transitively (their own relative
    // imports are followed by the vendorer), so the transport imports generated
    // wire types instead of hand-writing the wire shape ([[the-compression-principle]]).
    seeds.insert("transport/WsClientMessage".to_string());
    seeds.insert("transport/WsServerMessage".to_string());

    // 2. Vendor: copy each seed file + everything it transitively imports.
    //
    //    ATOMIC swap: vendor into a STAGING dir first, then replace `wire/` only
    //    once the whole transitive closure copied successfully. A missing/renamed
    //    protocol binding (a stale `protocol/typescript`) must fail LOUD WITHOUT
    //    destroying the committed `wire/` tree — the old "remove_dir_all then error
    //    mid-copy" shape left the repo's generated SDK deleted on any failure,
    //    forcing a manual `git checkout` to recover. Staging makes a re-run safe.
    let wire_dir = out_dir.join(WIRE_SUBDIR);
    let staging = out_dir.join(".wire.staging");
    let _ = fs::remove_dir_all(&staging);
    let mut copied: BTreeSet<String> = BTreeSet::new();
    for module in &seeds {
        if let Err(e) = copy_with_deps(module, protocol_dir, &staging, &mut copied) {
            // Drop the partial staging tree so a retry starts clean; the committed
            // `wire/` was never touched.
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    }
    // Closure complete — now (and only now) swap the freshly-built tree in.
    let _ = fs::remove_dir_all(&wire_dir);
    fs::rename(&staging, &wire_dir)?;

    // 3. Emit both primitives' surfaces, importing the vendored tree locally.
    let import_base = format!("./{WIRE_SUBDIR}");
    fs::create_dir_all(out_dir)?;
    fs::write(
        out_dir.join("CommandMap.ts"),
        generate_command_map(commands, &import_base),
    )?;
    fs::write(
        // EventMap.ts: class -> payload, importing the vendored payloads.
        out_dir.join("EventMap.ts"),
        generate_event_map(events, &import_base),
    )?;
    fs::write(
        // EventApi.ts sits in generated/; EventMap is a sibling, Events one dir up.
        out_dir.join("EventApi.ts"),
        generate_event_api(events, "./EventMap", "../Events"),
    )?;
    fs::write(
        out_dir.join("CommandApi.ts"),
        // CommandApi.ts sits in generated/, the Commands facade one dir up.
        generate_command_api(commands, &import_base, "../Commands"),
    )?;
    Ok(())
}

/// Copy `module` (`.ts` implied) from `protocol_dir` into `wire_dir`, preserving
/// its relative path, then recurse into every RELATIVE import it declares so the
/// vendored tree is closed (no dangling imports). `copied` dedupes / breaks
/// cycles.
fn copy_with_deps(
    module: &str,
    protocol_dir: &Path,
    wire_dir: &Path,
    copied: &mut BTreeSet<String>,
) -> io::Result<()> {
    if !copied.insert(module.to_string()) {
        return Ok(());
    }
    let src = protocol_dir.join(format!("{module}.ts"));
    let contents = fs::read_to_string(&src).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("vendoring wire type '{module}' from {}: {e}", src.display()),
        )
    })?;

    let dst = wire_dir.join(format!("{module}.ts"));
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&dst, &contents)?;

    // Follow each relative import, resolved against this module's directory.
    let module_dir = Path::new(module).parent().map(Path::to_path_buf);
    for spec in relative_import_specs(&contents) {
        if let Some(dep_module) = resolve_relative_module(module_dir.as_deref(), &spec) {
            copy_with_deps(&dep_module, protocol_dir, wire_dir, copied)?;
        }
    }
    Ok(())
}

/// Extract the RELATIVE import specifiers (those starting with `.`) from a TS
/// file — both `from './X'` and `from "./X"`. Bare/package specifiers are
/// ignored (vendored types only import each other relatively).
fn relative_import_specs(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in contents.split("from ").skip(1) {
        let raw = raw.trim_start();
        let (quote, rest) = match raw.split_at_checked(1) {
            Some(("'", rest)) => ('\'', rest),
            Some(("\"", rest)) => ('"', rest),
            _ => continue,
        };
        if let Some(end) = rest.find(quote) {
            let spec = &rest[..end];
            if spec.starts_with('.') {
                out.push(spec.to_string());
            }
        }
    }
    out
}

/// Resolve a relative import specifier (`./HandleRef`, `../ai/ToolCall`) against
/// the importing module's directory into a normalized `module` key (no `.ts`),
/// e.g. (`runtime`, `./HandleRef`) → `runtime/HandleRef`. Returns `None` if it
/// escapes above the vendored root (shouldn't happen for protocol types).
fn resolve_relative_module(module_dir: Option<&Path>, spec: &str) -> Option<String> {
    let mut parts: Vec<String> = match module_dir {
        Some(dir) => dir
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect(),
        None => Vec::new(),
    };
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other.trim_end_matches(".ts").to_string()),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(
        parts
            .iter()
            .map(PathBuf::from)
            .fold(PathBuf::new(), |acc, p| acc.join(p))
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{command_registry, event_registry};

    fn protocol_dir() -> PathBuf {
        // core/continuum-core → ../../protocol/typescript
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/typescript")
    }

    /// Regenerates the committed TypeScript SDK from the live registry — the same
    /// way ts-rs's `export_bindings_*` tests regenerate `protocol/typescript`.
    /// Running `cargo test` keeps `sdk/typescript/generated/{CommandMap,CommandApi}.ts`
    /// + `wire/**` in sync with the Rust command declarations; the output is
    /// committed. Only writes when the protocol bindings exist (they're produced by
    /// the export_bindings tests in the same run / a prior one).
    ///
    /// Writes BOTH primitives' generated surfaces (CommandMap/CommandApi +
    /// EventMap/EventApi) + the vendored wire tree, all from the Rust registries.
    #[test]
    fn generates_live_typescript_sdk() {
        let protocol = protocol_dir();
        if !protocol.exists() {
            return;
        }
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/typescript/generated");
        write_typescript_sdk(&command_registry(), &event_registry(), &protocol, &out)
            .expect("regenerate the committed TypeScript SDK");
    }

    // what this catches: the generator actually WRITES a self-contained SDK —
    // CommandMap + CommandApi + a vendored wire tree — and the vendored tree is
    // CLOSED (every relative import in every copied file resolves to another
    // copied file). A dangling vendored import would break the SDK's standalone
    // compile; this is the structural guard that the "vendor" model holds.
    #[test]
    fn writes_self_contained_sdk_with_closed_wire_tree() {
        let protocol = protocol_dir();
        if !protocol.exists() {
            // ts-rs bindings not generated in this checkout — skip rather than
            // fail (the export_bindings tests produce them).
            return;
        }
        let out = std::env::temp_dir().join("continuum_sdk_emit_closed_tree_test");
        let _ = fs::remove_dir_all(&out);

        write_typescript_sdk(&command_registry(), &event_registry(), &protocol, &out)
            .expect("emit SDK");

        // Top-level surface written.
        let map = fs::read_to_string(out.join("CommandMap.ts")).expect("CommandMap.ts");
        let api = fs::read_to_string(out.join("CommandApi.ts")).expect("CommandApi.ts");
        assert!(map.contains("export interface CommandMap"));
        assert!(api.contains("export class CommandApi"));
        // The map/api import the vendored tree, never protocol/ or escape paths.
        assert!(map.contains("from './wire/"), "map imports vendored types:\n{map}");
        assert!(!map.contains("../../../"), "no escape path leaks into the SDK");
        assert!(api.contains("from '../Commands'"), "api reaches the facade one dir up");

        // The enveloped commands' generics were vendored, and (critically) the
        // file THEY import — HandleRef — was followed transitively into the tree.
        let wire = out.join("wire");
        assert!(wire.join("runtime/CommandRequest.ts").exists(), "envelope vendored");
        assert!(
            wire.join("runtime/HandleRef.ts").exists(),
            "transitive import (CommandRequest → HandleRef) was followed"
        );
        assert!(wire.join("chat/ChatSendParams.ts").exists(), "a command param vendored");

        // CLOSURE GUARD: walk every vendored file and assert each relative import
        // points at a file that was also vendored. This is the real correctness
        // property of the copier.
        assert_closed(&wire);

        let _ = fs::remove_dir_all(&out);
    }

    fn assert_closed(wire: &Path) {
        let mut stack = vec![wire.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).expect("read wire dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("ts") {
                    continue;
                }
                let contents = fs::read_to_string(&path).expect("read vendored file");
                let module_dir = path
                    .parent()
                    .unwrap()
                    .strip_prefix(wire)
                    .unwrap()
                    .to_path_buf();
                for spec in relative_import_specs(&contents) {
                    let dep = resolve_relative_module(
                        Some(module_dir.as_path()).filter(|p| !p.as_os_str().is_empty()),
                        &spec,
                    )
                    .unwrap_or_else(|| panic!("unresolvable import {spec} in {}", path.display()));
                    assert!(
                        wire.join(format!("{dep}.ts")).exists(),
                        "dangling vendored import: {} → {spec} (resolved {dep}) not copied",
                        path.display()
                    );
                }
            }
        }
    }
}
