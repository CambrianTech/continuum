//! A resident's captured acts, oldest first, read through the capture store's own playback reader
//! (`page`, `detail`) and its one decoder (`prompt_capture::decode`): nothing here parses a row.

use std::collections::HashMap;
use std::path::Path;

use continuum_capture::prompt_capture::{decode, detail, page, CallHeader};
use continuum_cognition::ai::types::TextGenerationRequest;
use continuum_cognition::inference::request_body::ACT_PURPOSE;
use uuid::Uuid;

/// One act as she sent it.
pub struct CapturedAct {
    /// When it was submitted (the submitted row's `captured_at_ms`).
    pub at_ms: u64,
    /// The room it was for (the capture header's).
    pub room: Uuid,
    /// The window her binding fitted it to; `None` for a capture with no window receipt.
    pub window: Option<u32>,
    pub request: TextGenerationRequest,
    /// The engine replied: only then did her view take its tail as sent.
    pub served: bool,
}

/// One resident's capture.
pub struct Resident {
    pub persona: Uuid,
    pub acts: Vec<CapturedAct>,
    /// Calls the decoder refused (integrity, an older schema), named so a thin replay is visible.
    pub undecodable: usize,
}

/// Every act in the capture at `path` (`<persona>.jsonl` in its capture directory) submitted at or
/// after `since_ms`, oldest first.
pub fn resident(path: &Path, since_ms: u64) -> Result<Resident, String> {
    let persona: Uuid = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("{}: no file stem to name the persona", path.display()))?
        .parse()
        .map_err(|error| format!("{}: the file stem is not a persona id: {error}", path.display()))?;
    let dir = path.parent().ok_or_else(|| format!("{}: no capture directory", path.display()))?;
    // The newest header per request: a terminal row supersedes its submitted row.
    let mut headers: HashMap<String, CallHeader> = HashMap::new();
    let mut position: Option<String> = None;
    loop {
        let found = page(dir, persona, position.as_deref(), false, usize::MAX).map_err(|error| format!("{}: {error}", path.display()))?;
        for header in found.entries {
            if header.started_at_ms < since_ms {
                continue;
            }
            let newer = headers.get(&header.request_id).is_some_and(|seen| seen.captured_at_ms >= header.captured_at_ms);
            if !newer {
                headers.insert(header.request_id.clone(), header);
            }
        }
        match found.older {
            Some(older) => position = Some(older),
            None => break,
        }
    }
    let mut acts = Vec::new();
    let mut undecodable = 0;
    for header in headers.into_values() {
        let call = detail(dir, persona, &header.cursor).map_err(|error| format!("{persona} {}: {error}", header.request_id))?;
        let call = match decode(call) {
            Ok(call) => call,
            Err(_) => {
                undecodable += 1;
                continue;
            }
        };
        if call.request.purpose.as_deref() != Some(ACT_PURPOSE) {
            continue;
        }
        acts.push(CapturedAct {
            at_ms: header.started_at_ms,
            room: header.room_id,
            window: call.context_window,
            served: call.response.is_some(),
            request: call.request,
        });
    }
    acts.sort_by_key(|act| act.at_ms);
    Ok(Resident { persona, acts, undecodable })
}
