//! THE TRAINING PLANNER: the window and chunk an in-engine LoRA run trains at, from what was
//! measured. Pure arithmetic over measurements (footprint rows, `/train/measure`'s per-example
//! lengths), no IO and no engine calls; the adapter (`engine_lora_adapter.rs`) gathers the
//! measurements, asks here, and runs. Moved out of the adapter verbatim (it had grown to 3,734
//! lines mixing the run's IO, the lane, the footprint store and this planner), so the plan can
//! be read, tested and reasoned about as one thing. It lives in inference/planning (card
//! c8165aa6, from genome/fine_tuning/engine_plan.rs) because serving plans against it too: a
//! host is eligible for a base only with one learning context reserved beside serving (card
//! 3b8fb153), and that reserve must be THIS calculation, not a second estimate. Storage (the
//! footprint rows) stays with its owner and is passed in.
use serde::{Deserialize, Serialize};

/// The engine's own default chunk when a caller sends none (`server-train.cpp`,
/// `train_chunk_for(window, 512)`): the chunk every footprint row before `chunk` existed was
/// measured at.
pub const TRAINING_CHUNK: u32 = 512;
pub const fn default_chunk() -> u32 {
    TRAINING_CHUNK
}
/// The smallest chunk the engine accepts (fork #52: 64, 128, or a multiple of 256). Under 256
/// a step is slower but its explicit attention scores (window x chunk x heads, no flash
/// backward) shrink with it, and at chunk 256 they were ~75% of Qwen3.8-27B's training graph on
/// the 5090 (Fable, 2026-10-11), so a window that holds her whole lived turns fits beside serving.
pub const TRAINING_CHUNK_MIN: u32 = 64;


/// The chunk the engine will actually train at for `window` when asked for `asked`: the
/// largest multiple of 256 that divides the window and is at most `asked` — the engine's
/// `train_chunk_for`, mirrored so the shape the core keys its footprint under is the shape
/// the engine runs.
pub fn train_chunk_for(window: u32, asked: u32) -> u32 {
    // under 256, 128 or 64 (each divides a window, which is a multiple of 256); the engine's
    // `train_chunk_for` (fork #52), mirrored so the shape keyed here is the shape that runs
    if asked < 128 {
        return 64;
    }
    if asked < 256 {
        return 128;
    }
    let blocks = window / 256;
    let mut g = blocks.min((asked / 256).max(1));
    while g >= 1 {
        if blocks % g == 0 {
            return g * 256;
        }
        g -= 1;
    }
    256
}

/// The chunk a job trains at and the footprint it leases, from the largest chunk down: a
/// chunk whose measured footprint fits the governed VRAM leases that number; a chunk never
/// measured is a calibration run (leases everything governed and free, `None`); a chunk
/// measured too large for what is free is halved and the next shape asked. The smallest
/// chunk's measurement stands even when it does not fit: the governor then waits for the
/// capacity rather than this guessing smaller than the engine can run.
pub fn choose_chunk(window: u32, available: u64, measured_at: impl Fn(u32) -> Option<u64>) -> (u32, Option<u64>) {
    let mut chunk = train_chunk_for(window, TRAINING_CHUNK);
    loop {
        let measured = measured_at(chunk);
        let next = if chunk / 2 >= TRAINING_CHUNK_MIN { Some(train_chunk_for(window, chunk / 2)) } else { None };
        match (measured, next) {
            (None, _) => return (chunk, None),
            (Some(bytes), _) if bytes <= available => return (chunk, Some(bytes)),
            (Some(bytes), None) => return (chunk, Some(bytes)),
            (Some(_), Some(smaller)) if smaller == chunk => return (chunk, measured),
            (Some(_), Some(smaller)) => chunk = smaller,
        }
    }
}

/// Where a shape's [`Need`] comes from. Kept explicit because they are different claims: a
/// whole run measured it, a refusal proved at least this much (a graph alone over what was free,
/// which the fit never reads), or the fit predicted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/fine_tuning/NeedSource.ts")]
pub enum NeedSource {
    WholeRun,
    LowerBound,
    Predicted,
}

/// What one training shape needs, and on what evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub bytes: u64,
    pub source: NeedSource,
}

impl Need {
    /// The bytes this need DECIDES against `available`, or `None` when it decides nothing. A
    /// whole run or the fit's prediction decides either way. A lower bound only DISQUALIFIES:
    /// over `available` it proves the shape does not fit, but under it proves nothing (the whole
    /// run needs at least that much, perhaps more), so the shape is still unknown and calibrates,
    /// never leasing the bound as if it were the need (Codex on #4985; #4984 leased it).
    pub fn decides(self, available: u64) -> Option<u64> {
        match self.source {
            NeedSource::LowerBound if self.bytes <= available => None,
            _ => Some(self.bytes),
        }
    }

    /// Whether this need PROVES the shape does not fit `available`: a whole run or a prediction
    /// over it, or a lower bound over it. A lower bound under it proves nothing, so it never rules
    /// a shape out (card de9e1342: a parked job wakes to re-plan exactly when this turns false).
    pub fn rules_out(self, available: u64) -> bool {
        self.decides(available).is_some_and(|bytes| bytes > available)
    }
}

