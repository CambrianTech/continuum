//! One line per resident: the replay's prompts beside the captured ones, so the harness's
//! fidelity is read at a glance. Fresh characters (a prompt less what it shares from its start
//! with the one before) are the fresh-token proxy: the engine reuses only the shared prefix.

use super::decode::Resident;
use super::step::ActSample;

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0usize), |(sum, count), value| (sum + value, count + 1));
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn median(mut values: Vec<usize>) -> usize {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0) // unwrap_or: no act has no median
}

/// Shared characters over rendered characters, per act, averaged.
fn shared_fraction(samples: &[ActSample], pick: impl Fn(&ActSample) -> (usize, usize)) -> f64 {
    mean(samples.iter().skip(1).map(|sample| {
        let (shared, whole) = pick(sample);
        shared as f64 / whole.max(1) as f64
    }))
}

pub fn line(resident: &Resident, name: &str, samples: &[ActSample]) -> String {
    let refused = samples.iter().filter(|sample| sample.refused.is_some()).count();
    let served = resident.acts.iter().filter(|act| act.served).count();
    let turnovers = samples.iter().filter(|sample| sample.turned_over).count();
    let rendered = median(samples.iter().map(|sample| sample.rendered_chars).collect());
    let rendered_mean = mean(samples.iter().map(|sample| sample.rendered_chars as f64));
    let shared = shared_fraction(samples, |sample| (sample.shared_chars, sample.rendered_chars));
    let fresh = mean(samples.iter().map(|sample| sample.rendered_chars.saturating_sub(sample.shared_chars) as f64));
    let captured_median = median(samples.iter().map(|sample| sample.captured_chars).collect());
    let captured_shared = shared_fraction(samples, |sample| (sample.captured_shared_chars, sample.captured_chars));
    let captured_fresh = mean(samples.iter().map(|sample| sample.captured_chars.saturating_sub(sample.captured_shared_chars) as f64));
    let grant = |pick: fn(&ActSample) -> u64| mean(samples.iter().map(|sample| pick(sample) as f64));
    format!(
        "{name} ({persona}): acts {acts} (served live {served}, refused by replay {refused}, undecodable {undecodable}), \
         turnovers {turnovers}; rendered chars median {rendered} mean {rendered_mean:.0}, prefix shared {shared:.1}%, \
         fresh chars mean {fresh:.0} | captured: median {captured_median}, prefix shared {captured_shared:.1}%, fresh chars mean {captured_fresh:.0} \
         | grants mean (tokens): view {view:.0} history {history:.0} tools {tools:.0} framing {framing:.0} reply {reply:.0}",
        persona = resident.persona,
        acts = samples.len(),
        undecodable = resident.undecodable,
        shared = shared * 100.0,
        captured_shared = captured_shared * 100.0,
        view = grant(|s: &ActSample| s.grants.view),
        history = grant(|s: &ActSample| s.grants.history),
        tools = grant(|s: &ActSample| s.grants.tools),
        framing = grant(|s: &ActSample| s.grants.framing),
        reply = grant(|s: &ActSample| s.grants.reply),
    )
}

/// One line per act, for `FACULTY_REPLAY_ACTS=1`.
pub fn act_line(index: usize, sample: &ActSample) -> String {
    format!(
        "  act {index}: rendered {} shared {} fresh {} | captured {} shared {} fresh {} | turnover {} refused {} | grants {:?}",
        sample.rendered_chars,
        sample.shared_chars,
        sample.rendered_chars.saturating_sub(sample.shared_chars),
        sample.captured_chars,
        sample.captured_shared_chars,
        sample.captured_chars.saturating_sub(sample.captured_shared_chars),
        sample.turned_over,
        sample.refused.unwrap_or("-"), // unwrap_or: sent
        sample.grants,
    )
}
