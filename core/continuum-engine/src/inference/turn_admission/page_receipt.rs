//! The one judge of a page switch's reply. Both the bounded wait and an unresolved switch's late
//! reply are judged here (Codex's follow-up on #5772: the late path had its own reading, which
//! took a socket closed under the request for a receipt).

/// What a terminal receipt proved the engine did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Terminal {
    /// The switch completed: the body names this slot, this file and the action's count.
    Completed,
    /// The engine refused the switch with its typed request error.
    Rejected,
}

/// A terminal receipt for `action` on `slot` into `filename`, or `None`: a transport failure
/// (status 0), a truncated or unreadable body, or one that names another slot or file proves
/// nothing about whether the engine ended the switch.
pub(crate) fn terminal_receipt(
    action: &str,
    slot: u32,
    filename: &str,
    status: u16,
    body: Option<&serde_json::Value>,
) -> Option<Terminal> {
    let body = body?;
    let count = if action == "save" { "n_saved" } else { "n_restored" };
    if (200..300).contains(&status)
        && body["id_slot"].as_u64() == Some(u64::from(slot))
        && body["filename"].as_str() == Some(filename)
        && body[count].as_u64().is_some()
    {
        return Some(Terminal::Completed);
    }
    (status == 400
        && body["error"]["code"].as_u64() == Some(400)
        && body["error"]["type"].as_str() == Some("invalid_request_error")
        && body["error"]["message"].as_str().is_some())
    .then_some(Terminal::Rejected)
}