/// A shape's need: what is known (its whole-run row, else the fit's prediction), raised to its
/// measured lower bound when that is larger. A graph-only refusal therefore rules its shape out
/// even where the fit is optimistic (card 030d70f0: the 5090 held a graph-only row of 18.6 GB at
/// window 68,608 and re-planned that shape run after run). `None` when nothing is known: the
/// shape is a calibration run.
pub fn need(whole_run: Option<u64>, lower_bound: Option<u64>, predicted: Option<u64>) -> Option<Need> {
    let known = whole_run
        .map(|bytes| Need { bytes, source: NeedSource::WholeRun })
        .or(predicted.map(|bytes| Need { bytes, source: NeedSource::Predicted }));
    match (known, lower_bound) {
        (Some(k), Some(bound)) if bound > k.bytes => Some(Need { bytes: bound, source: NeedSource::LowerBound }),
        (Some(k), _) => Some(k),
        (None, bound) => bound.map(|bytes| Need { bytes, source: NeedSource::LowerBound }),
    }
}

/// The smallest training window the calculation offers: the smallest window any lane is ever
/// served at ([`super::MIN_SERVE_CTX`]). A gene never trains on less
/// context than she could be served in, and under it a lived example has no room for her system
/// and tool head and her reply, so the run would only skip examples.
pub const TRAINING_WINDOW_MIN: u32 = super::MIN_SERVE_CTX;

/// A per-chunk training graph as a function of the window, fitted to measured footprints of ONE
/// shape (model, rank, targets, depth, walk, recompute): bytes ~ chunk x (per_token + per_token_per_pos x window).
/// Two terms because the graph is two costs: the chunk's activations (~chunk) and its
/// attention scores against the cached prefix (~chunk x window; no flash backward, so explicit).
/// Measured on the 5090 2026-10-10: at window 1536 chunk 256 the recompute graph was 2.6 GB,
/// and at window 68608 chunk 512 the engine reported it needed 7.4 GB more than the 2.8 GB it
/// could add, so the window term dominates long lived conversations, and one point cannot
/// separate the two terms.
///
/// A THIRD term once the rows can identify it: the window's KV cache, which grows with the window
/// and not with the chunk (~35 KB a position for the 27B at q8, Fable 2026-10-11). At chunk 256
/// and up it hides inside the window term; at chunk 64 the two-term model would scale it down
/// four-fold and promise a window that does not fit. It is fitted when the rows span two or more
/// chunks and two or more windows (three rows at least); until then the engine's preflight
/// refuses an under-predicted graph and files its true size, which is the row that identifies it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphModel {
    /// bytes per window position, whatever the chunk (the KV cache); 0 until identifiable
    per_pos: f64,
    per_token: f64,
    per_token_per_pos: f64,
}

impl GraphModel {
    /// Whether the rows identified the window's KV-cache term (the third term).
    pub fn kv_term_identified(&self) -> bool {
        self.per_pos > 0.0
    }

    /// The three-term least-squares fit, `None` when the rows cannot identify it or it is not
    /// physical (a negative term). Features are scaled to similar magnitudes (a window of tens of
    /// thousands times a chunk of hundreds) so the normal equations stay well conditioned.
    pub(crate) fn fit_with_kv(rows: &[(u32, u32, u64)]) -> Option<Self> {
        let pts: Vec<(f64, f64, f64)> = rows.iter().filter(|(_, c, _)| *c > 0).map(|&(w, c, b)| (f64::from(w), f64::from(c), b as f64)).collect();
        let distinct = |f: &dyn Fn(&(f64, f64, f64)) -> u64| {
            let mut v: Vec<u64> = pts.iter().map(f).collect();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        if pts.len() < 3 || distinct(&|p| p.0 as u64) < 2 || distinct(&|p| p.1 as u64) < 2 {
            return None;
        }
        const W: f64 = 1.0e4;
        const C: f64 = 256.0;
        // features (w, c, c*w) scaled; y = bytes
        let mut ata = [[0.0f64; 3]; 3];
        let mut aty = [0.0f64; 3];
        for &(w, c, y) in &pts {
            let f = [w / W, c / C, (c / C) * (w / W)];
            for i in 0..3 {
                aty[i] += f[i] * y;
                for j in 0..3 {
                    ata[i][j] += f[i] * f[j];
                }
            }
        }
        let x = solve3(ata, aty)?;
        let (per_pos, per_token, per_token_per_pos) = (x[0] / W, x[1] / C, x[2] / (C * W));
        (per_pos >= 0.0 && per_token >= 0.0 && per_token_per_pos > 0.0 && [per_pos, per_token, per_token_per_pos].iter().all(|v| v.is_finite()))
            .then_some(Self { per_pos, per_token, per_token_per_pos })
    }

    /// A least-squares line through `bytes / chunk` against `window`, over rows at two or more
    /// DISTINCT windows. `None` with fewer, or when the fit has no positive window term (the
    /// window would not be what limits the graph, so no window calculation applies).
    pub fn fit(rows: &[(u32, u32, u64)]) -> Option<Self> {
        if let Some(model) = Self::fit_with_kv(rows) {
            return Some(model);
        }
        let pts: Vec<(f64, f64)> = rows
            .iter()
            .filter(|(_, c, _)| *c > 0)
            .map(|&(w, c, b)| (f64::from(w), b as f64 / f64::from(c)))
            .collect();
        let mut windows: Vec<u64> = pts.iter().map(|(w, _)| *w as u64).collect();
        windows.sort_unstable();
        windows.dedup();
        if windows.len() < 2 {
            return None;
        }
        let n = pts.len() as f64;
        let mean_w = pts.iter().map(|p| p.0).sum::<f64>() / n;
        let mean_g = pts.iter().map(|p| p.1).sum::<f64>() / n;
        let sxx: f64 = pts.iter().map(|p| (p.0 - mean_w).powi(2)).sum();
        let sxy: f64 = pts.iter().map(|p| (p.0 - mean_w) * (p.1 - mean_g)).sum();
        let per_token_per_pos = sxy / sxx;
        if per_token_per_pos <= 0.0 {
            return None;
        }
        let per_token = (mean_g - per_token_per_pos * mean_w).max(0.0);
        Some(Self { per_pos: 0.0, per_token, per_token_per_pos })
    }

    pub fn bytes(&self, window: u32, chunk: u32) -> u64 {
        (self.per_pos * f64::from(window) + f64::from(chunk) * (self.per_token + self.per_token_per_pos * f64::from(window))) as u64
    }
}

/// A 3x3 linear solve by Gaussian elimination with partial pivoting; `None` when singular.
pub(crate) fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    let scale = a.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    if scale == 0.0 {
        return None;
    }
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() <= scale * 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col];
        for row in col + 1..3 {
            let f = a[row][col] / pivot_row[col];
            for (x, p) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *x -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0f64; 3];
    for row in (0..3).rev() {
        let tail: f64 = (row + 1..3).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - tail) / a[row][row];
    }
    Some(x)
}

