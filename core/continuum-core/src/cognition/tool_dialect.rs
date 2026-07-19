//! tool_dialect — the ADAPTER between our command namespace and the tool-call
//! dialect models were actually trained on. [[joel-boundary-design-values]]:
//! "always adapters — meet the model ergonomically, never hardcode around it."
//!
//! ## Why (Joel, 2026-07-10: "our tools are a foreign language; theirs is the
//! model's native tongue")
//!
//! Two facts about every tool-trained model (Devstral/OpenHands, Qwen-Coder,
//! Hermes, the cloud models we cannot fine-tune):
//!
//! 1. **The OpenAI function-name convention is `[a-zA-Z0-9_-]{1,64}`.** Our
//!    command names carry slashes (`code/run`, `commands/help`) — a shape no
//!    model ever saw inside a `tools` array during training, and one the spec
//!    those models were trained against does not even allow.
//! 2. **The hot verbs have conventional names** — `bash`, `read_file`,
//!    `write_file`, `edit_file`, `grep` — burned in by OpenHands/SWE-agent-style
//!    scaffolds. A model reaches for `bash` by reflex; `code/shell` it must
//!    learn from a menu (the discovery-tool trap: 14/14 acts on `commands/help`,
//!    zero edits).
//!
//! So the WIRE speaks the model's dialect and the SUBSTRATE keeps its canonical
//! names: specs are renamed on OFFER ([`to_wire_spec`]), calls are mapped back
//! on RETURN ([`from_wire_name`]) before authorization/execution — the same
//! trivial-adapter shape the legacy Node personas used. Internal representation
//! stays OpenAI-shaped (`name/arguments/input_schema`) end to end.
//!
//! Canonical names remain first-class on the wire too: [`from_wire_name`] passes
//! unknown names through untouched, so `code/run` still resolves if a model says
//! it (MORE surface, never less). One table, one place — the compression rule.

use crate::ai::types::NativeToolSpec;

/// The FEW verbs where the model's trained reflex name IS the right thing to
/// offer — the name and the tool are effectively the same thing, universally.
/// "We can rename to what they're trained but only WHEN it makes sense" (Joel).
/// `code/shell` IS `bash`; offering it any other way fights the deepest reflex
/// every code model shares. Kept deliberately tiny — the default is OUR name.
const KEEP_TRAINED: &[(&str, &str)] = &[("code/shell", "bash")];

/// Trained reflex names we ACCEPT (a model reaching for `read_file` still lands)
/// and NAME in the tool's description as a bridge — but do NOT offer as the
/// primary name. The persona is always offered OUR tool; the reflex is the
/// on-ramp, not the identity. Adding a hot verb is one row.
const REFLEX: &[(&str, &str)] = &[
    ("code/read", "read_file"),
    ("code/write", "write_file"),
    ("code/edit", "edit_file"),
    ("code/search", "grep"),
    ("code/list", "list_files"),
    ("code/tree", "file_tree"),
    ("code/run", "run_code"),
    ("code/git/diff", "git_diff"),
    ("code/git/status", "git_status"),
    ("code/git/add", "git_add"),
    ("code/git/commit", "git_commit"),
    ("code/git/apply", "git_apply"),
    ("interface/screenshot", "screenshot"),
    ("commands/list", "list_commands"),
    ("commands/help", "help"),
    ("work/claim", "claim_task"),
];

/// Charset-legalize OUR command name for the OpenAI function-name convention
/// (`[a-zA-Z0-9_-]`): slashes → underscores, so `code/read` → `code_read`. This
/// is what lets us OFFER our own namespace on the wire — the slash a model never
/// saw in a `tools` array becomes an underscore it has. Reversible because no
/// native command name carries an underscore (they use `/` and `-`).
fn wire_name(ours: &str) -> String {
    ours.replace('/', "_")
}

