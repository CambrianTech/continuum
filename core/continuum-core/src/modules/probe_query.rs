//! `debug/probes/query` — read the HISTORICAL probe ledger off disk (#235).
//!
//! ## Why this exists
//!
//! `ProbeStreamModule` already serves `debug/probes/{open,next,close}`: a
//! handle-based subscriber on the **live in-process broadcast**. Nothing read the
//! **on-disk ledger** that [`JsonlProbeFileSink`] has been writing all along. So every
//! post-hoc question — *did she actually call that verb?*, *how many turns rendered a
//! zero budget?* — was answered by shelling out to `grep`/`jq` against
//! `~/.continuum/probes/continuum-probes.jsonl` and hand-rolling the class filter.
//!
//! That gap has a cost beyond ergonomics, and it is the reason this verb exists rather
//! than a shell alias. Per
//! [[a-citizen-saying-i-have-nothing-to-contribute-is-a-substrate-gap-report]], the
//! action ledger is not merely *better* evidence than a citizen's prose about what she
//! did — it is the *only* evidence. An instrument that is awkward to reach for is an
//! instrument that gets skipped in favour of reading chat, which is exactly the
//! prose-blindness that cost hours on #358. Cheap to query = actually queried.
//!
//! ## The honesty requirement (the part that is NOT convenience)
//!
//! This command MUST distinguish three outcomes that all "look empty" from a shell:
//!
//! | outcome | what it means | how it renders |
//! |---|---|---|
//! | `CONTINUUM_PROBE_DIR` unset | glass box is OFF — nothing was ever written | **`Err`, loud**, with the remedy |
//! | dir set, no files | configured but the sink never opened | `Err`, loud, names the path |
//! | files read, 0 matched | a real, honest zero | `Ok` + `matched: 0` + a summary saying so |
//!
//! Collapsing the first two into "no results" is precisely the mistake that produced
//! *"no captures / glass box isn't writing"* on #358 — a claim about my SEARCH dressed
//! up as a claim about the SYSTEM. A zero has to be earned, and the caller has to be
//! able to tell an earned zero from a misconfigured one.
//!
//! ## Denominators, not just hits
//!
//! `matched` is the count BEFORE `limit` is applied, and `scanned` is every line read.
//! A caller who sees 12 rows needs to know whether that was 12 of 12 or 12 of 4,000 —
//! reporting only the returned page is how a sample gets mistaken for a population.
//!
//! ## Not implemented on purpose: ANSI stripping
//!
//! #235 asked for ANSI-strip. Measured before building it: the live ledger contains
//! **zero** ESC bytes (`grep -c $'\033'` → 0). The sink serialises through
//! `serde_json`, so colour codes cannot appear. That requirement came from scraping the
//! *server log*, a different file. A stripper here would be code nothing can reach —
//! the #344 shape — so it is deliberately absent rather than written-and-dead.

use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::routing::probe_file_sink::{class_passes_filter, DEFAULT_MAX_LOG_FILES, ENV_PROBE_DIR};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Base filename the rolling sink writes; generations land beside it as `.1`, `.2`, …
/// Must match [`JsonlProbeFileSink::new_rolling`] — one name, one place.
const LEDGER_BASENAME: &str = "continuum-probes.jsonl";

/// Rows returned when the caller does not say. Small enough to read, large enough to
/// show a pattern; `matched` always reports the true total so this is never mistaken
/// for the population.
const DEFAULT_LIMIT: u32 = 50;

/// Hard ceiling on rows in one response — a `limit` of 100k would blow the IPC frame.
const MAX_LIMIT: u32 = 2_000;

// ─────────────────────────── debug/probes/query ──────────────────────────