/// PURE: `rows` (measured at a larger window) as the engine's "middle" rule meets `window`: kept
/// whole when its full length fits (tokens_full <= window + 1), truncated when its smallest
/// does (tokens_min <= window + 1, the head plus her last exchange), skipped otherwise. The
/// trained length of a truncated one is not known without a render, so it reports its smallest
/// (a lower bound).
pub fn refit_middle(rows: Vec<MeasuredExample>, window: u32) -> Vec<MeasuredExample> {
    let holds = u64::from(window) + 1;
    rows.into_iter()
        .map(|mut r| {
            (r.fit, r.tokens_trained) = if r.tokens_full <= holds {
                (ExampleFit::Kept, r.tokens_full)
            } else if r.tokens_min <= holds {
                (ExampleFit::Truncated, r.tokens_min)
            } else {
                (ExampleFit::Skipped, 0)
            };
            r
        })
        .collect()
}

/// PURE: the least a calibration run of (`window`, `chunk`) can need, by measurement: the largest
/// whole-run row of the SAME family (the walk being planned) at a window and a chunk no larger
/// than the candidate's. That the graph and its context grow with both is a PLANNER ASSUMPTION (what
/// the 5090's CUDA rows show), not a proven bound for every backend or allocator; under it such a row
/// is a lower bound. A floor that is never met waits visibly (the job reports waiting_for_capacity),
/// exactly as a measured shape's admission does;
/// a row at a larger window or chunk, or of the other walk, bounds nothing for this candidate and
/// would keep it from ever calibrating (Astra on #4949). `None`: nothing bounds it.
pub fn calibration_floor(rows: &[(u32, u32, u64)], window: u32, chunk: u32) -> Option<u64> {
    rows.iter().filter(|&&(w, c, _)| w <= window && c <= chunk).map(|&(_, _, bytes)| bytes).max()
}

/// PURE: whether the exact walk is measured NOT to fit `window` in `available`: a whole-run row of
/// its family at this window or a smaller one, at any chunk, already exceeded it. Its graph grows
/// with the window and not the chunk, so no smaller chunk rescues it (the 5090, 2026-10-11).
pub fn exact_ruled_out(exact_rows: &[(u32, u32, u64)], window: u32, available: u64) -> bool {
    exact_rows.iter().any(|&(w, _, bytes)| w <= window && bytes > available)
}

/// PURE: the training window an example set NEEDS: the window that trains every one of them
/// WHOLE (the largest `tokens_full`, by the engine's own render; an example fits a window of at
/// least tokens_full - 1), rounded up to the context granularity, at least
/// [`TRAINING_WINDOW_MIN`], never above `served`. Learning sees what serving saw (Joel); only when
/// memory cannot hold that window does it shrink, and then the partition truncates (fit:middle,
/// down to an example's tokens_min) or parks what no longer fits. The window is planned for the
/// data, and the chunk for the window (Fable, 2026-10-11).
pub fn window_for_need(rows: &[MeasuredExample], served: u32) -> u32 {
    // in u64 throughout, clamped to `served` (a u32) before narrowing, so the cast cannot truncate
    let need = rows.iter().fold(0u64, |most, r| most.max(r.tokens_full.saturating_sub(1)));
    let window = need.div_ceil(256).saturating_mul(256).max(u64::from(TRAINING_WINDOW_MIN)).min(u64::from(served));
    window as u32
}

