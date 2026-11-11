use super::*;

fn act(input: u32, fresh: u32, output: u32, decode_per_token: impl Fn(f64) -> f64, prefill_per_token: impl Fn(f64) -> f64) -> ActSample {
    let cached = input - fresh;
    ActSample {
        input,
        cached,
        fresh,
        prefill_ms: prefill_per_token(f64::from(cached) + f64::from(fresh) / 2.0) * f64::from(fresh) * 1_000.0,
        output,
        decode_ms: decode_per_token(f64::from(input) + f64::from(output) / 2.0) * f64::from(output) * 1_000.0,
    }
}

// what this catches: a fit that does not recover the lines it was fed (a sign or weighting error in
// the running co-moments), or a marginal cost that is not the derivative of the act's seconds.
#[test]
fn the_fit_recovers_known_lines_and_its_marginal_is_the_derivative() {
    let decode = |c: f64| 0.05 + 2e-6 * c;
    let prefill = |c: f64| 0.004 + 1e-7 * c;
    let mut fit = ActTimeFit::default();
    for input in [10_000u32, 20_000, 35_000, 50_000, 75_000] {
        fit.observe(&act(input, 1_000 + input / 10, 400, decode, prefill));
    }
    let ContextPrice::Measured(time) = fit.price() else { panic!("five acts over a spread fit") };
    assert!((time.decode.slope - 2e-6).abs() < 1e-12 && (time.decode.intercept - 0.05).abs() < 1e-9, "{time:?}");
    assert!((time.prefill.slope - 1e-7).abs() < 1e-13 && (time.prefill.intercept - 0.004).abs() < 1e-9, "{time:?}");
    assert!((time.fresh.slope - 0.1).abs() < 1e-9 && (time.fresh.intercept - 1_000.0).abs() < 1e-6, "{time:?}");
    assert!(time.seconds_per_token_se(30_000.0) < 1e-9, "exact lines leave no residual: {time:?}");
    for context in [12_000.0, 30_000.0, 70_000.0] {
        let step = 1.0;
        let numeric = (time.seconds(context + step) - time.seconds(context - step)) / (2.0 * step);
        assert!((time.seconds_per_token(context) - numeric).abs() < 1e-9 * numeric.abs().max(1.0), "{context}: {} vs {numeric}", time.seconds_per_token(context));
    }
    // The composed curve reproduces each act's engine seconds where the lines are exact.
    let sample = act(35_000, 4_500, 400, decode, prefill);
    assert!((time.seconds(35_000.0) - (sample.decode_ms + sample.prefill_ms) / 1_000.0).abs() < 1e-6);
}

// what this catches: a price minted from too few acts or from acts all at one context (the slope
// is undetermined there), or a slope inside its own error: unmeasured is a type, never a number.
#[test]
fn too_few_acts_no_spread_or_noise_is_unmeasured() {
    let flat = |_: f64| 0.05;
    let mut fit = ActTimeFit::default();
    fit.observe(&act(10_000, 1_000, 400, flat, flat));
    fit.observe(&act(20_000, 1_000, 400, flat, flat));
    assert_eq!(fit.price(), ContextPrice::Unmeasured, "two acts fit no line with an error");
    let mut same = ActTimeFit::default();
    for _ in 0..5 {
        same.observe(&act(30_000, 1_000, 400, flat, flat));
    }
    assert_eq!(same.price(), ContextPrice::Unmeasured, "no spread in context, no slope");
    // Four acts whose seconds wander with no trend in context: a slope inside its own error.
    let mut noisy = ActTimeFit::default();
    for (input, per_token) in [(10_000u32, 0.10), (20_000, 0.05), (30_000, 0.15), (40_000, 0.08)] {
        noisy.observe(&act(input, 1_000, 400, |_| per_token, |_| per_token / 10.0));
    }
    assert_eq!(noisy.price(), ContextPrice::Unmeasured, "noise is never a price");
}

// what this catches: a fit whose fresh line is still empty (a field added after her acts were
// measured) refusing to price at all, which left her view unpriced and filling the window (Aris,
// the M5, 2026-11-11); priced cold instead, every held token is charged as prefilled.
#[test]
fn an_empty_fresh_line_prices_every_held_token_as_fresh() {
    let decode = |c: f64| 0.05 + 2e-6 * c;
    let prefill = |c: f64| 0.004 + 1e-7 * c;
    let mut fit = ActTimeFit::default();
    for input in [10_000u32, 20_000, 35_000, 50_000, 75_000] {
        fit.observe(&act(input, 1_000 + input / 10, 400, decode, prefill));
    }
    fit.fresh = Moments::default();
    let ContextPrice::Measured(time) = fit.price() else { panic!("decode and prefill measured: priced, never unmeasured") };
    assert_eq!(time.fresh_at(30_000.0), 30_000.0, "cold: every held token is fresh");
    assert!(time.seconds_per_token(30_000.0) > 0.0);
}

// what this catches (the M5, 2026-11-11): a fresh line whose cold and warm acts leave its slope
// noise (here falling with context) made the price Unmeasured, so her view went unpriced and filled
// her window; it prices cold instead, every held token charged as prefilled.
#[test]
fn a_fresh_line_that_cannot_decide_prices_cold() {
    let decode = |c: f64| 0.05 + 2e-6 * c;
    let prefill = |c: f64| 0.004 + 1e-7 * c;
    let mut fit = ActTimeFit::default();
    for (input, fresh) in [(10_000u32, 10_000u32), (20_000, 8_000), (35_000, 2_000), (50_000, 1_000), (75_000, 500)] {
        fit.observe(&act(input, fresh, 400, decode, prefill));
    }
    let ContextPrice::Measured(time) = fit.price() else { panic!("decode and prefill measured: priced, never unmeasured") };
    assert_eq!(time.fresh_at(30_000.0), 30_000.0, "cold: every held token is fresh");
}

// what this catches (Aris, the M5, 2026-11-11): decode and prefill per-token slopes inside their own
// errors (contention hides the per-token trend) left the price Unmeasured and his view unbounded,
// though his whole act's seconds rose measurably with context. The whole-act curve prices him; it
// is a measurement, never a guess, and a whole act that is noise too stays unmeasured.
#[test]
fn a_measured_whole_act_prices_when_its_parts_are_noise() {
    let flat_decode = |_: f64| 0.06;
    let flat_prefill = |_: f64| 0.008;
    let mut fit = ActTimeFit::default();
    // Output grows with context, so the whole act's seconds do; the per-token lines are flat and
    // her fresh tokens wander.
    for (input, fresh, output) in [(10_000u32, 3_000u32, 200u32), (20_000, 500, 400), (35_000, 2_500, 700), (50_000, 800, 1_000), (75_000, 2_000, 1_500)] {
        fit.observe(&act(input, fresh, output, flat_decode, flat_prefill));
    }
    let ContextPrice::Measured(time) = fit.price() else { panic!("a measured whole act prices") };
    assert_eq!(time.curve, Curve::Whole, "{time:?}");
    assert!((time.seconds_per_token(40_000.0) - time.observed.slope).abs() < 1e-12);
    assert!(time.seconds(60_000.0) > time.seconds(20_000.0), "the act costs more holding more");
}
