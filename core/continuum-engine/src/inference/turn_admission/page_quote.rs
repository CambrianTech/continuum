//! A page save's size quote (`save-size`): asked before any checkpoint file is written, so the
//! save is admitted against the bytes it will write. Moved whole out of `turn_admission.rs`.

use super::page_body;

/// Why a page save's size quote gave no size: each a different fault, so the refusal names which
/// (card 8fa1c147; enrico 2026-10-25/26: 11 refusals and 13 cold turns read only "quote
/// unavailable", and nothing said whether the engine refused, answered for another slot, or was
/// still busy with a prefill when the bound passed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuoteMiss {
    /// The request did not reach the engine or its reply did not arrive.
    Unsent(String),
    /// The engine answered with a non-success status.
    Status(u16),
    /// The engine answered for another slot or another file.
    Mismatch,
    /// The answer named no positive size.
    NoSize,
    /// The engine stopped making progress (or stopped answering) before it quoted.
    EngineStopped,
}

impl std::fmt::Display for QuoteMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsent(error) => write!(f, "request not delivered: {error}"),
            Self::Status(status) => write!(f, "engine answered HTTP {status}"),
            Self::Mismatch => f.write_str("engine answered for another slot or file"),
            Self::NoSize => f.write_str("engine named no positive size"),
            Self::EngineStopped => f.write_str("engine stopped progressing before it quoted"),
        }
    }
}

/// Ask the engine what a save of `slot` into `filename` would write. The quote names the same
/// checkpoint subset as the write, or the admitted bytes lie. A non-success status returns before
/// its body is read, so an engine that refuses and then stalls its error body is a `Status` miss
/// at once, never a wait that a stall watchdog could report as `EngineStopped` (Codex on #5441).
pub(crate) async fn request_quote(client: &reqwest::Client, root: &str, slot: u32, filename: &str) -> Result<u64, QuoteMiss> {
    let response = client.post(format!("{}/slots/{slot}?action=save-size", root.trim_end_matches('/')))
        .json(&page_body(filename)).send().await
        .map_err(|error| QuoteMiss::Unsent(error.to_string()))?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
        return Err(QuoteMiss::Status(status));
    }
    let body = response.json::<serde_json::Value>().await.ok();
    read_quote(status, body.as_ref(), slot, filename)
}

/// PURE: the size an engine's `save-size` reply quotes for exactly this slot and file.
pub(crate) fn read_quote(status: u16, body: Option<&serde_json::Value>, slot: u32, filename: &str) -> Result<u64, QuoteMiss> {
    if !(200..300).contains(&status) {
        return Err(QuoteMiss::Status(status));
    }
    let body = body.ok_or(QuoteMiss::NoSize)?;
    if body["id_slot"].as_u64() != Some(u64::from(slot)) || body["filename"].as_str() != Some(filename) {
        return Err(QuoteMiss::Mismatch);
    }
    body["projected_bytes"].as_u64().filter(|bytes| *bytes > 0).ok_or(QuoteMiss::NoSize)
}