/// The training window, CALCULATED before dispatch (Joel, 2026-10-10: "can't it calculate what it
/// can do instead of failing and us waiting?"). INTERIM: Joel's standing rule is that learning sees
/// what serving sees (train at her served window); the engine's attention memory grows with the
/// cached prefix, so at her 68k window a lived conversation does not fit beside serving. Until that
/// memory stops growing with the prefix (flash-attention backward, or the prefix forward-only as a
/// leaf), the window is the largest the measured graph says fits `available` at the floor chunk,
/// never above `served`, never under [`TRAINING_WINDOW_MIN`]. The engine's fit:"middle" drops her
/// OLDEST history and keeps the system and tool head and her reply (fork #29), which is also how
/// serving fits her perception every turn. With fewer than two windows of this shape measured
/// there is nothing to calculate from: `served`, and that run's footprint (or refusal) is the
/// measurement. The window is in the footprint key, so a later engine that fits climbs back up.
/// Why `choose_window` chose its window, from the SAME evaluation that chose it (Cormac on #4972:
/// a second evaluation of one decision can drift from the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFit {
    /// The fitted model says the window holds at the floor chunk.
    Fits,
    /// The rows can't identify a model yet (one window measured): the window is the cap, and the
    /// run is the measurement.
    Unmodelled,
    /// The fitted model PREDICTS that even the floor window at the floor chunk exceeds what is free.
    /// A prediction extrapolated from the measured windows, not a measured impossibility (Codex on
    /// #4972: the 5090's two measured windows were 67,584 and 66,304, far from the 2,048 floor), so
    /// the window is the bottom of the range, not a plan; a refusal measured at the exact shape is a
    /// separate, measured fact (card 030d70f0; 2026-10-11: window_calculated reported 2048 for 16
    /// examples none of which could train there).
    PredictedNoFit { predicted_at_floor: u64 },
}

/// The planned window and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPlan {
    pub window: u32,
    pub fit: WindowFit,
}

/// `rows` are whole-run footprints, what the model is fitted to. `bounds` are every measured row,
/// a graph-only refusal included: a graph alone over `available` proves the whole run is over it
/// too, so it caps the window whichever it included (card 030d70f0, where the 5090's graph-only
/// refusals were invisible to every plan read).
pub fn choose_window(
    served: u32,
    available: u64,
    rows: &[(u32, u32, u64)],
    bounds: &[(u32, u32, u64)],
) -> WindowPlan {
    // A MEASUREMENT IS A HARD BOUND (Cormac on #4939): a window measured at the smallest chunk
    // that did not fit what is free now cannot fit at it, nor at any larger window, whatever the
    // fit predicts. Without the cap a non-physical or single-chunk fit could plan the refused
    // window again, and the refusal ladder's return would loop. Each floor refusal files its row
    // and lowers the cap, so the ladder always moves down, and ends at the smallest window.
    let cap = bounds
        .iter()
        .filter(|&&(_, c, b)| c == TRAINING_CHUNK_MIN && b > available)
        .map(|&(w, _, _)| train_window(w.saturating_sub(256).max(TRAINING_WINDOW_MIN)))
        .min()
        .map_or(served, |cap| cap.min(served));
    let Some(model) = GraphModel::fit(rows) else {
        return WindowPlan { window: cap, fit: WindowFit::Unmodelled };
    };
    let mut window = cap;
    while window > TRAINING_WINDOW_MIN && model.bytes(window, TRAINING_CHUNK_MIN) > available {
        window = train_window(window.saturating_sub(256).max(TRAINING_WINDOW_MIN));
    }
    let at_window = model.bytes(window, TRAINING_CHUNK_MIN);
    let fit = if at_window > available {
        // the loop only stops above `available` at the floor: the model predicts no window fits
        WindowFit::PredictedNoFit { predicted_at_floor: at_window }
    } else {
        WindowFit::Fits
    };
    WindowPlan { window, fit }
}

/// The depths a plan may step down to from `from` (`None` = every one of the model's `layers`
/// blocks): each half the one before, down to one block, so the search is bounded by log2(layers)
/// rungs. A rung is a candidate, not a claim: it is chosen only by a plan, and the run there (or
/// its refusal) is its measurement.
pub fn shallower_depths(from: Option<u32>, layers: u32) -> Vec<u32> {
    let mut depth = from.unwrap_or(layers).min(layers); // unwrap_or: no depth asked IS every block (top_layers omitted, fork #27)
    let mut rungs = Vec::new();
    while depth > 1 {
        depth /= 2;
        rungs.push(depth);
    }
    rungs
}

/// THE DEPTH RUNG, between the chunk and the window (ONE-RESIDENT-MODEL: "depth, recompute, or
/// smaller chunks, never a smaller window"). `plan_at(depth)` is [`choose_window`] over that
/// depth's own rows, which already steps the chunk (a bound at the smallest chunk is what caps a
/// window). When the asked depth's plan holds `need` (the window that trains every example
/// whole), it stands. Otherwise the first shallower depth whose plan holds `need` is chosen, so
/// every example stays whole while depth is searched; an unmeasured depth plans `need` unmodelled
/// and its run is the measurement. Only when no depth holds it does the asked depth's smaller
/// window stand, and the partition parks what no longer fits. `layers` unknown (no run on this
/// engine has reported its block count yet) offers no rung.
pub fn choose_depth(
    asked: Option<u32>,
    layers: Option<u32>,
    need: u32,
    plan_at: impl Fn(Option<u32>) -> WindowPlan,
) -> (Option<u32>, WindowPlan) {
    let holds = |plan: &WindowPlan| plan.window >= need && !matches!(plan.fit, WindowFit::PredictedNoFit { .. });
    let at_asked = plan_at(asked);
    if holds(&at_asked) {
        return (asked, at_asked);
    }
    layers
        .into_iter()
        .flat_map(|n| shallower_depths(asked, n))
        .map(|depth| (Some(depth), plan_at(Some(depth))))
        .find(|(_, plan)| holds(plan))
        .unwrap_or((asked, at_asked)) // unwrap_or: no shallower depth holds it, so the asked depth and its smaller window stand
}

