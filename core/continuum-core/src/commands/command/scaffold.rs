//! The scaffold RENDERER — pure functions that turn a [`CommandIdent`] plus a few
//! options into the text of a `commands/<cat>/<verb>.rs` file built on the
//! `action_command!` macro. No filesystem here: this is the part both `command/new`
//! and `command/migrate` share, so what they emit can't drift, and it's unit-tested
//! by asserting on the rendered string.

use super::ident::CommandIdent;
use crate::sdk_codegen::CommandError;

/// Which command shape to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Form {
    /// No deps: a unit struct, auto-registered via `register_stateless_command!`
    /// (inside the macro). Fully wired by the generator — compiles and routes.
    Stateless,
    /// Holds `state: Arc<StateType>` from an owning module. The macro registers the
    /// descriptor; the module's `commands()` constructs the runtime object — so the
    /// generator prints the two wiring lines it can't safely auto-edit.
    DepHolding {
        /// The state type captured (`"DataState"`).
        state_type: String,
        /// The module that owns it (`"data"`) — used for the `use` import and the
        /// printed `commands()` instruction.
        module: String,
    },
}

/// The access level the generated `action_command!` block declares. Maps to an
/// `AccessLevel::*` ident the macro consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    AiSafe,
    Privileged,
    Internal,
}

impl Access {
    /// Parse a CLI-friendly spelling (`"ai-safe"`, `"privileged"`, `"internal"`).
    pub fn parse(s: &str) -> Result<Self, CommandError> {
        match s
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_'], "")
            .as_str()
        {
            "aisafe" => Ok(Access::AiSafe),
            "privileged" => Ok(Access::Privileged),
            "internal" => Ok(Access::Internal),
            other => Err(CommandError::Invalid(format!(
                "unknown access level '{other}' (want ai-safe | privileged | internal)"
            ))),
        }
    }

    /// The `AccessLevel::*` variant ident the macro expects after `access:`.
    fn ident(self) -> &'static str {
        match self {
            Access::AiSafe => "AiSafe",
            Access::Privileged => "Privileged",
            Access::Internal => "Internal",
        }
    }
}

/// Everything the renderer needs beyond the [`CommandIdent`].
#[derive(Debug, Clone)]
pub struct ScaffoldOpts {
    pub form: Form,
    pub access: Access,
    /// The `///` doc line ⟹ the model-facing `DESCRIPTION`. Trimmed; a sensible
    /// default is supplied when empty.
    pub description: String,
    /// The `run` body to emit. `None` → an unimplemented stub that fails loud.
    /// `command/migrate` supplies the transplanted legacy block here.
    pub body: Option<String>,
}

