//! `command/migrate` — port a legacy `handle_command` match arm onto the macro.
//!
//! The bulk of the consolidation is moving ~200 commands off the stringly
//! `ServiceModule::handle_command` match and onto the typed `action_command!` path.
//! This command reads a module's source, finds the arm for a given wire name,
//! captures the legacy body, and scaffolds the dep-holding command file with that
//! body carried inline as a `TODO(migrate)` block — so porting a command becomes
//! "review the captured body, wire it up, drop the arm, rebuild."
//!
//! It deliberately does NOT auto-transplant the legacy code as live `run` body:
//! legacy arms read `self.<field>` and call private handlers, which don't compile in
//! the new file unchanged. Emitting a fail-loud stub WITH the legacy code inline is
//! honest — nothing fake-compiles as done — while still removing the boilerplate.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ident::CommandIdent;
use super::scaffold::{render_command_file, Access, Form, ScaffoldOpts};
use super::wiring::{ensure_mod_lines, resolve_src_root, write_command_file};
use crate::sdk_codegen::{CommandError, Ctx};

/// Params for `command/migrate`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/command/CommandMigrateParams.ts"
)]
pub struct CommandMigrateParams {
    /// The legacy wire name to port, e.g. `data/list`.
    pub command: String,
    /// The module file stem under `modules/` that owns it, e.g. `data` (reads
    /// `modules/data.rs`).
    pub module: String,
    /// The state type the command captures. Defaults to `<Module>State` (the
    /// codebase convention: `DataModule` ⟶ `DataState`).
    #[serde(default)]
    #[ts(optional)]
    pub state_type: Option<String>,
    /// Access level for the ported command (default `privileged` — a migration stub
    /// stays restricted until the author confirms and opens it).
    #[serde(default)]
    #[ts(optional)]
    pub access: Option<String>,
    /// Overwrite an existing file (default: refuse).
    #[serde(default)]
    pub force: bool,
    /// Override the continuum-core `src/` root (default: the crate this binary was
    /// built from).
    #[serde(default)]
    #[ts(optional)]
    pub src_root: Option<String>,
}

/// Result of `command/migrate`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/command/CommandMigrateResult.ts"
)]
pub struct CommandMigrateResult {
    /// The wire name ported.
    pub command: String,
    /// Absolute path of the scaffolded file.
    pub file_path: String,
    /// `mod.rs` files created or edited.
    pub mod_files_touched: Vec<String>,
    /// Whether a legacy match arm was found and captured (false ⟹ a bare stub; the
    /// command may live elsewhere or use a different name spelling).
    pub legacy_arm_found: bool,
    /// The manual steps to finish the port.
    pub next_steps: Vec<String>,
}

crate::action_command! {
    /// Port a legacy handle_command match arm onto the action_command! macro: reads
    /// the module source, captures the arm's body inline as a TODO(migrate) block,
    /// and scaffolds the dep-holding command file + pub-mod wiring. Finish by porting
    /// the captured body and deleting the arm.
    pub struct CommandMigrate;
    name: "command/migrate",
    access: Privileged,
    params: CommandMigrateParams,
    output: CommandMigrateResult,
    run(_this, _ctx, p) => {
        migrate(p).await
    }
}

pub(crate) async fn migrate(p: CommandMigrateParams) -> Result<CommandMigrateResult, CommandError> {
    let id = CommandIdent::parse(&p.command)?;
    let access = match p.access.as_deref() {
        Some(s) => Access::parse(s)?,
        None => Access::Privileged,
    };
    let state_type = p
        .state_type
        .clone()
        .unwrap_or_else(|| format!("{}State", pascal(&p.module)));
    let module = p.module.clone();

    // Read the legacy module + capture the arm body off the async runtime.
    let src_override = p.src_root.clone();
    let module_for_fs = p.module.clone();
    let command_for_fs = p.command.clone();
    let id_for_fs = id.clone();
    let force = p.force;
    let state_type_for_fs = state_type.clone();
    let access_for_fs = access;
    let module_for_render = module.clone();

    let outcome: MigrateFsOutcome = tokio::task::spawn_blocking(move || {
        let root = resolve_src_root(src_override.as_deref())?;
        let module_path = root.join(format!("modules/{module_for_fs}.rs"));
        let legacy = std::fs::read_to_string(&module_path)
            .map_err(|e| CommandError::NotFound(format!("read {}: {e}", module_path.display())))?;
        let captured = extract_match_arm(&legacy, &command_for_fs);

        let body = build_stub_body(&id_for_fs.name, captured.as_deref());
        let opts = ScaffoldOpts {
            form: Form::DepHolding {
                state_type: state_type_for_fs,
                module: module_for_render,
            },
            access: access_for_fs,
            description: format!("TODO(migrate): describe `{}`.", id_for_fs.name),
            body: Some(body),
        };
        let content = render_command_file(&id_for_fs, &opts);
        let path = write_command_file(&root, &id_for_fs, &content, force)?;
        let mods = ensure_mod_lines(&root, &id_for_fs)?;
        Ok::<_, CommandError>(MigrateFsOutcome {
            file_path: path.display().to_string(),
            mod_files: mods.into_iter().map(|p| p.display().to_string()).collect(),
            legacy_arm_found: captured.is_some(),
        })
    })
    .await
    .map_err(|e| CommandError::Internal(format!("migrate task panicked: {e}")))??;

    let cat = id.segments[0].replace('-', "_");
    let mut next_steps = vec![
        format!(
            "Port the captured body in {} (self.* → this.state.*; replace the stub).",
            outcome.file_path
        ),
        format!(
            "Expose it: add `Arc::new({} {{ state: state.clone() }})` to \
             `commands::{cat}::command_objects(state)`, and include that in \
             `{}Module::commands()`.",
            id.struct_name,
            pascal(&module)
        ),
        format!(
            "Delete the `\"{}\" => ...` arm from modules/{}.rs::handle_command, then \
             rebuild.",
            id.name, module
        ),
    ];
    if !outcome.legacy_arm_found {
        next_steps.insert(
            0,
            format!(
                "NOTE: no `\"{}\"` match arm found in modules/{}.rs — the file is a bare \
                 stub. Check the name spelling / owning module.",
                id.name, module
            ),
        );
    }

    Ok(CommandMigrateResult {
        command: id.name,
        file_path: outcome.file_path,
        mod_files_touched: outcome.mod_files,
        legacy_arm_found: outcome.legacy_arm_found,
        next_steps,
    })
}

