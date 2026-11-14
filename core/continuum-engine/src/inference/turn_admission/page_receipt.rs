//! The one judge of a page switch's reply. Both the bounded wait and an unresolved switch's late
//! reply are judged here (Codex's follow-up on #5772: the late path had its own reading, which
//! took a socket closed under the request for a receipt).

/// A KV page switch the fork performs on one slot (`/slots/{id}?action=`): the verb is a type,
/// its wire spelling derived, never a string a caller types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageAction {
    Save,
    Restore,
    Erase,
}

impl PageAction {
    /// The receipt field the fork counts this switch in, and whether its receipt names the file
    /// (the fork's erase receipt names a slot, not a file).
    fn receipt_count(self) -> (&'static str, bool) {
        match self {
            Self::Save => ("n_saved", true),
            Self::Restore => ("n_restored", true),
            Self::Erase => ("n_erased", false),
        }
    }

    /// The fork's own timing and byte fields for this switch's work (`timings.save_ms`,
    /// `n_written` for a save; the read side for the rest).
    fn engine_fields(self) -> (&'static str, &'static str) {
        match self {
            Self::Save => ("save_ms", "n_written"),
            Self::Restore | Self::Erase => ("restore_ms", "n_read"),
        }
    }
}

/// The engine's own account of a switch's work, each part absent when the engine gave none (an
/// older engine, or no terminal body). `ms` is its clock (it starts after a busy slot's deferral,
/// so the caller's elapsed minus `ms` is the wait); since fork #118 it splits into `copy_ms`, its
/// task loop's share (the state copied out or in: the only part other slots' decode waits for),
/// and `io_ms`, its IO worker's (the page files written or read, beside the decode); `admit_ms` is
/// its wait for the engine's page IO byte budget and `in_flight_bytes` what every switch held then.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct EngineWork {
    pub(crate) ms: Option<u64>,
    pub(crate) bytes: Option<u64>,
    pub(crate) copy_ms: Option<f64>,
    pub(crate) io_ms: Option<f64>,
    pub(crate) admit_ms: Option<f64>,
    pub(crate) in_flight_bytes: Option<u64>,
}

impl EngineWork {
    pub(crate) fn of(action: PageAction, body: Option<&serde_json::Value>) -> Self {
        let Some(body) = body else { return Self::default() };
        let (timing, bytes) = action.engine_fields();
        let timings = &body["timings"];
        Self {
            ms: timings[timing].as_f64().map(|ms| ms as u64),
            bytes: body[bytes].as_u64(),
            copy_ms: timings["copy_ms"].as_f64(),
            io_ms: timings["io_ms"].as_f64(),
            admit_ms: timings["admit_ms"].as_f64(),
            in_flight_bytes: body["page_io"]["io_bytes_in_flight"].as_u64(),
        }
    }

    /// A part as a probe shows it: its value, or "none" when the engine gave none.
    pub(crate) fn shown<T: std::fmt::Display>(part: Option<T>) -> String {
        part.map_or_else(|| "none".to_string(), |v| v.to_string())
    }
}

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
    action: PageAction,
    slot: u32,
    filename: &str,
    status: u16,
    body: Option<&serde_json::Value>,
) -> Option<Terminal> {
    let body = body?;
    let (count, names_file) = action.receipt_count();
    let file_matches = !names_file || body["filename"].as_str() == Some(filename);
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
        assert_eq!(terminal_receipt(PageAction::Erase, 3, "unused", 200, Some(&valid)), Some(Terminal::Completed));
        for invalid in [json!({"id_slot": 2, "n_erased": 10}), json!({"id_slot": 3}),
            json!({"id_slot": 3, "n_erased": -1}), json!({"id_slot": 3, "n_erased": "10"}),
            json!({"id_slot": 3, "filename": "page", "n_saved": 10})] {
            assert_eq!(terminal_receipt(PageAction::Erase, 3, "page", 200, Some(&invalid)), None);
        }
        assert_eq!(terminal_receipt(PageAction::Erase, 3, "unused", 200, None), None);
        assert_eq!(terminal_receipt(PageAction::Erase, 3, "unused", 500, Some(&valid)), None);
    }

    // what this catches: the engine's split of a switch (its loop's copy, its worker's IO, fork
    // #118) unread, so a live read cannot tell whether a slow switch stalled the other minds' decode
    // or only its own disk; an older engine's receipt reads as "none", never as zero.
    #[test]
    fn engine_work_reads_the_loop_and_io_shares_and_none_from_an_older_engine() {
        let streamed = json!({"id_slot": 1, "filename": "p", "n_restored": 84, "n_read": 4096,
            "timings": {"restore_ms": 1026.4, "copy_ms": 35.2, "io_ms": 991.2, "admit_ms": 0.5},
            "page_io": {"io_bytes": 9000, "io_bytes_in_flight": 9000, "io_budget_bytes": 0}});
        let work = EngineWork::of(PageAction::Restore, Some(&streamed));
        assert_eq!(work, EngineWork { ms: Some(1026), bytes: Some(4096), copy_ms: Some(35.2), io_ms: Some(991.2),
            admit_ms: Some(0.5), in_flight_bytes: Some(9000) });
        let older = json!({"id_slot": 1, "filename": "p", "n_saved": 84, "n_written": 4096, "timings": {"save_ms": 2253.0}});
        let work = EngineWork::of(PageAction::Save, Some(&older));
        assert_eq!((work.ms, work.copy_ms, work.io_ms, work.in_flight_bytes), (Some(2253), None, None, None));
        assert_eq!(EngineWork::shown(work.copy_ms), "none");
        assert_eq!(EngineWork::of(PageAction::Save, None), EngineWork::default());
    }

    #[test]
    fn erase_support_does_not_weaken_save_or_restore_receipts() {
        let erased = json!({"id_slot": 3, "n_erased": 10});
        for action in [PageAction::Save, PageAction::Restore] {
            assert_eq!(terminal_receipt(action, 3, "page", 200, Some(&erased)), None);
        }
        let saved = json!({"id_slot": 3, "filename": "other", "n_saved": 10});
        assert_eq!(terminal_receipt(PageAction::Save, 3, "page", 200, Some(&saved)), None);
    }
}