/// The window a run trains at: the engine's training context is n_ctx, which rounds up to a
/// multiple of 256, and a window that is not one reached opt_init's assert and took the
/// serving process down (fork #27 now refuses it). Rounded DOWN, so the lease never grows past
/// what was asked; at least one 256-token context.
pub fn train_window(sequence_length: u32) -> u32 {
    (sequence_length / 256).max(1) * 256
}

/// THE PARTITION of a job's examples at its window (fork #51's `/train/measure`, the engine's
/// own fit rule): which of the request's examples were sent, and how each one met the window.
/// Recorded in the job's own directory (`partition.json`) before the run, and amended at its end
/// with any the run itself skipped. An example PARKED here (skipped at the window: her system and
/// tool head plus her last exchange overflow it) is neither trained nor pending. It waits for a
/// larger window, so it never refills her bucket only to be skipped again every tick, and
/// `return` never hands it back. The 5090, 2026-10-11: at a 16,896-token window the engine kept
/// 54 of Kimi's 311 lived turns and skipped 257, which the run then counted as consumed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Partition {
    pub window: u32,
    /// request indices sent to the engine, in order: the run's own example indices index this
    pub sent: Vec<usize>,
    /// request indices parked, ascending: not whole at the window by the measure (cut or not
    /// held), or skipped by the run itself
    pub parked: Vec<usize>,
    /// every example's measurement, by request index
    pub rows: Vec<MeasuredExample>,
}

/// One example as `/train/measure` reports it (fork #51).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredExample {
    pub index: usize,
    /// rendered tokens before anything is dropped
    pub tokens_full: u64,
    /// the smallest window it trains in: the head plus her last exchange
    pub tokens_min: u64,
    /// tokens as it would train at this window (0 when skipped)
    pub tokens_trained: u64,
    pub fit: ExampleFit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExampleFit {
    Kept,
    Truncated,
    Skipped,
}

/// PURE: the partition a measurement at `window` implies. Only an example that trains WHOLE at
/// the window (`Kept`) is sent, in request order. One the window would cut (`Truncated`: her
/// history dropped to fit) or cannot hold at all (`Skipped`) is parked until a window that holds
/// it whole: she learns from whole experiences only, on every path (Codex on #4988; Joel: no
/// clipping).
pub fn partition_of(window: u32, rows: Vec<MeasuredExample>) -> Partition {
    let sent = rows.iter().filter(|r| r.fit == ExampleFit::Kept).map(|r| r.index).collect();
    let parked = rows.iter().filter(|r| r.fit != ExampleFit::Kept).map(|r| r.index).collect();
    Partition { window, sent, parked, rows }
}

