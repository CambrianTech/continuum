//! `command/new` — scaffold a fresh command file on the `action_command!` macro.
//!
//! A command that writes commands. Given a wire name, it emits a complete
//! `commands/<cat>/<verb>.rs` (typed Params/Result with their ts-rs export paths, the
//! `action_command!` block with a fail-loud stub body, and a name-mirrors-path test)
//! and wires the `pub mod` lines so the file is reachable from the crate root. A
//! stateless command is fully live after a rebuild — it self-registers. A dep-holding
//! command also needs its runtime object constructed by the owning module; the
//! generator can't safely auto-edit that, so it returns the exact two wiring lines.
//!
//! Authoring source means a rebuild is required to take effect — there is no
//! hot-registration. The generator's job is to remove the boilerplate, not to dodge
//! the compiler.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ident::CommandIdent;
use super::scaffold::{render_command_file, Access, Form, ScaffoldOpts};
use super::wiring::{ensure_mod_lines, resolve_src_root, write_command_file};
use crate::sdk_codegen::{CommandError, Ctx};

/// Params for `command/new`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/command/CommandNewParams.ts"
)]
pub struct CommandNewParams {
    /// The wire name to create, e.g. `data/list` or `code/git/status`. The file path
    /// and struct name are derived from it (path == namespace).
    pub name: String,
    /// Access level: `ai-safe` (default), `privileged`, or `internal`.
    #[serde(default)]
    #[ts(optional)]
    pub access: Option<String>,
    /// Force the zero-ceremony stateless form even if a state type is given.
    #[serde(default)]
    pub stateless: bool,
    /// The module state type to capture (`DataState`) — presence selects the
    /// dep-holding form. Requires `module`.
    #[serde(default)]
    #[ts(optional)]
    pub state_type: Option<String>,
    /// The module that owns the state (`data`) — used for the `use` import and the
    /// printed `commands()` wiring instruction.
    #[serde(default)]
    #[ts(optional)]
    pub module: Option<String>,
    /// The one-line, model-facing description (⟹ the command's `DESCRIPTION`).
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    /// Overwrite an existing file (default: refuse, to avoid clobbering by mistake).
    #[serde(default)]
    pub force: bool,
    /// Override the continuum-core `src/` root (default: the crate this binary was
    /// built from). For out-of-tree checkouts.
    #[serde(default)]
    #[ts(optional)]
    pub src_root: Option<String>,
}

/// Result of `command/new`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/command/CommandNewResult.ts"
)]
pub struct CommandNewResult {
    /// The wire name created.
    pub name: String,
    /// Absolute path of the file written.
    pub file_path: String,
    /// The `mod.rs` files created or edited to make the command reachable.
    pub mod_files_touched: Vec<String>,
    /// The remaining manual steps (fill the body; for dep-holding, the `commands()`
    /// wiring the generator can't safely auto-edit). Empty-body stub always lists
    /// the "fill the run body, then rebuild" step.
    pub next_steps: Vec<String>,
}

crate::action_command! {
    /// Scaffold a new command file built on the action_command! macro: emits the
    /// typed Params/Result, the command block with a fail-loud stub, and a test, and
    /// wires the pub-mod lines. Stateless commands are live after a rebuild; dep-
    /// holding commands also need their object added to the owning module's commands().
    pub struct CommandNew;
    name: "command/new",
    access: Privileged,
    params: CommandNewParams,
    output: CommandNewResult,
    run(_this, _ctx, p) => {
        scaffold_new(p).await
    }
}

