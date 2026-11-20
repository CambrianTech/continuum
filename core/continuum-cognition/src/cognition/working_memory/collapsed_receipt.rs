//! The line a collapsed result leaves in her view: what it was, and the one call that reopens
//! it. One owner renders it, and the same owner is the only reader of it (diagnostic replay
//! omits these lines and nothing else): reading accepts exactly what rendering emits, because a
//! read is checked by rendering the fields back. Before this the writer emitted
//! `"… — collapsed]"` while the reader accepted only `"… — collapsed; query tool/output to
//! expand]"`, so no pointer the index emitted was ever recognised.

use uuid::Uuid;

use super::{action_log, workspace_name, ResultAttribution};
use crate::cognition::tool_executor::tool_output::ToolOutputParams;

/// A collapsed, saved result: its provenance and the saved output its reopen call names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollapsedReceipt {
    seq: u64,
    room: Option<Uuid>,
    operation: String,
    handle: String,
    workspace: Option<String>,
}

impl CollapsedReceipt {
    /// The receipt a record collapses to; `None` when its save failed, so nothing could reopen
    /// it and it stays whole.
    pub(crate) fn of(record: &action_log::ActionRecord) -> Option<Self> {
        Some(Self {
            seq: record.seq,
            room: record.room,
            operation: record.operation.clone(),
            handle: record.handle.clone()?,
            workspace: workspace_name(record.scope.as_deref()),
        })
    }

    /// A line this owner rendered, read back: its fields, accepted only when they render to
    /// exactly `text`. Anything else (a body, a protocol part, an edited line) is `None`.
    pub(crate) fn read(text: &str) -> Option<Self> {
        let body = text.strip_prefix("[result #")?.strip_suffix(" — collapsed]")?;
        let (seq, rest) = body.split_once("; room ")?;
        let (room, rest) = rest.split_once("; operation ")?;
        let (operation, rest) = rest.split_once("; saved: ")?;
        let (call, workspace) = match rest.split_once("; workspace ") {
            Some((call, workspace)) => (call, Some(workspace.to_owned())),
            None => (rest, None),
        };
        let args = call.split_once('(')?.1.strip_suffix(')')?;
        let params: ToolOutputParams = serde_json::from_str(args).ok()?; // boundary: the reopen call this owner rendered, read back
        let receipt = Self {
            seq: seq.parse().ok()?,
            room: match room {
                "unrecorded" => None,
                room => Some(Uuid::parse_str(room).ok()?),
            },
            operation: operation.to_owned(),
            handle: params.handle?.into(),
            workspace,
        };
        (receipt.to_string() == text).then_some(receipt)
    }
}

impl std::fmt::Display for CollapsedReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let attribution = ResultAttribution {
            seq: self.seq,
            room: self.room,
            operation: Some(&self.operation),
            spill_handle: Some(&self.handle),
        };
        write!(f, "[result {attribution}")?;
        if let Some(workspace) = &self.workspace {
            write!(f, "; workspace {workspace}")?;
        }
        f.write_str(" — collapsed]")
    }
}
