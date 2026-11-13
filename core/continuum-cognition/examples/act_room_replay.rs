//! Replay a resident's real turn sequence through her output-room rule and count the turns it
//! would cut: the validator of the censored answer need (`working_set/need.rs`; the M5 2026-11-07,
//! fe91c2b55: turns cut at `max_tokens` mid-answer at rooms of 853, 288 and 435 tokens).
//!
//! `cargo run -p continuum-cognition --example act_room_replay -- <prompt-capture dir> [turns]`
//!
//! Each persona's captures are read through the capture store's own playback reader
//! (`continuum_capture::prompt_capture::{page, detail}`: the `.pidx` index and its `.prev`
//! generation, the newest header per request). Every completed generation, oldest first, feeds a
//! fresh [`WorkingSetRegistry`] through the same calls the faculty makes (`ResponseEmission::of`,
//! `record_emission_in_memory`). Before each turn the room her need grants is read: an act
//! (`cognition/act`) at her act room (`OutputNeed::act_room`, the reply claim's floor), a pass
//! (`cognition/deliberation`, whose `max_tokens` is its grant) at her whole need
//! (`OutputNeed::total`, the claim's want before a time term or a pending cut). The turn is replayed
//! under that room with her think bounded at the room past her measured answer, as the request
//! bounds it (`ActRoom::think_within`). A turn that landed live is cut when its demand reaches the
//! room, and is then recorded as that cut. A turn cut live is a known cut when the room is no
//! larger than what it wrote, and otherwise UNKNOWN (its true need was never seen): it is recorded
//! as it happened, a lower bound. A turn before her first measurement is recorded as it happened.
//!
//! Not modelled: the allocation's growth of a grant above its floor by priority, the time term's
//! upward test, and a pending cut's retained peak. Run it once on each rule (before and after a
//! change to the need): it prints one row per resident and kind, the would-be cuts over her newest
//! `turns` turns of that kind (default 200), the unknowns, and the median and p90 room granted,
//! beside the median `maxTokens` the live core sent. `ACT_ROOM_REPLAY_CUTS=1` also prints each
//! would-be cut (her room, its answer share, what the turn wrote) to stderr.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use continuum_capture::prompt_capture::{detail, page, CallHeader};
use continuum_cognition::ai::types::TextGenerationResponse;
use continuum_cognition::cognition::working_set::{ActRoom, EmissionStop, ResponseEmission, WorkingSetRegistry};
use uuid::Uuid;

/// Which room a turn is sent with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Act,
    Pass,
}

/// One completed generation as captured: its kind, the `max_tokens` it was sent with, the
/// response, and when it started.
struct Generation {
    started_at_ms: u64,
    kind: Option<Kind>,
    max_tokens: u32,
    response: TextGenerationResponse,
}

/// Every completed generation of `persona`, oldest first (newest header per request).
fn generations(dir: &Path, persona: Uuid) -> Result<Vec<Generation>, String> {
    let mut headers: HashMap<String, CallHeader> = HashMap::new();
    let mut position: Option<String> = None;
    loop {
        let page = page(dir, persona, position.as_deref(), false, usize::MAX).map_err(|e| format!("{persona}: {e}"))?;
        for header in page.entries {
            let newer = headers.get(&header.request_id).is_some_and(|seen| seen.captured_at_ms >= header.captured_at_ms);
            if !newer {
                headers.insert(header.request_id.clone(), header);
            }
        }
        match page.older {
            Some(older) => position = Some(older),
            None => break,
        }
    }
    let mut out = Vec::new();
    for header in headers.into_values().filter(|h| h.terminal.is_some()) {
        let call = detail(dir, persona, &header.cursor).map_err(|e| format!("{persona} {}: {e}", header.request_id))?;
        let (Some(submitted), Some(terminal)) = (call.submitted, call.terminal) else { continue };
        let request = &submitted["request"];
        let Ok(response) = serde_json::from_value::<TextGenerationResponse>(terminal["response"].clone()) else { continue }; // BOUNDARY: a captured terminal row; one without a response (a failure) measured nothing
        let kind = match request["purpose"].as_str() {
            Some("cognition/act") => Some(Kind::Act),
            Some("cognition/deliberation") => Some(Kind::Pass),
            _ => None,
        };
        out.push(Generation {
            started_at_ms: header.started_at_ms,
            kind,
            max_tokens: request["maxTokens"].as_u64().map_or(0, |m| m as u32), // map_or: a request with no cap reads 0 on the report
            response,
        });
    }
    out.sort_by_key(|g| g.started_at_ms);
    Ok(out)
}

