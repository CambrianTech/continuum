//! A WHOLE tool call written inside one of a call's arguments (the M5, 2026-11-07, five times in
//! 15 min): `bash(cmd: {"tool_call": {"name": "code/list", "arguments": {"path": "."}}})`. The
//! shape is our own manual's (`ai_help` renders every call as `{"tool_call": …}`), copied into the
//! field that showed `{}`. Its meaning is plain: the inner call is her intent.
//!
//! - It names THIS command (canonical, alias or wire name): its `arguments` are the call's.
//! - It names ANOTHER command in her hands: that inner call is the one to run.
//! - It names nothing: refused ([`ArgumentsRefusal::WrappedCall`]), with the inner call as her
//!   next call.
//!
//! Recognised only as the call's ONE argument, holding exactly `{"tool_call": {"name": <string>,
//! "arguments": <object>}}` (or the call's one key being `tool_call` itself). Beside other
//! arguments, which of the two she meant is a guess, and the decoder's refusal stands.
//! Names resolve through the caller's resolver (the engine's `tool_dialect`, the one owner of
//! names), never a second one here.

use std::sync::LazyLock;

use dashmap::DashMap;
use serde_json::Value;

use crate::runtime::arguments::ArgumentsRefusal;
use crate::sdk_codegen::CommandDescriptor;

/// What a wrapped call names, resolved.
#[derive(Debug, Clone, Copy)]
pub enum WrappedCall<'a> {
    /// It names the command being called: `arguments` are that call's own.
    Same { property: &'a str, arguments: &'a Value },
    /// It names another command: that command, with `arguments`, is the call she meant.
    Other { property: &'a str, command: &'static CommandDescriptor, arguments: &'a Value },
    /// It names nothing in her hands.
    Unknown(UnknownCall<'a>),
}

/// A wrapped call whose name is no command in her hands.
#[derive(Debug, Clone, Copy)]
pub struct UnknownCall<'a> {
    pub property: &'a str,
    pub name: &'a str,
    pub arguments: &'a Value,
}

impl UnknownCall<'_> {
    /// Its refusal: the inner call, as her next call.
    pub fn refusal(&self) -> ArgumentsRefusal {
        ArgumentsRefusal::WrappedCall {
            property: self.property.to_string(),
            name: self.name.to_string(),
            arguments: self.arguments.to_string(), // boundary: her own call, rendered back to her as she wrote it
        }
    }
}

/// The wrapped call in `arguments`, if that is all they hold. `outer` is the command being called;
/// `resolve` maps a name she wrote to the command it names in her hands. Pure.
pub fn wrapped_call<'a>(
    outer: &str,
    arguments: &'a Value,
    resolve: impl Fn(&str) -> Option<&'static CommandDescriptor>,
) -> Option<WrappedCall<'a>> {
    let (property, value) = match arguments.as_object() {
        Some(map) if map.len() == 1 => map.iter().next()?,
        _ => return None,
    };
    let call = if property == "tool_call" { value } else { only_key(value, "tool_call")? };
    let call = call.as_object().filter(|c| c.len() == 2)?;
    let name = call.get("name")?.as_str()?;
    let inner = call.get("arguments").filter(|a| a.is_object())?;
    Some(match resolve(name) {
        Some(command) if command.name == outer => WrappedCall::Same { property, arguments: inner },
        Some(command) => WrappedCall::Other { property, command, arguments: inner },
        None => WrappedCall::Unknown(UnknownCall { property, name, arguments: inner }),
    })
}

fn only_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_object().filter(|m| m.len() == 1)?.get(key)
}

/// Wrapped calls accepted since boot, per (outer command, inner command, property).
static ACCEPTED: LazyLock<DashMap<(String, &'static str, String), u64>> = LazyLock::new(DashMap::new);

/// Record an accepted wrapped call (it decoded as the inner command's params): the outer and inner
/// commands, so the manual that taught the shape is fixed at its source.
pub fn record(outer: &str, inner: &'static str, property: &str) {
    let count = {
        let mut entry = ACCEPTED.entry((outer.to_string(), inner, property.to_string())).or_default();
        *entry += 1;
        *entry
    };
    continuum_base::probe!(
        class = "dispatch.call.wrapped_call",
        outer = outer,
        inner = inner,
        property = property,
        redirected = outer != inner,
        count,
        "a whole tool call arrived inside an argument: the inner call ran, never refused"
    );
}
