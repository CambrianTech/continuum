//! Her call's arguments are checked BEFORE the tool runs (ONE-ACT 6b, item 5): the engine's typed
//! marker for arguments that never parsed, then the command's own `Params` through its descriptor
//! (`ArgumentsContract`), the same decoder its dispatch uses. A refusal is the typed
//! `ArgumentsRefusal`, matched here by variant; nothing reads an `[arguments]` prefix back out of
//! a rendered message.
//!
//! The failure this ends: a call whose JSON did not parse ran as a params object holding only the
//! engine's marker, and was refused as "missing field `file_path`", a story about a field she had
//! written. The parse error and her raw text were dropped on the way (Benchy, the M5, 2026-11-05).

use serde_json::Value;

use crate::runtime::arguments::UnparsedArguments;
use crate::sdk_codegen::{command_descriptor, AccessLevel, ArgumentsRefusal};

/// Why `canonical` must not run with `input`, or `None` when it may.
pub(crate) fn refusal_before_dispatch(canonical: &str, input: &Value) -> Option<ArgumentsRefusal> {
    if let Some(unparsed) = UnparsedArguments::of(input) {
        return Some(ArgumentsRefusal::Unparsed(unparsed));
    }
    command_descriptor(canonical)?.arguments.check(input).err()
}

/// The exact how-to-call manual for a command SHE can run, rendered inline so the fix rides back
/// in the same observation (the same renderer `commands/help` uses).
pub(crate) fn manual_for(name: &str) -> Option<String> {
    command_descriptor(name)
        .filter(|d| d.access_level == AccessLevel::AiSafe)
        .map(|d| crate::sdk_codegen::ai_help::render_ai_help(d.name, d.description, &d.params_schema))
}

/// What she reads for a refused call: what was wrong, then her next move. A retry carries the
/// command's manual; a cut never offers one (the same call cuts at the same limit).
pub(crate) fn render(canonical: &str, refusal: &ArgumentsRefusal) -> String {
    match (refusal.retries(), manual_for(canonical)) {
        (false, _) => refusal.to_string(),
        (true, Some(manual)) => format!("{refusal}\n\nHere is the correct call — fix your arguments and retry:\n{manual}"),
        (true, None) => format!(
            "{refusal}\n→ Call `commands/help` with name \"{canonical}\" to see the exact argument names, \
             types, and a fill-in-the-blanks example, then retry."
        ),
    }
}