/// Render the full source of `commands/<cat>/<verb>.rs`.
pub fn render_command_file(id: &CommandIdent, opts: &ScaffoldOpts) -> String {
    let mut out = String::new();
    let desc = if opts.description.trim().is_empty() {
        format!("TODO: one-line, model-facing description of `{}`.", id.name)
    } else {
        opts.description.trim().to_string()
    };

    // ── module doc + imports ──────────────────────────────────────────────
    out.push_str(&format!("//! `{}` — {}\n\n", id.name, desc));

    if matches!(opts.form, Form::DepHolding { .. }) {
        out.push_str("use std::sync::Arc;\n\n");
    }
    out.push_str("use schemars::JsonSchema;\n");
    out.push_str("use serde::{Deserialize, Serialize};\n");
    out.push_str("use ts_rs::TS;\n\n");
    if let Form::DepHolding { state_type, module } = &opts.form {
        out.push_str(&format!("use crate::modules::{module}::{state_type};\n"));
    }
    out.push_str("use crate::sdk_codegen::{CommandError, Ctx};\n\n");

    // ── Params ────────────────────────────────────────────────────────────
    out.push_str(&format!("/// Params for `{}`.\n", id.name));
    out.push_str("#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]\n");
    out.push_str("#[serde(rename_all = \"camelCase\")]\n");
    out.push_str(&format!(
        "#[ts(export, export_to = \"../../../protocol/typescript/{}/{}.ts\")]\n",
        id.ts_subdir, id.params_type
    ));
    out.push_str(&format!("pub struct {} {{}}\n\n", id.params_type));

    // ── Result ────────────────────────────────────────────────────────────
    out.push_str(&format!("/// Result of `{}`.\n", id.name));
    out.push_str("#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]\n");
    out.push_str("#[serde(rename_all = \"camelCase\")]\n");
    out.push_str(&format!(
        "#[ts(export, export_to = \"../../../protocol/typescript/{}/{}.ts\")]\n",
        id.ts_subdir, id.result_type
    ));
    out.push_str(&format!("pub struct {} {{\n", id.result_type));
    out.push_str("    /// TODO: real result fields.\n");
    out.push_str("    pub ok: bool,\n");
    out.push_str("}\n\n");

    // ── the command block ─────────────────────────────────────────────────
    let body = opts.body.clone().unwrap_or_else(|| {
        format!(
            "        // TODO: implement.\n        Err(CommandError::Internal(\n            \
             \"{}: not implemented — fill the run body\".into(),\n        ))",
            id.name
        )
    });
    out.push_str("crate::action_command! {\n");
    out.push_str(&format!("    /// {desc}\n"));
    match &opts.form {
        Form::Stateless => {
            out.push_str(&format!("    pub struct {};\n", id.struct_name));
        }
        Form::DepHolding { state_type, .. } => {
            out.push_str(&format!(
                "    pub struct {} {{ state: Arc<{state_type}> }}\n",
                id.struct_name
            ));
        }
    }
    out.push_str(&format!("    name: \"{}\",\n", id.name));
    out.push_str(&format!("    access: {},\n", opts.access.ident()));
    out.push_str(&format!("    params: {},\n", id.params_type));
    out.push_str(&format!("    output: {},\n", id.result_type));
    // The receiver/ctx bindings: stateless ignores both; dep-holding names them so
    // the (stubbed or transplanted) body can reach `this.state` / `ctx`.
    let (this, ctx) = match opts.form {
        Form::Stateless => ("_this", "_ctx"),
        Form::DepHolding { .. } => ("this", "ctx"),
    };
    out.push_str(&format!("    run({this}, {ctx}, _p) => {{\n"));
    if let Form::DepHolding { .. } = opts.form {
        if opts.body.is_none() {
            // Touch the bindings so the unimplemented stub has no unused warnings.
            out.push_str("        let _ = (&this.state, ctx);\n");
        }
    }
    out.push_str(&body);
    out.push('\n');
    out.push_str("    }\n");
    out.push_str("}\n\n");

    // ── test ──────────────────────────────────────────────────────────────
    out.push_str("#[cfg(test)]\nmod tests {\n");
    out.push_str("    use super::*;\n");
    out.push_str("    use crate::sdk_codegen::ActionCommand;\n\n");
    out.push_str(&format!(
        "    // what this catches: `{}` carries its wire name so every caller (cu, the\n    \
         // persona tool surface, the grid) binds the right routing key; a drift from the\n    \
         // file path silently breaks dispatch.\n",
        id.name
    ));
    out.push_str("    #[test]\n");
    out.push_str("    fn name_mirrors_path() {\n");
    out.push_str(&format!(
        "        assert_eq!({}::NAME, \"{}\");\n",
        id.struct_name, id.name
    ));
    out.push_str("    }\n");
    out.push_str("}\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts_stateless() -> ScaffoldOpts {
        ScaffoldOpts {
            form: Form::Stateless,
            access: Access::AiSafe,
            description: "Echo a value back.".into(),
            body: None,
        }
    }

    // what this catches: a stateless scaffold renders a complete, macro-based command
    // file — the right wire name, the AiSafe access ident the macro consumes, the
    // derived param/result types with their ts-rs export path, and the unimplemented
    // fail-loud stub. If any token drifts the file won't compile in the tree.
    #[test]
    fn renders_a_complete_stateless_command() {
        let id = CommandIdent::parse("demo/echo").expect("valid");
        let src = render_command_file(&id, &opts_stateless());
        assert!(src.contains("crate::action_command! {"));
        assert!(src.contains("pub struct DemoEcho;"));
        assert!(src.contains("name: \"demo/echo\","));
        assert!(src.contains("access: AiSafe,"));
        assert!(src.contains("params: DemoEchoParams,"));
        assert!(src.contains("output: DemoEchoResult,"));
        assert!(src.contains("export_to = \"../../../protocol/typescript/demo/DemoEchoParams.ts\""));
        assert!(src.contains("not implemented"), "stub fails loud");
        assert!(src.contains("assert_eq!(DemoEcho::NAME, \"demo/echo\");"));
    }

    // what this catches: a dep-holding scaffold captures `state: Arc<State>`, imports
    // it from the owning module, and names the receiver `this` so a transplanted body
    // can reach `this.state` — the shape `command/migrate` produces for the bulk port.
    #[test]
    fn renders_a_dep_holding_command_with_state() {
        let id = CommandIdent::parse("data/list").expect("valid");
        let opts = ScaffoldOpts {
            form: Form::DepHolding {
                state_type: "DataState".into(),
                module: "data".into(),
            },
            access: Access::Privileged,
            description: "List entities in a collection.".into(),
            body: None,
        };
        let src = render_command_file(&id, &opts);
        assert!(src.contains("use std::sync::Arc;"));
        assert!(src.contains("use crate::modules::data::DataState;"));
        assert!(src.contains("pub struct DataList { state: Arc<DataState> }"));
        assert!(src.contains("access: Privileged,"));
        assert!(src.contains("run(this, ctx, _p) => {"));
        assert!(
            src.contains("let _ = (&this.state, ctx);"),
            "stub touches bindings"
        );
    }

    // what this catches: a transplanted body (the migrate path) is emitted verbatim
    // and the binding-touch line is suppressed, so the legacy code lands intact.
    #[test]
    fn transplants_a_provided_body() {
        let id = CommandIdent::parse("data/list").expect("valid");
        let opts = ScaffoldOpts {
            form: Form::DepHolding {
                state_type: "DataState".into(),
                module: "data".into(),
            },
            access: Access::Privileged,
            description: "List entities.".into(),
            body: Some("        Ok(DataListResult { ok: true })".into()),
        };
        let src = render_command_file(&id, &opts);
        assert!(src.contains("Ok(DataListResult { ok: true })"));
        assert!(
            !src.contains("let _ = (&this.state, ctx);"),
            "no stub touch when body given"
        );
    }

    // what this catches: access parsing accepts the CLI spellings and rejects junk.
    #[test]
    fn access_parses_cli_spellings() {
        assert_eq!(Access::parse("ai-safe").unwrap(), Access::AiSafe);
        assert_eq!(Access::parse("AiSafe").unwrap(), Access::AiSafe);
        assert_eq!(Access::parse("privileged").unwrap(), Access::Privileged);
        assert_eq!(Access::parse("internal").unwrap(), Access::Internal);
        assert!(Access::parse("root").is_err());
    }
}
