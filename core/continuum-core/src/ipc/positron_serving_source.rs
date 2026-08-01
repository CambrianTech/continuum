//! Serving glass-box emitter — the SERVING panel's source (#141 first slice:
//! the beat-WASTE control loop made visible).
//!
//! Own task + `tokio::time::interval` + a store into the served [`Substrate`]
//! — the same emit shape as [`crate::ipc::positron_metrics_source`]. Two
//! feeds, both already owned by the process:
//!
//! 1. **Header** — [`crate::inference::llama_server::current_serving`], the
//!    daemon-published snapshot (model / ready / lanes / ctx / degraded).
//!    Zero new plumbing: the accessor is installed at daemon boot.
//! 2. **Pager telemetry** — the `GGML_MOE_CAPTURE_FILE` JSONL the serving
//!    binary appends one line per decode token ("Positron tails it" — the
//!    comment in ggml-backend.cpp names this exact consumer). Offset-tail
//!    with truncation-reset, the same discipline as `moe-pager-driver`'s
//!    trace tail. Raw C++ lines carry only the perf fields; the decision
//!    fields (arms, decay) join when the Rust controller feed is live —
//!    `PagerCaptureEvent`'s `serde(default)`s make one decoder serve both.
//!
//! Honest absence throughout: no capture env → header-only view (a plain
//! serving-health widget); no snapshot yet → `header: None`. Never a
//! fabricated gauge ([[fallbacks-are-illegal-fail-loud]]).

use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Duration;

use continuum_positron::serving::{ServingArmView, ServingEventCard, ServingHeaderView, ServingViewState};
use continuum_positron::system_metrics::MetricSeriesView;
use continuum_positron::{StateBuilder, Substrate};

use crate::capacity::pager_capture::PagerCaptureEvent;

/// Emit cadence — matches the SYS gauge so the rail animates in one rhythm.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Ring length for the time-series (per-token samples, newest last). Bounded
/// per the perception resolution contract's byte-bound rule.
const WINDOW: usize = 120;

/// Bounded event-card ring.
const EVENT_WINDOW: usize = 12;

/// The env var the serving binary writes its per-token capture JSONL under.
/// Absent → the pager half of the widget is honestly absent.
const CAPTURE_FILE_ENV: &str = "GGML_MOE_CAPTURE_FILE";

fn push_ring(ring: &mut Vec<f32>, v: f32) {
    if ring.len() == WINDOW {
        ring.remove(0);
    }
    ring.push(v);
}

fn push_event(ring: &mut Vec<ServingEventCard>, card: ServingEventCard) {
    if ring.len() == EVENT_WINDOW {
        ring.remove(0);
    }
    ring.push(card);
}

/// Project a raw-valued ring onto the 0–100 sparkline scale by its own
/// running max — shape without unit math; the legend `current` carries the
/// real reading (source-formatted, per the MetricSeriesView contract).
fn normalized(ring: &[f32]) -> Vec<f32> {
    let max = ring.iter().cloned().fold(0.0_f32, f32::max);
    if max <= 0.0 {
        return vec![0.0; ring.len()];
    }
    ring.iter().map(|v| (v / max * 100.0).clamp(0.0, 100.0)).collect()
}

/// The pager half's fold state — rings + last-seen policy signals.
#[derive(Default)]
struct PagerFold {
    hit_ring: Vec<f32>,
    tok_ring: Vec<f32>,
    fetch_ring: Vec<f32>,
    events: Vec<ServingEventCard>,
    arms: Vec<ServingArmView>,
    last_decay: Option<f32>,
    last_resident: Option<u32>,
    last: Option<PagerCaptureEvent>,
}