/// Query the historical probe ledger. Stateless — it holds no deps, so it
/// self-registers rather than being handed out by a host module's `commands()`.
#[derive(Default)]
pub struct ProbeQuery;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeQueryParams {
    /// Class filters. Exact match OR namespace prefix (`persona` matches
    /// `persona.turn.spoke` but NOT `personality.x`), `*` = every class. Same rule as
    /// `CONTINUUM_PROBE_CLASSES` and `tracing`'s `EnvFilter` — one convention, reused.
    #[serde(default)]
    pub class: Option<Vec<String>>,
    /// Only events captured at or after this epoch-ms watermark.
    #[serde(default)]
    pub since_ms: Option<u64>,
    /// Case-insensitive substring over the message AND every field value — the
    /// "I know a persona's name but not which class carries it" search.
    #[serde(default)]
    pub contains: Option<String>,
    /// Project only these field keys. Omit for all fields.
    #[serde(default)]
    pub fields: Option<Vec<String>>,
    /// Max rows returned (newest kept). Default 50, capped at 2000.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// One ledger row, projected.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRow {
    pub captured_at_ms: u64,
    pub class: String,
    pub message: String,
    /// Field map, narrowed to `params.fields` when the caller asked for a projection.
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProbeQueryResult {
    /// Chronological, oldest → newest. The newest `limit` matches.
    pub events: Vec<ProbeRow>,
    /// Total matches BEFORE `limit` — the DENOMINATOR. `events.len() < matched` means
    /// the caller is looking at a page, not the population.
    pub matched: u32,
    /// Every line read across all generations, matched or not.
    pub scanned: u32,
    /// Ledger files actually read, oldest generation first.
    pub sources: Vec<String>,
    /// Plain-language reading, so a zero explains itself instead of being interpreted.
    pub summary: String,
}

#[async_trait]
impl ActionCommand for ProbeQuery {
    const NAME: &'static str = "debug/probes/query";
    const ALIASES: &'static [&'static str] = &[
        "probes",
        "probe_query",
        "probe_search",
        "glass_box",
        "debug/probes/search",
    ];
    // Operator/agent diagnostic, NOT part of the persona tool surface: one query can
    // return every citizen's cognition trace. That is a privacy scope question and a
    // window-budget hazard (#333), so it stays off the offered vocabulary until someone
    // decides a persona should see her PEERS' internals.
    const NATIVE: bool = false;
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Search the recorded probe ledger — the glass-box record of load-bearing \
         decisions across every task. Filter by class (exact or namespace prefix, \
         e.g. `persona` covers all `persona.*`), by a since-watermark, or by a \
         substring over messages and field values; project specific fields. Reports \
         how many events MATCHED versus how many were returned, so a page is never \
         mistaken for the whole. Reads history from disk — for the live tail use \
         debug/probes/open.";
    type Params = ProbeQueryParams;
    type Output = ProbeQueryResult;

    async fn run(&self, _ctx: &Ctx, p: ProbeQueryParams) -> Result<ProbeQueryResult, CommandError> {
        // Distinguish "glass box off" from "on and quiet" — see the module doc. An
        // unset var is a CONFIGURATION answer, never an empty result set.
        let dir = std::env::var(ENV_PROBE_DIR).map_err(|_| {
            CommandError::Invalid(format!(
                "the probe ledger is not being written: {ENV_PROBE_DIR} is unset, so the \
                 glass box is OFF and there is no history to search. This is a \
                 configuration state, NOT an empty result. Set it in ~/.continuum/config.env \
                 (start-server.sh sources that file with `set -a`, so exporting it in your \
                 shell alone will be overwritten) and restart the core."
            ))
        })?;
        let dir = PathBuf::from(dir);

        let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let filter: HashSet<String> = p.class.clone().unwrap_or_default().into_iter().collect();
        let needle = p.contains.as_ref().map(|s| s.to_lowercase());
        let project: Option<HashSet<String>> =
            p.fields.clone().map(|f| f.into_iter().collect());

        // Reading up to ~24MB of JSONL is blocking work; it never runs on the async
        // executor (CONCURRENCY-STYLE-GUIDE: spawn_blocking for filesystem scans).
        let scan = tokio::task::spawn_blocking(move || {
            scan_ledger(&dir, &filter, p.since_ms, needle.as_deref(), project.as_ref(), limit)
        })
        .await
        .map_err(|e| CommandError::Internal(format!("probe ledger scan panicked: {e}")))??;

        Ok(ProbeQueryResult {
            summary: summarize(&scan, limit),
            events: scan.events,
            matched: scan.matched,
            scanned: scan.scanned,
            sources: scan.sources,
        })
    }
}

/// Raw scan output, before the summary sentence is composed.
#[derive(Debug)]
struct Scan {
    events: Vec<ProbeRow>,
    matched: u32,
    scanned: u32,
    sources: Vec<String>,
}

