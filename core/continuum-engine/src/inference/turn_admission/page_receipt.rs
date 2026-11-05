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
    let (count, file_matches) = match action {
        "save" => ("n_saved", body["filename"].as_str() == Some(filename)),
        "restore" => ("n_restored", body["filename"].as_str() == Some(filename)),
        "erase" => ("n_erased", true), // the fork's erase receipt names a slot, not a file
        _ => return None,
    };
    if (200..300).contains(&status)
        && body["id_slot"].as_u64() == Some(u64::from(slot))
        && file_matches
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn erase_requires_its_own_slot_and_unsigned_count() {
        let valid = json!({"id_slot": 3, "n_erased": 0});
        assert_eq!(terminal_receipt("erase", 3, "unused", 200, Some(&valid)), Some(Terminal::Completed));
        for invalid in [json!({"id_slot": 2, "n_erased": 10}), json!({"id_slot": 3}),
            json!({"id_slot": 3, "n_erased": -1}), json!({"id_slot": 3, "n_erased": "10"}),
            json!({"id_slot": 3, "filename": "page", "n_saved": 10})] {
            assert_eq!(terminal_receipt("erase", 3, "page", 200, Some(&invalid)), None);
        }
        assert_eq!(terminal_receipt("erase", 3, "unused", 200, None), None);
        assert_eq!(terminal_receipt("erase", 3, "unused", 500, Some(&valid)), None);
    }

    #[test]
    fn erase_support_does_not_weaken_save_or_restore_receipts() {
        let erased = json!({"id_slot": 3, "n_erased": 10});
        for action in ["save", "restore", "unknown"] {
            assert_eq!(terminal_receipt(action, 3, "page", 200, Some(&erased)), None);
        }
        let saved = json!({"id_slot": 3, "filename": "other", "n_saved": 10});
        assert_eq!(terminal_receipt("save", 3, "page", 200, Some(&saved)), None);
    }
}