/// PURE: `partition` amended with the examples the RUN skipped (its own indices, into `sent`):
/// the run is the authority on what trained, so one it skipped is parked even if the measure
/// said it fit. An index past `sent` is not one this core sent and is ignored.
pub fn with_run_skipped(mut partition: Partition, run_skipped: &[usize]) -> Partition {
    for &i in run_skipped {
        if let Some(&request_index) = partition.sent.get(i) {
            if !partition.parked.contains(&request_index) {
                partition.parked.push(request_index);
            }
        }
    }
    partition.parked.sort_unstable();
    partition
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a serving reserve or a chunk plan read off the fit alone, under a
    // refusal the engine already measured (card 030d70f0), or a lower bound reported as if a
    // whole run had measured it.
    #[test]
    fn a_need_is_the_larger_of_what_is_known_and_its_measured_bound() {
        let refused = 18_587_879_424;
        assert_eq!(need(None, Some(refused), Some(1 << 30)), Some(Need { bytes: refused, source: NeedSource::LowerBound }), "a refusal outranks an optimistic fit");
        assert_eq!(need(Some(3 << 30), Some(3 << 30), None), Some(Need { bytes: 3 << 30, source: NeedSource::WholeRun }), "a whole run is its own bound");
        assert_eq!(need(None, None, Some(5 << 30)), Some(Need { bytes: 5 << 30, source: NeedSource::Predicted }));
        assert_eq!(need(None, Some(1 << 30), Some(5 << 30)).map(|n| n.source), Some(NeedSource::Predicted), "a smaller bound adds nothing");
        assert_eq!(need(None, None, None), None, "nothing known: calibrate");
        // what this catches (Codex on #4985): a lower bound under what is free leased as the
        // need, or read by serving eligibility as proof the reserve suffices.
        let bound = Need { bytes: 3 << 30, source: NeedSource::LowerBound };
        assert_eq!(bound.decides(4 << 30), None, "under free, a bound proves nothing: calibrate");
        assert_eq!(bound.decides(2 << 30), Some(3 << 30), "over free, it disqualifies");
        assert_eq!(Need { bytes: 3 << 30, source: NeedSource::Predicted }.decides(4 << 30), Some(3 << 30));
        // what this catches (card de9e1342): a parked job that wakes on a bound it has not
        // cleared, or never wakes once capacity clears its bound.
        assert!(bound.rules_out(2 << 30), "a bound over free rules the shape out");
        assert!(!bound.rules_out(4 << 30), "a bound under free does not: the job may re-plan");
        assert!(Need { bytes: 5 << 30, source: NeedSource::Predicted }.rules_out(4 << 30));
    }

    // what this catches (Fable, 2026-10-11): the window planned as the largest memory allowed at a
    // fixed chunk, so her long turns were skipped while a smaller chunk could have held them. The
    // window is what trains her examples WHOLE (the largest tokens_full, rounded up to 256),
    // capped at the served window and floored at the smallest servable one.
    #[test]
    fn the_window_is_what_the_examples_need() {
        let row = |tokens_full| MeasuredExample { index: 0, tokens_full, tokens_min: 64, tokens_trained: 0, fit: ExampleFit::Kept };
        assert_eq!(window_for_need(&[row(15_000), row(20_433), row(9_707)], 68_608), 20_480, "the longest example whole, rounded up to 256");
        assert_eq!(window_for_need(&[row(42_456)], 16_896), 16_896, "never above the served window");
        assert_eq!(window_for_need(&[row(64)], 68_608), TRAINING_WINDOW_MIN, "never below the smallest servable window");
        assert_eq!(window_for_need(&[row(20_481)], 68_608), 20_480, "tokens_min - 1 fits a window of exactly 20,480");
    }

    // what this catches (Cormac on #4940): a second ~5 s render per example to partition at a
    // window smaller than the one measured. Under "middle" the fit is arithmetic on the measured
    // lengths: whole, cut to its head and last exchange, or skipped, as the engine would decide.
    #[test]
    fn a_fit_at_a_smaller_window_is_arithmetic_on_the_measured_lengths() {
        let row = |index, full, min| MeasuredExample { index, tokens_full: full, tokens_min: min, tokens_trained: full, fit: ExampleFit::Kept };
        let refit = refit_middle(vec![row(0, 10_000, 4_000), row(1, 30_000, 8_000), row(2, 30_000, 25_000), row(3, 20_481, 9_000)], 20_480);
        let fits: Vec<ExampleFit> = refit.iter().map(|r| r.fit).collect();
        assert_eq!(fits, vec![ExampleFit::Kept, ExampleFit::Truncated, ExampleFit::Skipped, ExampleFit::Kept], "20,481 tokens fit a 20,480 window (window + 1)");
        assert_eq!(refit[1].tokens_trained, 8_000, "a truncated one reports its smallest length");
        assert_eq!(refit[2].tokens_trained, 0);
    }

    // what this catches (Astra and Cormac on #4949): a calibration floor taken from rows that bound
    // nothing for the candidate. Only a row at a window AND chunk no larger than the candidate's is a
    // lower bound, and the floor is the largest of those; a larger window's row would keep a smaller
    // candidate from ever calibrating.
    #[test]
    fn a_calibration_floor_comes_only_from_rows_no_larger_than_the_candidate() {
        let rows = [(16_896, 64, 3_000), (16_896, 256, 9_000), (67_584, 64, 20_000)];
        assert_eq!(calibration_floor(&rows, 40_960, 64), Some(3_000), "the w16896 c64 row bounds a w40960 c64 run; w67584's does not");
        assert_eq!(calibration_floor(&rows, 40_960, 256), Some(9_000), "the largest of the rows no larger than it");
        assert_eq!(calibration_floor(&rows, 8_192, 64), None, "every row is at a larger window: nothing bounds it");
        assert_eq!(calibration_floor(&[], 67_584, 64), None);
    }

    // what this catches (the 5090, 2026-10-11 14:02-14:08Z, jobs ef43162c, b96af9d4, ab3f2874): the
    // exact walk refused at window 67,584 with the same 18,198 MiB graph at chunks 512, 256 and 128,
    // so stepping the chunk down could never fit it. A measurement at or under the needed window
    // that exceeded what is free rules it out (the plain walk then trains her turns whole); one at
    // a larger window, or one that fit, does not.
    #[test]
    fn the_exact_walk_is_ruled_out_by_a_measurement_at_or_under_the_window() {
        let gib = 1024u64 * 1024 * 1024;
        let measured = [(67_584, 512, 20 * gib), (67_584, 128, 20 * gib)];
        assert!(exact_ruled_out(&measured, 67_584, 4 * gib), "measured at the needed window, over what is free");
        assert!(exact_ruled_out(&[(20_000, 64, 6 * gib)], 48_000, 4 * gib), "over at a smaller window: a larger one is over too");
        assert!(!exact_ruled_out(&[(67_584, 512, 20 * gib)], 16_896, 4 * gib), "over at a LARGER window says nothing about a smaller one");
        assert!(!exact_ruled_out(&[(1_536, 256, gib)], 67_584, 4 * gib), "a run that fit rules nothing out");
        assert!(!exact_ruled_out(&[], 67_584, 4 * gib), "nothing measured: the exact walk calibrates");
    }

    // what this catches (Fable, 2026-10-11): the KV cache grows with the window, not the chunk.
    // A two-term model folds it into the window term and scales it down with the chunk, so at
    // chunk 64 it promised a window that does not fit. With rows across two chunks it fits the
    // third term and predicts an unmeasured small-chunk graph; with rows at one chunk it cannot,
    // and keeps the two-term fit.
    #[test]
    fn the_graph_model_separates_the_kv_cache_from_the_chunk_once_two_chunks_are_measured() {
        let truth = GraphModel { per_pos: 35_000.0, per_token: 9.6e6, per_token_per_pos: 380.0 };
        let row = |w: u32, c: u32| (w, c, truth.bytes(w, c));
        let rows = [row(1_536, 256), row(68_608, 512), row(16_896, 256)];
        let fitted = GraphModel::fit(&rows).expect("test: fits");
        let (want, got) = (truth.bytes(40_960, 64) as f64, fitted.bytes(40_960, 64) as f64);
        assert!((got - want).abs() / want < 0.01, "a chunk-64 graph predicted within 1%: {got} vs {want}");
        assert!(fitted.per_pos > 0.0, "the KV term is identified");
        let one_chunk = [row(1_536, 256), row(68_608, 256)];
        assert_eq!(GraphModel::fit(&one_chunk).map(|m| m.per_pos), Some(0.0), "one chunk cannot separate it: the two-term fit stands");
        assert_eq!(train_chunk_for(68_608, 64), 64);
        assert_eq!(train_chunk_for(68_608, 255), 128, "under 256: 128 or 64, never 192");
        assert_eq!(train_chunk_for(68_608, 512), 512, "256 and up: the old divisor rule, so old keys hold");
    }

    // what this catches (card 030d70f0; the 5090, 2026-10-11): the floor window reported as a plan
    // when the model PREDICTS nothing fits (extrapolated from two measured windows, not measured at
    // the floor). These are that night's real plain-walk rows (whole-run bytes): four chunks
    // refused at 67,584 and one at 66,304, the graph flat across the two windows, against 4.55 GB
    // governed free. By the model, nothing fits at any window, so the floor is the bottom of the range, not a plan.
    // One window measured cannot identify a model, so it never claims nothing fits.
    #[test]
    fn the_window_plan_names_the_no_fit_case_on_the_measured_rows() {
        let rows = [
            (67_584, 512, 19_531_543_168),
            (67_584, 256, 15_327_436_416),
            (67_584, 128, 14_567_221_888),
            (67_584, 64, 13_587_430_016),
            (66_304, 256, 15_282_871_936),
        ];
        let free = 4_552_916_992;
        let plan = choose_window(67_584, free, &rows, &rows);
        assert_eq!(plan.window, TRAINING_WINDOW_MIN, "the window bottoms out at the floor");
        let WindowFit::PredictedNoFit { predicted_at_floor } = plan.fit else {
            panic!("the 5090's rows: the model predicts no window fits beside serving, got {:?}", plan.fit);
        };
        assert!(predicted_at_floor > free, "the prediction at the floor exceeds what is free: {predicted_at_floor}");
        assert_eq!(choose_window(67_584, free, &rows[..4], &rows[..4]).fit, WindowFit::Unmodelled, "one window measured: no model, no claim");
        assert_eq!(choose_window(67_584, u64::MAX, &rows, &rows).fit, WindowFit::Fits, "with room for it, the window fits");
    }

    // what this catches (card d6dae498, and the 5090 2026-10-10 job 00431d4d): the chunk the core
    // keys under differing from the chunk the engine runs; a window planned that the measured
    // graph cannot hold at the floor chunk; and a chunk ladder that never shrinks when the
    // measured graph does not fit what is free.
    #[test]
    fn the_chunk_is_the_engines_rule_and_the_window_and_its_chunk_come_from_one_model() {
        // the engine's rule, mirrored: the largest multiple of 256 dividing the window
        assert_eq!(train_chunk_for(1536, 512), 512);
        assert_eq!(train_chunk_for(1536, 256), 256);
        assert_eq!(train_chunk_for(1280, 512), 256, "1280 = 5 x 256: 512 does not divide it");
        assert_eq!(train_chunk_for(66_560, 512), 512);
        assert_eq!(train_chunk_for(256, 512), 256);
        // what this catches (the 5090, 2026-10-10, job 00431d4d): a lived 68k-window batch was
        // dispatched only to be refused, 7.4 GB over. With two windows of a shape measured, the
        // window is CALCULATED before dispatch: the largest that fits at the floor chunk, never
        // above served, never under the floor; with fewer, served (the run is the measurement).
        let mib = 1024u64 * 1024;
        // three rows across two chunks, so the KV term is identified and the floor chunk's graph
        // is predicted, not extrapolated (with rows at one chunk the two-term fit is optimistic
        // under it, and the engine's preflight refusal files the row that corrects it)
        let truth = GraphModel { per_pos: 35_000.0, per_token: 9.6e6, per_token_per_pos: 380.0 };
        let rows = [(1536, 256, truth.bytes(1536, 256)), (68608, 512, truth.bytes(68608, 512)), (16896, 256, truth.bytes(16896, 256))];
        let model = GraphModel::fit(&rows).expect("test: rows fit");
        assert!(model.per_token_per_pos > 0.0 && model.per_pos > 0.0, "the window and KV terms are real");
        let free = 4142 * mib;
        let w = choose_window(68608, free, &rows, &rows).window;
        assert!(w % 256 == 0 && w < 68608 && w >= TRAINING_WINDOW_MIN, "a smaller 256-multiple window: {w}");
        assert!(model.bytes(w, TRAINING_CHUNK_MIN) <= free, "that fits");
        assert!(model.bytes(w + 256, TRAINING_CHUNK_MIN) > free || w == 68608, "and is the largest that does");
        assert_eq!(choose_window(68608, free, &rows[..1], &rows[..1]).window, 68608, "one window measured: nothing to calculate, served");
        // and the chunk at that window comes from the same model, never the default 512 that the
        // window was not sized for (Cormac on #4920)
        let (c, _) = choose_chunk(w, free, |c| Some(model.bytes(w, c)));
        assert!(model.bytes(w, c) <= free, "the chunk at the calculated window fits too: {c}");
        assert_eq!(choose_window(1536, free, &rows, &rows).window, 1536, "never above served");
        assert_eq!(choose_window(68608, 1, &rows, &rows).window, TRAINING_WINDOW_MIN, "never under the floor");
        // the chunk: unmeasured calibrates at the default; a measurement that fits leases;
        // one that does not fit halves the chunk and asks that shape; the smallest stands
        let gib = 1024u64 * 1024 * 1024;
        assert_eq!(choose_chunk(1536, 6 * gib, |_| None), (512, None), "never measured: calibrate at the default");
        assert_eq!(choose_chunk(1536, 6 * gib, |c| (c == 512).then_some(3 * gib)), (512, Some(3 * gib)));
        let measured = |c: u32| match c { 512 => Some(12 * gib), 256 => Some(5 * gib), _ => None };
        assert_eq!(choose_chunk(1536, 6 * gib, measured), (256, Some(5 * gib)), "the 5090: 12.6 GB at 512 does not fit 6.2 GB free; 256 does");
        assert_eq!(choose_chunk(1536, 6 * gib, |c| (c == 512).then_some(12 * gib)), (256, None), "512 too big, 256 never measured: calibrate at 256");
        assert_eq!(choose_chunk(1536, gib, measured), (128, None), "512 and 256 too big, 128 never measured: calibrate at 128 (fork #52)");
        let every = |c: u32| match c { 512 => Some(12 * gib), 256 => Some(5 * gib), 128 => Some(3 * gib), 64 => Some(2 * gib), _ => None };
        assert_eq!(choose_chunk(1536, gib, every), (64, Some(2 * gib)), "nothing fits: the smallest chunk's number stands and the governor waits");
        assert_eq!(choose_chunk(256, gib, |_| Some(5 * gib)), (64, Some(5 * gib)), "a 256 window steps down to 64 too");
    }

    // what this catches: Kimi's job 598e183c (the 5090 windows box, 2026-10-28): refused at window
    // 45,056 with only a smaller window left on the ladder, which parks her longest lived turns.
    // A shallower depth that holds the window her examples need is chosen first, every example
    // whole; a depth is chosen by its plan, never assumed to fit; and the search is bounded.
    #[test]
    fn a_shallower_depth_holds_her_whole_examples_before_any_window_shrinks() {
        assert_eq!(shallower_depths(None, 64), vec![32, 16, 8, 4, 2, 1], "halving to one block: log2 rungs");
        assert_eq!(shallower_depths(Some(8), 64), vec![4, 2, 1], "from an asked depth");
        assert_eq!(shallower_depths(Some(1), 64), Vec::<u32>::new(), "one block has no rung below it");
        assert_eq!(shallower_depths(Some(99), 4), vec![2, 1], "an asked depth past the model is the model");

        let need = 45_312;
        let plan = |window: u32, fit: WindowFit| WindowPlan { window, fit };
        // full depth measured not to hold `need` at the smallest chunk; 32 not either; 16 unmeasured
        let refused_until_16 = |depth: Option<u32>| match depth {
            None => plan(30_720, WindowFit::Fits),
            Some(32) => plan(40_960, WindowFit::Fits),
            Some(_) => plan(need, WindowFit::Unmodelled),
        };
        assert_eq!(choose_depth(None, Some(64), need, refused_until_16), (Some(16), plan(need, WindowFit::Unmodelled)), "the first depth that holds her whole examples; its run measures it");
        let holds_at_full = |_: Option<u32>| plan(need, WindowFit::Fits);
        assert_eq!(choose_depth(None, Some(64), need, holds_at_full).0, None, "a depth that holds stands: no step");
        let predicted_no_fit = |depth: Option<u32>| match depth {
            None => plan(30_720, WindowFit::Fits),
            Some(_) => plan(need, WindowFit::PredictedNoFit { predicted_at_floor: 1 }),
        };
        assert_eq!(choose_depth(None, Some(64), need, predicted_no_fit), (None, plan(30_720, WindowFit::Fits)), "no depth holds it: the asked depth's smaller window stands");
        assert_eq!(choose_depth(None, None, need, refused_until_16), (None, plan(30_720, WindowFit::Fits)), "the block count unreported: no rung is offered");
        let asked = std::cell::Cell::new(0);
        let counting = |depth: Option<u32>| {
            asked.set(asked.get() + 1);
            predicted_no_fit(depth)
        };
        choose_depth(None, Some(64), need, counting);
        assert_eq!(asked.get(), 7, "bounded: the asked depth and six rungs, then the window");
    }
}