/// Walk every ledger generation oldest-first, keeping the newest `limit` matches.
///
/// Split out so the filtering rules are unit-testable against a temp dir without an
/// async runtime, a live core, or an env var.
fn scan_ledger(
    dir: &PathBuf,
    filter: &HashSet<String>,
    since_ms: Option<u64>,
    needle: Option<&str>,
    project: Option<&HashSet<String>>,
    limit: u32,
) -> Result<Scan, CommandError> {
    // Oldest generation first so a straight append yields chronological order: the
    // sink rotates live → `.1` → `.2`, so higher suffix = older.
    let mut paths: Vec<PathBuf> = (1..DEFAULT_MAX_LOG_FILES)
        .rev()
        .map(|n| dir.join(format!("{LEDGER_BASENAME}.{n}")))
        .filter(|p| p.is_file())
        .collect();
    let live = dir.join(LEDGER_BASENAME);
    if live.is_file() {
        paths.push(live);
    }

    if paths.is_empty() {
        // Configured but nothing on disk: still a configuration answer, not a zero.
        return Err(CommandError::Internal(format!(
            "{ENV_PROBE_DIR} points at {} but no {LEDGER_BASENAME} exists there — the sink \
             has never opened. Nothing was recorded, so this is not an empty search result.",
            dir.display()
        )));
    }

    let mut kept: VecDeque<ProbeRow> = VecDeque::with_capacity(limit as usize);
    let mut matched: u32 = 0;
    let mut scanned: u32 = 0;
    let mut sources = Vec::with_capacity(paths.len());

    for path in &paths {
        let file = File::open(path).map_err(|e| {
            CommandError::Internal(format!("probe ledger unreadable at {}: {e}", path.display()))
        })?;
        sources.push(path.display().to_string());

        for line in BufReader::new(file).lines() {
            let Ok(line) = line else { continue };
            if line.trim().is_empty() {
                continue;
            }
            scanned += 1;
            // A partially-flushed tail line is normal on a live file, not an error.
            let Ok(row) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(row) = to_row(&row, filter, since_ms, needle, project) else {
                continue;
            };
            matched += 1;
            if kept.len() == limit as usize {
                kept.pop_front();
            }
            kept.push_back(row);
        }
    }

    Ok(Scan {
        events: kept.into_iter().collect(),
        matched,
        scanned,
        sources,
    })
}

/// Apply every filter to one raw ledger line; `None` = filtered out.
fn to_row(
    raw: &Value,
    filter: &HashSet<String>,
    since_ms: Option<u64>,
    needle: Option<&str>,
    project: Option<&HashSet<String>>,
) -> Option<ProbeRow> {
    let class = raw.get("class")?.as_str()?;
    // The SAME predicate the sink writes with — exact-or-namespace-prefix lives in one
    // place, so a query can never disagree with what was captured.
    if !class_passes_filter(class, filter) {
        return None;
    }
    let captured_at_ms = raw.get("captured_at_ms").and_then(Value::as_u64)?;
    if since_ms.is_some_and(|w| captured_at_ms < w) {
        return None;
    }
    let message = raw.get("message").and_then(Value::as_str).unwrap_or("");
    let fields = raw
        .get("fields")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(needle) = needle {
        // Search the message AND every field value — the caller usually knows a name
        // or an id, not which key carries it.
        let hit = message.to_lowercase().contains(needle)
            || fields.values().any(|v| match v {
                Value::String(s) => s.to_lowercase().contains(needle),
                other => other.to_string().to_lowercase().contains(needle),
            });
        if !hit {
            return None;
        }
    }

    let fields = match project {
        Some(keys) => fields
            .into_iter()
            .filter(|(k, _)| keys.contains(k))
            .collect(),
        None => fields,
    };

    Some(ProbeRow {
        captured_at_ms,
        class: class.to_string(),
        message: message.to_string(),
        fields,
    })
}

/// State what the counts MEAN. Kept separate so the "how does a zero read" rule lives
/// in one testable place rather than inline in the handler.
fn summarize(scan: &Scan, limit: u32) -> String {
    let files = scan.sources.len();
    if scan.matched == 0 {
        return format!(
            "No events matched, out of {} line(s) read across {} ledger file(s). The glass box \
             IS recording — this is a real, honest zero for these filters, not a missing \
             instrument. Widen the class filter or drop the since-watermark to see more.",
            scan.scanned, files
        );
    }
    let shown = scan.events.len();
    if (shown as u32) < scan.matched {
        return format!(
            "Showing the {shown} most recent of {} matching event(s), from {} line(s) across {} \
             ledger file(s). This is a PAGE, not the whole set — raise limit (max {}) or \
             narrow the filters before drawing a conclusion from it.",
            scan.matched, scan.scanned, files, MAX_LIMIT
        );
    }
    format!(
        "All {} matching event(s), from {} line(s) across {} ledger file(s) — this is the \
         complete set for these filters, not a page (limit was {limit}).",
        scan.matched, scan.scanned, files
    )
}

