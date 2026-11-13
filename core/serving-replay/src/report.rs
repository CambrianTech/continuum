//! What a replay reports: one record per act, a summary, and a one-screen rendering.
//! The JSON is the receipt a slice's PR pastes; the screen is what a person reads.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::target::{ActOutcome, EngineStats, MindId, PageInKind, PageInReceipt};

#[derive(Debug, Clone, Serialize)]
pub struct ActRecord {
    pub index: usize,
    pub mind: MindId,
    pub source: String,
    /// Her first act in the replay: nothing of hers can be reused yet.
    pub first_act: bool,
    /// The reuse the act's prompt makes available (`acts::available_reuse`).
    pub available_reuse: f64,
    pub page_in: Option<PageInReceipt>,
    pub outcome: Option<ActOutcome>,
    /// page_in + act, measured by the replay.
    pub wall_ms: f64,
    pub error: Option<String>,
}

impl ActRecord {
    /// Reuse achieved: cached tokens / prompt tokens.
    pub fn reuse(&self) -> Option<f64> {
        self.outcome.as_ref().filter(|o| o.prompt_tokens > 0).map(|o| o.cached_tokens as f64 / o.prompt_tokens as f64)
    }

    pub fn line(&self) -> String {
        let kind = self.page_in.as_ref().map_or("-", |p| p.kind.as_str());
        match (&self.outcome, &self.error) {
            (Some(o), _) => format!(
                "act {:>3} {:<12} {:<16} prompt {:>6} cached {:>6} fresh {:>6} reuse {:>5.1}% (avail {:>5.1}%) {:>8.0} ms",
                self.index,
                short(&self.mind),
                kind,
                o.prompt_tokens,
                o.cached_tokens,
                o.fresh_tokens,
                self.reuse().unwrap_or_default() * 100.0, // unwrap_or_default: an empty prompt reused nothing
                self.available_reuse * 100.0,
                self.wall_ms
            ),
            (None, e) => format!("act {:>3} {:<12} {:<16} FAILED: {}", self.index, short(&self.mind), kind, e.as_deref().unwrap_or("no outcome")), // unwrap_or: a missing outcome with no error text
        }
    }
}

fn short(mind: &MindId) -> &str {
    mind.0.get(..8).unwrap_or(&mind.0) // unwrap_or: ids shorter than 8 chars print whole
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub acts: usize,
    pub failed: usize,
    /// Token-weighted reuse over every completed act: Σ cached / Σ prompt.
    pub reuse: f64,
    /// The same over acts after each mind's first: what keeping a mind warm buys.
    pub warm_reuse: f64,
    /// The prompt-side ceiling for those same acts (mean of `available_reuse`).
    pub warm_available_reuse: f64,
    pub prompt_tokens: u64,
    pub fresh_tokens: u64,
    pub fresh_tokens_median: u64,
    pub page_ins: BTreeMap<PageInKind, usize>,
    pub evictions: usize,
    pub restore_ms_median: Option<f64>,
    pub page_out_ms_median: Option<f64>,
    pub page_out_bytes_median: Option<u64>,
    pub relaunches: Option<u32>,
    pub act_wall_ms_median: f64,
    pub act_wall_ms_p90: f64,
}

fn quantile(mut v: Vec<f64>, q: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    Some(v[((v.len() - 1) as f64 * q).round() as usize])
}

