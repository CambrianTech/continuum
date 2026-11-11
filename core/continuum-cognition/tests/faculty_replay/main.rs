//! The deliberation faculty's REAL prompt path over captured resident acts.
//!
//! The allocation dyno (`continuum-context/examples/replay/{allocate,epoch}.rs`) imitates the
//! faculty's view and epoch logic and was wrong three times. This replay runs the code we ship:
//! one `LlmDeliberationFaculty` per resident (identity, tools, working memory, window from her
//! capture), stepped act by act through `prompt_view_within`, so her view epoch and tool head carry
//! forward as they do live, and after each act the engine served her tail joins her view as sent
//! (`serve_view`, the request path's `view_served`). Every number it prints is about that code.
//!
//! ```text
//! FACULTY_REPLAY_CAPTURES=<dir>/<persona>.jsonl,<dir>/<persona>.jsonl \
//! FACULTY_REPLAY_SINCE=2026-11-10T21:00:00Z \
//!   cargo test -p continuum-cognition --features test-fixtures --test faculty_replay -- --ignored --nocapture
//! ```
//!
//! `FACULTY_REPLAY_ACTS=1` also prints one line per act. Each resident prints: acts, turnovers
//! (her view's epoch changed between acts), median and mean rendered chars, the mean share of each
//! prompt its predecessor's prefix covers, the mean fresh chars (rendered less that prefix), the
//! same three from the CAPTURED requests (their fidelity check), and the allocation's mean grants.
//!
//! Modules: `decode` (the capture store's reader and decoder), `rebuild` (a captured request back
//! into a workspace and her working memory), `step` (the faculty, act by act), `report`, `wire`
//! (one rendering of a request for both sides, and the shared-prefix count).
//!
//! FIDELITY LIMITS (v1), each a place the replay's prompt can differ from the live one:
//! - Her view's sources are rebuilt from the sent bytes, not her live producers: every message
//!   that is not a grounding block, her own act, native protocol or an earlier tail becomes one
//!   conversation turn (her thoughts, tasks, notices, held-work and facts included), so their
//!   levels (summary, pointer) and source priorities are lost, and an item the live view had
//!   already collapsed arrives as its collapsed text, a new item.
//! - Grounding units are keyed by content (the live keys are the sources'), carry no summary or
//!   pointer level, and a block's unit set per act is every distinct body her captured view held
//!   under that tag, so an epoch's superseded versions stay offered until a captured turnover.
//! - Her acts enter working memory as legacy receipts (`record_receipt`): no native calls, no
//!   typed observations, no spill custody, so her newest result renders as the legacy
//!   `Full result …` message and her actions stream holds whole results without handles.
//! - Her thoughts (reasoning) are not replayed into working memory, and her held work (cards),
//!   room updates and recall are absent; the Working Presence contract cannot be reproduced.
//! - Her authorized tool set is the union of the verbs her captured requests offered, not her
//!   whole registry; her learned tool head starts empty.
//! - No working-set registry (no measured output need) and no engine-measured density: the reply
//!   claim is unmeasured and every act is priced at the default calibration.
//! - Every submitted act is replayed, including a capacity retry of the same turn (each is its
//!   own captured request); only an act the engine served marks her tail as sent.
//! - The clock renders in this machine's timezone, not the capturing node's.

mod decode;
mod rebuild;
mod report;
mod step;
mod wire;

use std::path::Path;

// what this catches: a finding about her prompt (reuse, turnovers, grants) that holds for an
// imitation of the faculty but not for the faculty itself. Ignored: it needs live captures.
#[test]
#[ignore = "replays captured resident acts: set FACULTY_REPLAY_CAPTURES (and FACULTY_REPLAY_SINCE)"]
fn faculty_replay() -> Result<(), String> {
    let captures = std::env::var("FACULTY_REPLAY_CAPTURES")
        .map_err(|_| "set FACULTY_REPLAY_CAPTURES=<comma-separated capture .jsonl paths>".to_owned())?;
    let since_ms = match std::env::var("FACULTY_REPLAY_SINCE") {
        Ok(since) => chrono::DateTime::parse_from_rfc3339(since.trim())
            .map_err(|error| format!("FACULTY_REPLAY_SINCE={since}: not an RFC 3339 UTC time: {error}"))?
            .timestamp_millis()
            .max(0) as u64,
        Err(_) => 0,
    };
    let per_act = std::env::var("FACULTY_REPLAY_ACTS").is_ok_and(|value| value == "1");
    // The faculty's prompt path reads substrate state that may reach the runtime.
    let runtime = tokio::runtime::Runtime::new().map_err(|error| format!("tokio runtime: {error}"))?;
    let _entered = runtime.enter();
    let mut failed = Vec::new();
    for path in captures.split(',').map(str::trim).filter(|path| !path.is_empty()) {
        let replayed = decode::resident(Path::new(path), since_ms)
            .and_then(|resident| step::replay(&resident).map(|(name, samples)| (resident, name, samples)));
        match replayed {
            Ok((resident, name, samples)) => {
                println!("{}", report::line(&resident, &name, &samples));
                if per_act {
                    for (index, sample) in samples.iter().enumerate() {
                        println!("{}", report::act_line(index, sample));
                    }
                }
            }
            Err(error) => {
                println!("{path}: {error}");
                failed.push(error);
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} capture(s) could not be replayed: {}", failed.len(), failed.join("; ")))
    }
}