// `register_command!` alone registers only the DESCRIPTOR — which is what the
// suggester and the ACL read, and NOT what dispatch needs. Using it here produced
// the perfect false signal: "Unknown command: 'debug/probes/query'. Did you mean:
// debug/probes/query?" — the name was known to the suggester and unroutable by the
// kernel. `register_stateless_command!` registers the descriptor AND the runtime
// constructor, which is what actually makes the verb callable (#344's shape, self-
// inflicted: a correct capability nothing could invoke).
crate::register_stateless_command!(ProbeQuery);

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn row(class: &str, ms: u64, msg: &str, field: (&str, &str)) -> String {
        serde_json::json!({
            "captured_at_ms": ms,
            "class": class,
            "uri_chain": [],
            "message": msg,
            "fields": { field.0: field.1 },
        })
        .to_string()
    }

    fn ledger(lines: &[String]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut f = File::create(dir.path().join(LEDGER_BASENAME)).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        dir
    }

    fn scan(dir: &tempfile::TempDir, filter: &[&str], limit: u32) -> Scan {
        let set: HashSet<String> = filter.iter().map(|s| s.to_string()).collect();
        scan_ledger(&dir.path().to_path_buf(), &set, None, None, None, limit).unwrap()
    }

    /// what this catches (#235/#358): a configured-but-quiet ledger reading the same as
    /// a ledger that was never written. Conflating those is exactly how "no captures"
    /// got reported about a system that was recording fine — the zero must announce
    /// itself as earned.
    #[test]
    fn an_honest_zero_says_the_glass_box_is_recording() {
        let dir = ledger(&[row("persona.turn.start", 10, "hi", ("p", "Asha"))]);
        let s = scan(&dir, &["nothing.matches"], 50);
        assert_eq!(s.matched, 0);
        assert_eq!(s.scanned, 1, "the line was READ, just not matched");
        let text = summarize(&s, 50);
        assert!(text.contains("glass box IS recording"), "{text}");
        assert!(text.contains("honest zero"), "{text}");
    }

    /// what this catches: an unset CONTINUUM_PROBE_DIR, or a dir with no ledger,
    /// degrading into `Ok(empty)`. Both are configuration states and must be errors —
    /// a caller who gets `Ok` with 0 rows will conclude the system was silent.
    #[test]
    fn a_missing_ledger_is_an_error_not_an_empty_result() {
        let empty = tempfile::tempdir().unwrap();
        let err = scan_ledger(
            &empty.path().to_path_buf(),
            &HashSet::new(),
            None,
            None,
            None,
            50,
        )
        .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("never opened"), "{msg}");
        assert!(msg.contains("not an empty search result"), "{msg}");
    }

    /// what this catches: reporting only the returned page, so a caller reads 2 rows as
    /// "there were 2". The denominator is the whole point of the instrument.
    #[test]
    fn the_page_reports_its_denominator_and_keeps_the_newest() {
        let dir = ledger(&[
            row("persona.act.observed", 1, "a", ("v", "room/members")),
            row("persona.act.observed", 2, "b", ("v", "room/members")),
            row("persona.act.observed", 3, "c", ("v", "room/members")),
        ]);
        let s = scan(&dir, &["persona.act.observed"], 2);
        assert_eq!(s.matched, 3, "matched counts BEFORE the limit");
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].captured_at_ms, 2, "newest kept, chronological");
        assert_eq!(s.events[1].captured_at_ms, 3);
        let text = summarize(&s, 2);
        assert!(text.contains("is a PAGE, not the whole set"), "{text}");
    }

    /// what this catches: a query disagreeing with the sink about what a class filter
    /// means. Both sides call class_passes_filter, so `persona` must cover
    /// `persona.turn.start` and must NOT leak into `personality.*`.
    #[test]
    fn namespace_prefix_matches_subclasses_but_not_lookalikes() {
        let dir = ledger(&[
            row("persona.turn.start", 1, "mine", ("k", "v")),
            row("personality.quirk", 2, "not mine", ("k", "v")),
        ]);
        let s = scan(&dir, &["persona"], 50);
        assert_eq!(s.matched, 1, "personality.* must not match the persona filter");
        assert_eq!(s.events[0].class, "persona.turn.start");
    }

    /// what this catches: `contains` only searching the message. The thing an operator
    /// knows is usually a name or id sitting in a FIELD, which is how #358's
    /// "verbs=room/members" evidence was found.
    #[test]
    fn contains_searches_field_values_not_just_the_message() {
        let dir = ledger(&[
            row("persona.act.observed", 1, "acted", ("verbs", "room/members")),
            row("persona.act.observed", 2, "acted", ("verbs", "code/read")),
        ]);
        let set: HashSet<String> = HashSet::new();
        let s = scan_ledger(
            &dir.path().to_path_buf(),
            &set,
            None,
            Some("room/members"),
            None,
            50,
        )
        .unwrap();
        assert_eq!(s.matched, 1);
        assert_eq!(s.events[0].captured_at_ms, 1);
    }
}