impl Summary {
    pub fn of(acts: &[ActRecord], engine: &EngineStats) -> Self {
        let done: Vec<(&ActRecord, &ActOutcome)> = acts.iter().filter_map(|a| a.outcome.as_ref().map(|o| (a, o))).collect();
        let warm: Vec<&(&ActRecord, &ActOutcome)> = done.iter().filter(|(a, _)| !a.first_act).collect();
        let ratio = |num: u64, den: u64| if den == 0 { 0.0 } else { num as f64 / den as f64 };
        let mut page_ins = BTreeMap::new();
        for p in acts.iter().filter_map(|a| a.page_in.as_ref()) {
            *page_ins.entry(p.kind).or_insert(0) += 1;
        }
        let evicted: Vec<_> = acts.iter().filter_map(|a| a.page_in.as_ref()?.evicted.as_ref()).collect();
        let restores = acts.iter().filter_map(|a| a.page_in.as_ref()).filter(|p| p.kind == PageInKind::Restored).map(|p| p.ms).collect();
        let walls: Vec<f64> = acts.iter().map(|a| a.wall_ms).collect();
        Self {
            acts: acts.len(),
            failed: acts.len() - done.len(),
            reuse: ratio(done.iter().map(|(_, o)| o.cached_tokens).sum(), done.iter().map(|(_, o)| o.prompt_tokens).sum()),
            warm_reuse: ratio(warm.iter().map(|(_, o)| o.cached_tokens).sum(), warm.iter().map(|(_, o)| o.prompt_tokens).sum()),
            warm_available_reuse: if warm.is_empty() { 0.0 } else { warm.iter().map(|(a, _)| a.available_reuse).sum::<f64>() / warm.len() as f64 },
            prompt_tokens: done.iter().map(|(_, o)| o.prompt_tokens).sum(),
            fresh_tokens: done.iter().map(|(_, o)| o.fresh_tokens).sum(),
            fresh_tokens_median: quantile(done.iter().map(|(_, o)| o.fresh_tokens as f64).collect(), 0.5).unwrap_or_default() as u64, // unwrap_or_default: no completed act prefilled nothing
            page_ins,
            evictions: evicted.len(),
            restore_ms_median: quantile(restores, 0.5),
            page_out_ms_median: quantile(evicted.iter().map(|e| e.ms).collect(), 0.5),
            page_out_bytes_median: quantile(evicted.iter().map(|e| e.bytes as f64).collect(), 0.5).map(|b| b as u64),
            relaunches: engine.relaunches,
            act_wall_ms_median: quantile(walls.clone(), 0.5).unwrap_or_default(), // unwrap_or_default: an empty replay took no time
            act_wall_ms_p90: quantile(walls, 0.9).unwrap_or_default(), // unwrap_or_default: an empty replay took no time
        }
    }
}

/// The model the engine served: path, GGUF architecture, and whether that architecture is
/// hybrid (recurrent + attention), which decides whether partial KV rollback works.
#[derive(Debug, Clone, Serialize)]
pub struct ModelLabel {
    pub path: String,
    pub arch: Option<String>,
    /// `None` when the vendor source that owns the list was not available.
    pub hybrid: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub sequence: String,
    pub model: ModelLabel,
    pub engine: EngineStats,
    /// Time from launch to the engine answering ready (only when the replay launched it).
    pub engine_ready_ms: Option<f64>,
    pub replay_wall_ms: f64,
    pub summary: Summary,
    pub acts: Vec<ActRecord>,
}

impl Report {
    pub fn screen(&self) -> String {
        let s = &self.summary;
        let pct = |x: f64| format!("{:.1}%", x * 100.0);
        let ms = |x: Option<f64>| x.map_or("-".to_owned(), |v| format!("{v:.0} ms"));
        let hybrid = match self.model.hybrid {
            Some(true) => "hybrid",
            Some(false) => "DENSE (not hybrid: checkpoint/rollback paths are not exercised)",
            None => "hybrid: unknown",
        };
        let kinds: Vec<String> = s.page_ins.iter().map(|(k, n)| format!("{}={n}", k.as_str())).collect();
        [
            format!("serving-replay  {}", self.sequence),
            format!("model    {} ({}, {hybrid})", self.model.path, self.model.arch.as_deref().unwrap_or("arch unknown")), // unwrap_or: an unreadable header is said, not guessed
            format!("engine   {} seats={} relaunches={}", self.engine.engine, self.engine.seats, self.engine.relaunches.map_or("unobserved".to_owned(), |n| n.to_string())),
            format!("acts     {} ({} failed)  wall {:.1} s  act median {:.0} ms  p90 {:.0} ms", s.acts, s.failed, self.replay_wall_ms / 1e3, s.act_wall_ms_median, s.act_wall_ms_p90),
            format!("reuse    all {}   warm {} of an available {}", pct(s.reuse), pct(s.warm_reuse), pct(s.warm_available_reuse)),
            format!("prefill  {} fresh of {} prompt tokens  (median {} fresh per act)", s.fresh_tokens, s.prompt_tokens, s.fresh_tokens_median),
            format!("page-in  {}   evictions {}", kinds.join(" "), s.evictions),
            format!("paging   restore median {}  page-out median {} ({} bytes)", ms(s.restore_ms_median), ms(s.page_out_ms_median), s.page_out_bytes_median.map_or("-".to_owned(), |b| b.to_string())),
        ]
        .join("\n")
    }
}