/// The body, factored out so the unit test can drive it against a temp src root
/// without going through the macro's generated `run`.
pub(crate) async fn scaffold_new(p: CommandNewParams) -> Result<CommandNewResult, CommandError> {
    let id = CommandIdent::parse(&p.name)?;
    let access = match p.access.as_deref() {
        Some(s) => Access::parse(s)?,
        None => Access::AiSafe,
    };

    // Select the form. A state type selects dep-holding (module required); `stateless`
    // forces the unit-struct form regardless.
    let form = if p.stateless {
        Form::Stateless
    } else if let Some(state_type) = p.state_type.clone() {
        let module = p.module.clone().ok_or_else(|| {
            CommandError::Invalid(
                "dep-holding command needs `module` (the module that owns the state \
                 type) alongside `state_type`"
                    .into(),
            )
        })?;
        Form::DepHolding { state_type, module }
    } else {
        Form::Stateless
    };

    let opts = ScaffoldOpts {
        form: form.clone(),
        access,
        description: p.description.clone().unwrap_or_default(),
        body: None,
    };
    let content = render_command_file(&id, &opts);

    // Do the filesystem work off the async runtime.
    let src_override = p.src_root.clone();
    let id_for_fs = id.clone();
    let force = p.force;
    let (file_path, touched): (String, Vec<String>) = tokio::task::spawn_blocking(move || {
        let root = resolve_src_root(src_override.as_deref())?;
        let path = write_command_file(&root, &id_for_fs, &content, force)?;
        let mods = ensure_mod_lines(&root, &id_for_fs)?;
        Ok::<_, CommandError>((
            path.display().to_string(),
            mods.into_iter().map(|p| p.display().to_string()).collect(),
        ))
    })
    .await
    .map_err(|e| CommandError::Internal(format!("scaffold task panicked: {e}")))??;

    let mut next_steps = vec![format!(
        "Fill the `run` body in {file_path}, then rebuild (cargo build) — it registers \
         on the next build."
    )];
    if let Form::DepHolding { module, .. } = &form {
        let cat = id.segments[0].replace('-', "_");
        next_steps.push(format!(
            "Expose the runtime object: add `Arc::new({} {{ state: state.clone() }})` to \
             the Vec from `commands::{cat}::command_objects(state)` (create that fn if \
             absent — pattern: commands/code/git/mod.rs).",
            id.struct_name
        ));
        next_steps.push(format!(
            "Ensure `{}Module::commands()` includes \
             `commands::{cat}::command_objects(self.state.clone())` (pattern: \
             modules/code.rs).",
            pascal_module(module)
        ));
    }

    Ok(CommandNewResult {
        name: id.name,
        file_path,
        mod_files_touched: touched,
        next_steps,
    })
}

/// `data` → `Data`, `runtime_control` → `RuntimeControl` — the `XxxModule` prefix
/// for the printed `commands()` instruction.
fn pascal_module(module: &str) -> String {
    module
        .split(['-', '_'])
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

    // what this catches: command/new actually writes a compiling, self-registering
    // stateless command into a temp tree and wires both pub-mod lines (root + new
    // category) — the end-to-end generator contract, exercised without touching the
    // real source tree. A regression in rendering or wiring shows up as a missing
    // file, a missing mod line, or a wrong next-step.
    #[tokio::test]
    async fn scaffolds_a_stateless_command_into_a_temp_tree() {
        // Build a minimal src/ with a commands/ dir + an empty mod.rs.
        let tmp = std::env::temp_dir().join(format!("cmdnew-{}", std::process::id()));
        let commands = tmp.join("commands");
        std::fs::create_dir_all(&commands).unwrap();
        std::fs::write(commands.join("mod.rs"), "//! commands\n").unwrap();

        let res = scaffold_new(CommandNewParams {
            name: "demo/echo".into(),
            access: Some("ai-safe".into()),
            stateless: true,
            description: Some("Echo a value.".into()),
            src_root: Some(tmp.display().to_string()),
            ..Default::default()
        })
        .await
        .expect("scaffold ok");

        assert_eq!(res.name, "demo/echo");
        let file = commands.join("demo/echo.rs");
        assert!(file.exists(), "command file written");
        let body = std::fs::read_to_string(&file).unwrap();
        assert!(body.contains("name: \"demo/echo\","));
        // The stateless unit-struct form (no `{ state: ... }`) — `action_command!`
        // expands this to `register_stateless_command!`, wiring descriptor + object.
        assert!(
            body.contains("pub struct DemoEcho;"),
            "stateless unit-struct form"
        );
        assert!(body.contains("crate::action_command! {"));

        // Root mod.rs gained `pub mod demo;`, new category mod.rs has `pub mod echo;`.
        let root_mod = std::fs::read_to_string(commands.join("mod.rs")).unwrap();
        assert!(root_mod.contains("pub mod demo;"), "root wired: {root_mod}");
        let cat_mod = std::fs::read_to_string(commands.join("demo/mod.rs")).unwrap();
        assert!(
            cat_mod.contains("pub mod echo;"),
            "category wired: {cat_mod}"
        );

        assert!(res
            .next_steps
            .iter()
            .any(|s| s.contains("Fill the `run` body")));

        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: the dep-holding form refuses without a `module`, failing loud
    // at the missing precondition rather than emitting a file with a broken `use`.
    #[tokio::test]
    async fn dep_holding_without_module_fails_loud() {
        let err = scaffold_new(CommandNewParams {
            name: "data/list".into(),
            state_type: Some("DataState".into()),
            ..Default::default()
        })
        .await
        .expect_err("must require module");
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