impl PagerFold {
    /// Fold one capture frame in — rings, arm beliefs, and event cards for
    /// the discrete moments (serve start, decay switch, residency shift).
    fn apply(&mut self, ev: PagerCaptureEvent) {
        push_ring(&mut self.hit_ring, (ev.hit_rate * 100.0).clamp(0.0, 100.0));
        push_ring(&mut self.tok_ring, ev.tok_per_s.max(0.0));
        push_ring(&mut self.fetch_ring, ev.fetch_mb_s.max(0.0));

        if !ev.per_arm_reward.is_empty() {
            // Labels are the leaf crate's decay ladder when lengths agree —
            // data-driven, no invented arm names on mismatch.
            let ladder = expert_pager_policy::DECAY_ARMS;
            self.arms = ev
                .per_arm_reward
                .iter()
                .enumerate()
                .map(|(i, &reward)| ServingArmView {
                    label: ladder
                        .get(i)
                        .map(|d| format!("{d:.2}"))
                        .unwrap_or_else(|| format!("arm{i}")),
                    reward,
                    chosen: ladder
                        .get(i)
                        .is_some_and(|d| (*d as f32 - ev.chosen_decay).abs() < 1e-3),
                })
                .collect();
        }

        match self.last_decay {
            Some(prev) if (prev - ev.chosen_decay).abs() > 1e-3 => {
                push_event(
                    &mut self.events,
                    ServingEventCard {
                        at_token: ev.token,
                        kind: "decay-switch".into(),
                        detail: format!("bandit switched decay {prev:.2} → {:.2}", ev.chosen_decay),
                    },
                );
            }
            _ => {}
        }
        self.last_decay = Some(ev.chosen_decay);

        if let Some(prev) = self.last_resident {
            let delta = ev.resident_experts.abs_diff(prev);
            // A shift of >12.5% of the resident set in one token is a
            // structural residency event (page-in burst / eviction wave),
            // not sample noise.
            if delta > prev.max(8) / 8 {
                push_event(
                    &mut self.events,
                    ServingEventCard {
                        at_token: ev.token,
                        kind: "residency-shift".into(),
                        detail: format!("resident experts {prev} → {}", ev.resident_experts),
                    },
                );
            }
        }
        self.last_resident = Some(ev.resident_experts);
        self.last = Some(ev);
    }

    fn series(&self) -> Vec<MetricSeriesView> {
        let Some(last) = self.last.as_ref() else {
            return Vec::new();
        };
        vec![
            MetricSeriesView {
                label: "hit".into(),
                points: self.hit_ring.clone(),
                current: format!("{:.0}%", last.hit_rate * 100.0),
            },
            MetricSeriesView {
                label: "tok/s".into(),
                points: normalized(&self.tok_ring),
                current: format!("{:.2}", last.tok_per_s),
            },
            MetricSeriesView {
                label: "fetch".into(),
                points: normalized(&self.fetch_ring),
                current: format!("{:.0}MB/s", last.fetch_mb_s),
            },
        ]
    }
}

/// Offset-tailer for the capture JSONL: reads whole new lines, resets on
/// truncation (a new serve) and reports the reset so the fold can card it.
struct CaptureTail {
    path: PathBuf,
    /// Consumed offset — complete lines only; a torn tail stays unconsumed.
    offset: u64,
    /// Raw file length at last poll. Truncation detection compares against
    /// THIS, not the consumed offset — a new serve's file can grow past the
    /// old consumed offset between polls while still being shorter than the
    /// old raw length (caught by the tail test that motivated this field).
    last_len: u64,
}

impl CaptureTail {
    /// Read newly appended complete lines. Returns `(events, reset)` —
    /// `reset` true when the file shrank (new serve started).
    fn poll(&mut self) -> (Vec<PagerCaptureEvent>, bool) {
        let Ok(file) = std::fs::File::open(&self.path) else {
            return (Vec::new(), false);
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let reset = len < self.last_len;
        self.last_len = len;
        if reset {
            self.offset = 0;
        }
        if len == self.offset {
            return (Vec::new(), reset);
        }
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return (Vec::new(), reset);
        }
        let mut events = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(n) => {
                    // Only consume COMPLETE lines — a torn tail (no newline)
                    // stays for the next poll, same guard as the trace tail.
                    if !line.ends_with('\n') {
                        break;
                    }
                    self.offset += n as u64;
                    match serde_json::from_str::<PagerCaptureEvent>(line.trim()) {
                        Ok(ev) => events.push(ev),
                        Err(e) => {
                            tracing::warn!(target: "positron.serving", "capture line unparsable: {e}");
                        }
                    }
                }
                Err(_) => break,
            }
        }
        (events, reset)
    }
}

