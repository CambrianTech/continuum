//! Her call's arguments are checked BEFORE the tool runs (ONE-ACT 6b, item 5): a call the
//! classifier held (`CallBatch`) never runs and says why, and a committed one is checked against
//! the command's own `Params` through its descriptor (`ArgumentsContract`), the same decoder its
//! dispatch uses. A refusal is the typed
//! `ArgumentsRefusal`, matched here by variant; nothing reads an `[arguments]` prefix back out of
//! a rendered message.
//!
//! The failure this ends: a call whose JSON did not parse ran as a params object holding only the
//! engine's marker, and was refused as "missing field `file_path`", a story about a field she had
//! written. The parse error and her raw text were dropped on the way (Benchy, the M5, 2026-11-05).

use std::borrow::Cow;

use serde_json::Value;

use crate::ai::types::{CallEnding, ToolCall, UnparsedArguments};
use crate::sdk_codegen::{command_descriptor, AccessLevel, ArgumentsRefusal};
use continuum_kernel::runtime::arguments::accepted_shape::wrapped_call::{self, wrapped_call, WrappedCall};

/// A committed call as it runs: the command (`Some` when her arguments wrapped a call to another
/// command, which is the call she meant) and the arguments it runs with.
pub(crate) struct AcceptedCall<'a> {
    pub(crate) redirect: Option<&'static str>,
    pub(crate) input: Cow<'a, Value>,
}

/// The call a committed call to `canonical` runs as, or why it must not run: decoded as the
/// command's own params through the same decoder its dispatch uses, and when they do not decode
/// as sent, read once against the command's declared schema (refusal is the last resort, card
/// 9ed04d88), or, when they hold a whole tool call, that inner call. A command with no descriptor
/// is checked by its own dispatch.
pub(crate) fn accept_before_dispatch<'a>(canonical: &str, input: &'a Value) -> Result<AcceptedCall<'a>, ArgumentsRefusal> {
    let Some(descriptor) = command_descriptor(canonical) else {
        return Ok(AcceptedCall { redirect: None, input: Cow::Borrowed(input) });
    };
    let refusal = match descriptor.accept(input) {
        Ok(input) => return Ok(AcceptedCall { redirect: None, input }),
        Err(refusal) => refusal,
    };
    // A name she wrote resolves the one way every name she writes does (`tool_dialect`).
    let resolve = |name: &str| command_descriptor(&crate::ai::tool_dialect::resolve_wire_name(name));
    match wrapped_call(canonical, input, resolve) {
        None => Err(refusal),
        Some(WrappedCall::Same { property, arguments }) => {
            let input = descriptor.accept(arguments)?;
            wrapped_call::record(canonical, descriptor.name, property);
            Ok(AcceptedCall { redirect: None, input })
        }
        Some(WrappedCall::Other { property, command, arguments }) => {
            let input = command.accept(arguments)?;
            wrapped_call::record(canonical, command.name, property);
            Ok(AcceptedCall { redirect: Some(command.name), input })
        }
        Some(WrappedCall::Unknown(unknown)) => Err(unknown.refusal()),
    }
}

/// Why a held call never ran: the engine's marker when it wrote one, else the classifier's
/// ending with her raw call kept whole.
pub(crate) fn refusal_of_held(call: &ToolCall, ending: CallEnding) -> ArgumentsRefusal {
    match UnparsedArguments::of(&call.input) {
        Ok(Some(unparsed)) => ArgumentsRefusal::Unparsed(unparsed),
        Err(unreadable) => ArgumentsRefusal::UnreadableMarker(unreadable),
        Ok(None) => {
            let raw = call.input.to_string();
            ArgumentsRefusal::Unparsed(match ending {
                CallEnding::Failed => UnparsedArguments::Errored { raw },
                CallEnding::Unreported => UnparsedArguments::Unexplained { raw },
                // A complete call at the output limit, or (never held without a marker) any other.
                CallEnding::Cut | CallEnding::Malformed | CallEnding::Committed => UnparsedArguments::Cut { raw },
            })
        }
    }
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