struct MigrateFsOutcome {
    file_path: String,
    mod_files: Vec<String>,
    legacy_arm_found: bool,
}

/// Build the `run` body: a fail-loud stub carrying the captured legacy arm inline as
/// a comment block so the author can port it in place.
fn build_stub_body(name: &str, captured: Option<&str>) -> String {
    let mut b = String::new();
    b.push_str(
        "        // TODO(migrate): port the captured legacy body below.\n        \
         // Rewrite `self.<field>` as `this.state.<field>`, inline any private handler,\n        \
         // then delete this comment and the stub Err.\n",
    );
    match captured {
        Some(arm) => {
            b.push_str("        // --- legacy handle_command arm ---\n");
            for line in arm.lines() {
                b.push_str("        // ");
                b.push_str(line);
                b.push('\n');
            }
            b.push_str("        // --- end legacy arm ---\n");
        }
        None => {
            b.push_str("        // (no legacy arm found — author the body fresh)\n");
        }
    }
    b.push_str(&format!(
        "        Err(CommandError::Internal(\n            \"{name}: migration stub — port the legacy body\".into(),\n        ))"
    ));
    b
}

/// Best-effort capture of a `handle_command` match arm's body text for `command`.
///
/// Finds the `"<command>"` literal, scans to the `=>`, then captures either a
/// brace-balanced block or the comma-terminated expression that follows (the two
/// arm shapes in the codebase). Returns `None` if the literal or `=>` isn't found —
/// the caller emits a bare stub and flags it. This is a heuristic for authoring
/// assistance, not a parser; the emitted code is a commented reference, never live.
fn extract_match_arm(src: &str, command: &str) -> Option<String> {
    let needle = format!("\"{command}\"");
    let lit = src.find(&needle)?;
    let arrow_rel = src[lit..].find("=>")?;
    let mut i = lit + arrow_rel + 2;
    let bytes = src.as_bytes();
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let start = i;
    if bytes[i] as char == '{' {
        // Brace-balanced block.
        let mut depth = 0usize;
        while i < bytes.len() {
            match bytes[i] as char {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(src[start..=i].trim().to_string());
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    } else {
        // Expression up to the arm-terminating top-level comma.
        let mut paren = 0i32;
        let mut brace = 0i32;
        let mut bracket = 0i32;
        while i < bytes.len() {
            match bytes[i] as char {
                '(' => paren += 1,
                ')' => paren -= 1,
                '{' => brace += 1,
                '}' => brace -= 1,
                '[' => bracket += 1,
                ']' => bracket -= 1,
                ',' if paren == 0 && brace == 0 && bracket == 0 => {
                    return Some(src[start..i].trim().to_string());
                }
                _ => {}
            }
            i += 1;
        }
        Some(src[start..].trim().to_string())
    }
}

/// `data` → `Data`, `runtime_control` → `RuntimeControl`.
fn pascal(s: &str) -> String {
    s.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the comma-terminated arm shape (`"x" => self.foo(p).await,`)
    // is captured up to the top-level comma, not a comma nested in the call args.
    #[test]
    fn captures_expression_arm() {
        let src = r#"
            match command {
                "data/create" => self.handle_create(params, vec![1, 2]).await,
                "data/read" => self.handle_read(params).await,
                _ => Err("unknown".into()),
            }
        "#;
        let arm = extract_match_arm(src, "data/create").expect("found");
        assert_eq!(arm, "self.handle_create(params, vec![1, 2]).await");
    }

    // what this catches: the block arm shape (`"x" => { ... }`) is captured with
    // balanced braces, including nested blocks.
    #[test]
    fn captures_block_arm() {
        let src = r#"
            match command {
                "data/query" | "data/list" => {
                    let q = build(params);
                    if q.ok { run(q) } else { Err("bad".into()) }
                }
                _ => unreachable!(),
            }
        "#;
        let arm = extract_match_arm(src, "data/list").expect("found");
        assert!(arm.starts_with('{') && arm.ends_with('}'));
        assert!(arm.contains("build(params)"));
        assert!(arm.contains("if q.ok"));
    }

    // what this catches: a missing command yields None so the caller emits a flagged
    // bare stub rather than silently pretending it ported something.
    #[test]
    fn missing_command_is_none() {
        let src = r#"match command { "data/read" => x, _ => y }"#;
        assert!(extract_match_arm(src, "data/nope").is_none());
    }

    // what this catches: the stub body carries the captured legacy arm as a comment
    // block and always ends in a fail-loud Err — nothing fake-compiles as done.
    #[test]
    fn stub_body_carries_legacy_and_fails_loud() {
        let body = build_stub_body("data/list", Some("self.handle_list(p).await"));
        assert!(body.contains("// self.handle_list(p).await"));
        assert!(body.contains("migration stub — port the legacy body"));
        assert!(body.contains("TODO(migrate)"));
    }
}