/// Spawn the emitter: every tick fold the serving snapshot + any new capture
/// frames and store the view under `kind="serving"`. Runs for the process
/// lifetime.
pub fn spawn_serving_emitter(rt: &tokio::runtime::Handle, substrate: Substrate) {
    rt.spawn(async move {
        // Sole writer of the "serving" kind → its own standalone Revisions well.
        let builder = StateBuilder::standalone();
        let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
        let mut fold = PagerFold::default();
        // Capture feed location is serve-side config; read once — the env of
        // a running core doesn't change. Absent = header-only widget.
        let mut tail = std::env::var(CAPTURE_FILE_ENV)
            .ok()
            .map(|p| CaptureTail { path: PathBuf::from(p), offset: 0, last_len: 0 });
        loop {
            ticker.tick().await;

            if let Some(t) = tail.as_mut() {
                let (events, reset) = t.poll();
                if reset {
                    let at = fold.last.as_ref().map(|e| e.token).unwrap_or(0);
                    fold = PagerFold::default();
                    push_event(
                        &mut fold.events,
                        ServingEventCard {
                            at_token: at,
                            kind: "serve-start".into(),
                            detail: "capture reset — new serve".into(),
                        },
                    );
                }
                for ev in events {
                    fold.apply(ev);
                }
            }

            let snap = crate::inference::llama_server::current_serving();
            let header = ServingHeaderView {
                model: snap.active_model.clone(),
                ready: snap.ready,
                lanes: snap.lanes,
                context_window: snap.served_context_window,
                degraded_reason: snap.degraded_reason.clone(),
            };

            let view = ServingViewState {
                header: Some(header),
                series: fold.series(),
                arms: fold.arms.clone(),
                events: fold.events.clone(),
                sample_interval_ms: SAMPLE_INTERVAL.as_millis() as u64,
            };
            substrate.store(builder.session(view));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(token: u64, hit: f32, decay: f32, resident: u32) -> PagerCaptureEvent {
        serde_json::from_str(&format!(
            r#"{{"token":{token},"hit_rate":{hit},"fault_wait_ms":1.0,"tok_per_s":0.5,"bytes_fetched_mb":1485,"fetch_mb_s":2458,"resident_experts":{resident},"chosen_decay":{decay},"per_arm_reward":[0.1,0.2,0.3,0.2,0.1,0.05]}}"#
        ))
        .expect("frame json")
    }

    // what this catches: the fold's event-card rules — a decay switch and a
    // >1/8 residency shift each produce ONE card, steady frames produce none,
    // and the chosen arm is marked from the leaf crate's ladder (data-driven
    // labels, never invented).
    #[test]
    fn fold_cards_switches_and_shifts_only() {
        let mut fold = PagerFold::default();
        fold.apply(frame(1, 0.5, 0.99, 4416));
        fold.apply(frame(2, 0.55, 0.99, 4420)); // steady — no cards
        assert!(fold.events.is_empty(), "steady frames must not card");
        fold.apply(frame(3, 0.6, 0.30, 4420)); // decay switch
        fold.apply(frame(4, 0.6, 0.30, 2208)); // residency halved
        let kinds: Vec<&str> = fold.events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["decay-switch", "residency-shift"]);
        let chosen: Vec<bool> = fold.arms.iter().map(|a| a.chosen).collect();
        assert_eq!(chosen.iter().filter(|c| **c).count(), 1, "exactly one chosen arm");
        assert!(fold.arms.iter().any(|a| a.label == "0.30" && a.chosen));
    }

    // what this catches: the raw C++ perf-only line (no decision fields)
    // still folds — series present, arms honestly absent — the serde(default)
    // contract exercised at the fold level, not just decode.
    #[test]
    fn raw_perf_only_feed_folds_without_arms() {
        let mut fold = PagerFold::default();
        let raw: PagerCaptureEvent = serde_json::from_str(
            r#"{"token":0,"hit_rate":0.62,"fault_wait_ms":8178,"tok_per_s":0.33,"bytes_fetched_mb":4000,"fetch_mb_s":2458,"resident_experts":8037}"#,
        )
        .expect("raw line");
        fold.apply(raw);
        let series = fold.series();
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].current, "62%");
        assert!(fold.arms.is_empty(), "no decision feed → no fabricated arms");
    }

    // what this catches: the tailer consumes only COMPLETE lines (torn tail
    // stays), and truncation reports a reset instead of replaying stale
    // offsets into the new serve's file.
    #[test]
    fn tail_reads_whole_lines_and_reports_truncation() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("capture.jsonl");
        std::fs::write(
            &path,
            "{\"token\":0,\"hit_rate\":0.5,\"fault_wait_ms\":1,\"tok_per_s\":0.4,\"bytes_fetched_mb\":10,\"fetch_mb_s\":100,\"resident_experts\":100}\n{\"token\":1,\"hit_rate\":0.6,\"fault_wait",
        )
        .expect("write");
        let mut tail = CaptureTail { path: path.clone(), offset: 0, last_len: 0 };
        let (events, reset) = tail.poll();
        assert!(!reset);
        assert_eq!(events.len(), 1, "torn second line must NOT be consumed");
        assert_eq!(events[0].token, 0);

        // New serve truncates the file → reset reported, new content read.
        std::fs::write(
            &path,
            "{\"token\":0,\"hit_rate\":0.7,\"fault_wait_ms\":1,\"tok_per_s\":0.5,\"bytes_fetched_mb\":10,\"fetch_mb_s\":100,\"resident_experts\":100}\n",
        )
        .expect("rewrite");
        let (events, reset) = tail.poll();
        assert!(reset, "shrunk file must report a reset");
        assert_eq!(events.len(), 1);
        assert!((events[0].hit_rate - 0.7).abs() < 1e-6);
    }
}