/// Rename a spec to the wire dialect. The persona is ALWAYS offered OUR tool:
/// its canonical name made charset-legal (`code_read`), with the model's trained
/// reflex (`read_file`) named in the DESCRIPTION as a bridge. The one exception
/// is [`KEEP_TRAINED`] (`bash`), where the trained name IS the tool. The long
/// tail (no reflex) is charset-legalized too — same namespace, end to end.
pub fn to_wire_spec(mut spec: NativeToolSpec) -> NativeToolSpec {
    if let Some((_, trained)) = KEEP_TRAINED.iter().find(|(ours, _)| *ours == spec.name) {
        spec.name = (*trained).to_string();
        return spec;
    }
    if let Some((_, reflex)) = REFLEX.iter().find(|(ours, _)| *ours == spec.name) {
        // Name the reflex in the description so she can bridge from what she was
        // trained on — but the tool she's offered stays ours.
        let base = spec.description.trim_end().trim_end_matches('.');
        spec.description = format!("{base}. (Your trained `{reflex}` also works.)");
    }
    spec.name = wire_name(&spec.name);
    spec
}

/// Map a wire tool-call name back to the canonical command. Accepts, in order:
/// a trained reflex (`read_file`/`bash` → `code/read`/`code/shell`), OUR
/// charset-legal name (`code_read` → `code/read`), and passes an already-
/// canonical or unknown name through untouched. The adapter only ever WIDENS the
/// surface — whatever she emits from her training OR our menu resolves.
pub fn from_wire_name(wire: &str) -> String {
    if let Some((ours, _)) = KEEP_TRAINED
        .iter()
        .chain(REFLEX.iter())
        .find(|(_, alias)| *alias == wire)
    {
        return (*ours).to_string();
    }
    // Our charset-legal name → restore the slashes (`code_read` → `code/read`).
    // Reflex names were already caught above, so an underscore here is ours.
    if wire.contains('_') {
        return wire.replace('_', "/");
    }
    wire.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(n: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: n.to_string(),
            description: "Read a file".to_string(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the directive — ALWAYS offer OUR tool. `code/read` is
    // offered as our charset-legal `code_read` (never the foreign `read_file`),
    // with the trained reflex NAMED in the description as a bridge. `code/shell`
    // is the one KEEP_TRAINED exception (offered `bash`). Nothing on the wire
    // carries a slash (the OpenAI function-name charset).
    #[test]
    fn always_offers_our_tool_with_the_reflex_as_a_bridge() {
        let read = to_wire_spec(spec("code/read"));
        assert_eq!(read.name, "code_read", "offered under OUR name, charset-legal");
        assert!(
            read.description.contains("read_file"),
            "the trained reflex is named as a bridge: {}",
            read.description
        );
        // The one "when it makes sense" rename: shell IS bash.
        assert_eq!(to_wire_spec(spec("code/shell")).name, "bash");
        // Long tail: our name, charset-legal, no reflex hint.
        assert_eq!(to_wire_spec(spec("cognition/eval")).name, "cognition_eval");
        // No offered name ever carries a slash.
        for ours in ["code/read", "code/shell", "cognition/eval", "work/claim", "code/git/status"] {
            assert!(
                !to_wire_spec(spec(ours)).name.contains('/'),
                "{ours} reached the wire with a slash"
            );
        }
    }

    // what this catches: the adapter only ever WIDENS — whatever she emits
    // resolves. Her trained reflex (`read_file`, `bash`), OUR offered name
    // (`code_read`), the raw canonical (`code/read`), and an unknown name all map
    // back correctly. This is what makes the rename safe: she can reach for the
    // name she was trained on OR the one we put on the menu.
    #[test]
    fn from_wire_accepts_reflex_and_our_name_and_canonical() {
        // Trained reflex → our command.
        assert_eq!(from_wire_name("read_file"), "code/read");
        assert_eq!(from_wire_name("bash"), "code/shell");
        assert_eq!(from_wire_name("claim_task"), "work/claim");
        // OUR offered (charset-legal) name → our command.
        assert_eq!(from_wire_name("code_read"), "code/read");
        assert_eq!(from_wire_name("code_git_status"), "code/git/status");
        // The raw canonical a model might still emit → itself.
        assert_eq!(from_wire_name("code/read"), "code/read");
        // Genuinely unknown → passed through untouched (the executor fails it
        // loud with a did-you-mean; the adapter never invents a route).
        assert_eq!(from_wire_name("frobnicate"), "frobnicate");
        // Full round-trip: offer OUR name, get it back.
        for ours in ["code/read", "code/list", "work/claim", "code/git/commit", "cognition/eval"] {
            let offered = to_wire_spec(spec(ours)).name;
            assert_eq!(from_wire_name(&offered), ours, "round-trip for {ours}");
        }
    }
}
