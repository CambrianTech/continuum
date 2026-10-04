//! Indexed tool-call assembly shared by provider transports and native parsing.
//! This owner never publishes text or executes tools. Final argument validation
//! remains with the response owner; fragments are not complete tool calls.

#[derive(Default)]
pub(crate) struct StreamToolAccum {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
}

/// IDs and names are full values (native parsers can discover a longer name);
/// arguments are incremental suffixes. Empty metadata never erases a known value.
pub(crate) struct ToolCallDelta {
    pub(crate) index: usize,
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) arguments: Option<String>,
}

pub(crate) fn accumulate_tool_call(acc: &mut Vec<StreamToolAccum>, delta: ToolCallDelta) {
    if acc.len() <= delta.index { acc.resize_with(delta.index + 1, StreamToolAccum::default); }
    let slot = &mut acc[delta.index];
    if let Some(id) = delta.id.filter(|id| !id.is_empty()) { slot.id = id; }
    if let Some(name) = delta.name.filter(|name| !name.is_empty()) { slot.name = name; }
    if let Some(arguments) = delta.arguments { slot.arguments.push_str(&arguments); }
}
