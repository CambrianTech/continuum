use super::*;
use crate::detect::Insufficient;

const HOUR: u64 = 3_600_000;

fn policy() -> ServicePolicy {
    ServicePolicy { history: 8, min_evidence: 3, sustained: 2 }
}

/// The 5090 as Kimi's node reported it on 2026-10-26.
fn stream() -> StreamFacts {
    StreamFacts {
        node: "bigmama".into(),
        builds: Builds { core: "b2deeee1a".into(), engine: String::new() },
        config: ServingConfig {
            lanes: 2,
            ctx_per_lane: 49_152,
            cache_ram_mib: 4096,
            kv_type: String::new(),
            backend: String::new(),
            expert_residency: String::new(),
        },
        incarnation: EngineIncarnation { pid: 4242, started_ms: 0 },
        residents_active: 2,
        memory_ceiling_bytes: 0,
    }
}

fn span(hour: u64) -> Window {
    Window { start_ms: hour * HOUR, end_ms: (hour + 1) * HOUR }
}

/// Kimi's refusal: required 26,101 > budget 17,829 at a 49,152 window.
const KIMI: Refusal = Refusal::PromptCapacity { required_tokens: 26_101, budget_tokens: 17_829, context_window: Some(49_152) };

fn refused(refusal: Refusal) -> Turn {
    Turn { completion: None, refusal: Some(refusal) }
}

fn completed(wall_ms: u64) -> Turn {
    Turn {
        completion: Some(Completion {
            generations: 3,
            acts: 2,
            wall_ms,
            prompt_tokens: 24_000,
            output_tokens: 300,
            cached_tokens: 18_000,
            prefilled_tokens: 6_000,
            model: "qwen3.6-35b-a3b".into(),
        }),
        refusal: None,
    }
}

// what this catches: Kimi, 2026-10-26. Her loop ran every beat and every turn was refused before
// inference; the node said nothing for 17 hours. Demand with zero completions is a stall within
// ONE window, never idle, and the alarm names the refusal's numbers (required, budget, context).
// A mind parked on that refusal asks nothing new, and the next window is still a stall.
#[test]
fn every_turn_refused_at_capacity_is_a_stall_within_one_window_naming_the_numbers() {
    let (mut ledger, mut state) = (ResidentLedger::default(), ServiceState::default());
    for _ in 0..12 {
        ledger.note(refused(KIMI));
    }
    let window = ledger.close();
    let verdict = judge(&mut state, &policy(), &window, span(0), &stream());
    assert_eq!(verdict, Verdict::Stalled { offered: 12, failures: 12 });
    let kimi = Uuid::from_u128(0x5090);
    let Some(alarm) = ResidentAlarm::of(kimi, "Kimi", &window, span(0), &verdict) else { panic!("a stall raises an alarm") };
    assert_eq!((alarm.persona_id, alarm.state.clone(), alarm.demand, alarm.completions, alarm.refusals), (kimi, AlarmState::Stalled, 12, 0, 12));
    assert_eq!(alarm.refusal, Some(KIMI));
    assert!(alarm.parked);
    assert_eq!(alarm.line(), "Kimi stalled: 12 asked, 0 completed, prompt capacity 26101 > 17829 at context 49152, parked");

    let parked = ledger.close();
    assert_eq!((parked.offered, parked.dominant), (1, Some((KIMI, 0))));
    assert_eq!(judge(&mut state, &policy(), &parked, span(1), &stream()), Verdict::Stalled { offered: 1, failures: 0 });
}

// what this catches: the alarm must never cry wolf on a working mind. Turns that complete at her
// own pace read Insufficient while her history fills, then Healthy, and raise nothing; a turn
// that completed and THEN parked (Sahar at 20:05Z) is not a stall in that window, but the next
// window, parked with nothing completed, is.
#[test]
fn a_working_resident_is_healthy_and_a_completed_turn_that_then_parks_stalls_the_next_window() {
    let (mut ledger, mut state) = (ResidentLedger::default(), ServiceState::default());
    for hour in 0..4 {
        for wall_ms in [50_000, 55_000, 60_000] {
            ledger.note(completed(wall_ms));
        }
        let window = ledger.close();
        assert_eq!(window.warmup, Warmup::Warm);
        let verdict = judge(&mut state, &policy(), &window, span(hour), &stream());
        let expected = if hour < 3 { Verdict::Insufficient(Insufficient::NoReference) } else { Verdict::Healthy };
        assert_eq!(verdict, expected, "hour {hour}");
        assert_eq!(ResidentAlarm::of(Uuid::nil(), "Aiko", &window, span(hour), &verdict), None);
    }

    let sahar = Refusal::PromptCapacity { required_tokens: 63_840, budget_tokens: 49_152, context_window: Some(49_152) };
    ledger.note(completed(55_000));
    ledger.note(Turn { completion: None, refusal: Some(sahar) });
    assert_eq!(judge(&mut state, &policy(), &ledger.close(), span(4), &stream()), Verdict::Healthy);
    assert!(matches!(judge(&mut state, &policy(), &ledger.close(), span(5), &stream()), Verdict::Stalled { .. }));
}

// what this catches: an idle resident (nothing asked) and a stopped one (her loop ended while
// parked) must never read as stalled: no demand is Insufficient, and the alarm stays silent. A
// turn that dispatched nothing and was refused nothing is not demand either.
#[test]
fn an_idle_or_stopped_resident_is_insufficient_never_stalled() {
    let (mut ledger, mut state) = (ResidentLedger::default(), ServiceState::default());
    ledger.note(Turn { completion: None, refusal: None });
    let idle = ledger.close();
    let verdict = judge(&mut state, &policy(), &idle, span(0), &stream());
    assert_eq!(verdict, Verdict::Insufficient(Insufficient::NoDemand));
    assert_eq!(ResidentAlarm::of(Uuid::nil(), "Atlas", &idle, span(0), &verdict), None);

    ledger.note(refused(KIMI));
    ledger.unpark();
    assert!(matches!(judge(&mut state, &policy(), &ledger.close(), span(1), &stream()), Verdict::Stalled { .. }), "the refused hour itself was a stall");
    assert_eq!(judge(&mut state, &policy(), &ledger.close(), span(2), &stream()), Verdict::Insufficient(Insufficient::NoDemand), "stopped, not stalled");
}

// what this catches: the dominant refusal is the class met MOST, at its latest numbers, so a
// lane-starved blip does not hide the capacity refusal that is the real cause.
#[test]
fn the_dominant_refusal_is_the_class_met_most_at_its_latest_numbers() {
    let earlier = Refusal::PromptCapacity { required_tokens: 25_000, budget_tokens: 17_829, context_window: None };
    let window = ResidentWindow::of(vec![refused(earlier), refused(KIMI), refused(Refusal::LaneStarved)], None, "");
    assert_eq!(window.dominant, Some((KIMI, 2)));
}