#[derive(Default)]
struct Row {
    turns: usize,
    live_cuts: usize,
    would_cut: usize,
    unknown: usize,
    rooms: Vec<u32>,
    sent: Vec<u32>,
}

fn quantile(values: &mut [u32], num: usize, den: usize) -> u32 {
    values.sort_unstable();
    match values.len() {
        0 => 0,
        n => values[((n * num).div_ceil(den)).saturating_sub(1).min(n - 1)],
    }
}

/// Replay `gens` through a fresh registry; count over the newest `window` measured turns per kind.
fn replay(persona: Uuid, gens: &[Generation], window: usize) -> HashMap<Kind, Row> {
    let reg = WorkingSetRegistry::new();
    let mut counted: HashSet<usize> = HashSet::new();
    for kind in [Kind::Act, Kind::Pass] {
        let of_kind: Vec<usize> = gens.iter().enumerate().filter(|(_, g)| g.kind == Some(kind)).map(|(i, _)| i).collect();
        counted.extend(of_kind[of_kind.len().saturating_sub(window)..].iter().copied());
    }
    let mut rows: HashMap<Kind, Row> = HashMap::new();
    for (i, g) in gens.iter().enumerate() {
        let live = ResponseEmission::of(&g.response, None, g.max_tokens);
        let out = g.response.usage.output_tokens;
        let think = live.reasoning_tokens;
        let need = reg.need_of(persona);
        let granted = match (g.kind, need) {
            (Some(Kind::Act), Some(n)) => Some(n.act_room()),
            (Some(Kind::Pass), Some(n)) => Some(n.total()),
            _ => None,
        };
        let (record_out, record_think, stop) = match (g.kind, granted.filter(|&room| room > 0)) {
            (Some(kind), Some(room)) => {
                let think_budget = ActRoom::of(need).think_within(room).unwrap_or(room); // unwrap_or: a measured room always has a think share
                let think_used = think.min(think_budget);
                let wrote = think_used.saturating_add(out - think);
                let cut_live = matches!(live.stop, EmissionStop::CutMidAnswer | EmissionStop::CutMidThought);
                let tally = counted.contains(&i);
                let row = rows.entry(kind).or_default();
                if tally {
                    row.turns += 1;
                    row.live_cuts += usize::from(cut_live);
                    row.rooms.push(room);
                    row.sent.push(g.max_tokens);
                }
                if wrote >= room {
                    row.would_cut += usize::from(tally);
                    if tally && std::env::var_os("ACT_ROOM_REPLAY_CUTS").is_some() {
                        eprintln!("cut {persona} {kind:?} room={room} answer_room={} think_live={think} answer_live={} wrote={wrote} live_stop={:?}", room - think_budget, out - think, live.stop);
                    }
                    let think_at_cut = think_used.min(room);
                    (room, think_at_cut, EmissionStop::classify(true, room > think_at_cut))
                } else if cut_live {
                    row.unknown += usize::from(tally);
                    (out, think, live.stop)
                } else {
                    (wrote, think_used, live.stop)
                }
            }
            _ => (out, think, live.stop),
        };
        if record_out > 0 {
            reg.record_emission_in_memory(persona, record_out, record_think, stop, g.started_at_ms);
        }
    }
    rows
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or("usage: act_room_replay <prompt-capture dir> [turns]")?);
    let window = match args.next() {
        Some(n) => n.parse::<usize>().map_err(|e| format!("turns: {e}"))?,
        None => 200,
    };
    let mut personas: Vec<Uuid> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.strip_suffix(".pidx")?.parse::<Uuid>().ok())
        .collect();
    personas.sort();
    println!("| resident | kind | turns | live cuts | would-be cuts | unknown | median room | p90 room | median maxTokens sent |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for persona in personas {
        let gens = generations(&dir, persona)?;
        let mut rows = replay(persona, &gens, window);
        for kind in [Kind::Act, Kind::Pass] {
            let Some(row) = rows.get_mut(&kind) else { continue };
            let (median, p90) = (quantile(&mut row.rooms, 1, 2), quantile(&mut row.rooms, 9, 10));
            println!(
                "| {persona} | {kind:?} | {} | {} | {} | {} | {median} | {p90} | {} |",
                row.turns,
                row.live_cuts,
                row.would_cut,
                row.unknown,
                quantile(&mut row.sent, 1, 2)
            );
        }
    }
    Ok(())
}
