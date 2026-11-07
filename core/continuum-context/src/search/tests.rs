use super::*;

fn group(id: &str, priority: u8, options: &[u32]) -> Group {
    Group { id: id.to_owned(), priority, options: options.to_vec() }
}

fn spent(groups: &[Group], picks: &[usize]) -> u32 {
    groups.iter().zip(picks).map(|(g, &p)| g.options[p]).sum()
}

/// Every combination, exhaustively: the optimum the search is held to.
fn brute(window: u32, groups: &[Group], continuous: &[FlexClaim]) -> f64 {
    let mut best = f64::NEG_INFINITY;
    let mut picks = vec![0usize; groups.len()];
    loop {
        if let Some(value) = value_of(window, groups, &picks, continuous) {
            best = best.max(value);
        }
        let mut i = 0;
        loop {
            if i == groups.len() {
                return best;
            }
            picks[i] += 1;
            if picks[i] < groups[i].options.len() {
                break;
            }
            picks[i] = 0;
            i += 1;
        }
    }
}

// what this catches (Joel, 2026-11-07: "It's an optimizer too"): the flexbox's grant rounded down
// to a level collapses a high-priority item (its grant fell between its whole and its pointer)
// while two low-priority items keep grants they cannot use either. The search keeps the
// high-priority item whole by collapsing the two cheaper ones.
#[test]
fn the_search_keeps_a_high_value_item_whole_where_the_first_guess_collapsed_it() {
    let groups = [group("newest", 10, &[1_000, 100]), group("a", 3, &[600, 50]), group("b", 3, &[600, 50])];
    let window = 1_150;
    let guess = first_guess(window, &groups, &[]);
    assert_eq!(guess, [1, 1, 1], "precondition: the rounded flexbox collapses all three");
    let found = search(window, &groups, &[]).expect("fits");
    assert_eq!(found.picks, [0, 1, 1], "{found:?}");
    assert!(found.refined && found.value > found.first_guess_value);
    assert!(spent(&groups, &found.picks) <= window);
}

// what this catches: the ballpark table missing the optimum by more than its stated bound, or
// answering a combination that does not fit. Over a spread of instances: always within the window,
// never below the first guess, and equal to the exhaustive optimum whenever every option's cost
// above its hard limit is a multiple of the table's unit (where rounding loses nothing).
#[test]
fn the_search_fits_beats_the_first_guess_and_matches_the_optimum_where_rounding_is_exact() {
    let mut seed: u64 = 0x5eed;
    let mut next = |below: u32| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as u32) % below.max(1)
    };
    let (mut exact, mut matched) = (0, 0);
    for _ in 0..200 {
        let count = 2 + next(3) as usize;
        let groups: Vec<Group> = (0..count)
            .map(|g| {
                let mut costs: Vec<u32> = (0..1 + next(4)).map(|_| 10 + next(2_000)).collect();
                costs.sort_unstable_by(|a, b| b.cmp(a));
                costs.dedup();
                group(&format!("g{g}"), 1 + next(10) as u8, &costs)
            })
            .collect();
        let limits: u32 = groups.iter().map(Group::limit).sum();
        let window = limits + next(groups.iter().map(|g| g.want() - g.limit()).sum::<u32>() + 1);
        let history = FlexClaim { source_id: "history".into(), priority: 1 + next(10) as u8, floor_tokens: 0, min_tokens: 0, max_tokens: next(1_500), required: false };
        let continuous = [history];
        let found = search(window, &groups, &continuous).expect("every limit fits");
        assert!(spent(&groups, &found.picks) <= window);
        assert!(found.value >= found.first_guess_value);
        let span: u32 = groups.iter().map(|g| g.want() - g.limit()).sum::<u32>().min(window - limits);
        let unit = span.div_ceil(groups.iter().map(|g| g.options.len() as u32).sum()).max(1);
        let optimum = brute(window, &groups, &continuous);
        if found.value + 1e-9 >= optimum {
            matched += 1;
        }
        if groups.iter().all(|g| g.options.iter().all(|&c| (c - g.limit()) % unit == 0)) {
            exact += 1;
            assert!(found.value + 1e-9 >= optimum, "exact rounding must find the optimum: {groups:?} in {window}");
        }
    }
    println!("search matched the exhaustive optimum on {matched} of 200 instances ({exact} with exact rounding)");
}

// what this catches: a refusal that cannot say what binds, or one raised while a reduction would
// still fit.
#[test]
fn only_every_hard_limit_together_refuses_and_it_names_the_largest() {
    let groups = [group("tools", 7, &[11_000, 700]), group("framing", 10, &[2_700, 2_400])];
    let view = FlexClaim { source_id: "view/conversation".into(), priority: 4, floor_tokens: 9_000, min_tokens: 9_000, max_tokens: 20_000, required: true };
    assert!(search(12_200, &groups, &[view.clone()]).is_ok(), "every part at its hard limit fits");
    let over = search(12_000, &groups, &[view]).expect_err("12,100 of hard limits in 12,000");
    assert_eq!((over.needed, over.binding.as_str(), over.binding_limit), (12_100, "view/conversation", 9_000));
}

// what this catches: a search too slow to run every act. A resident's act at its size (her hands,
// her menu, her grounding, her required parts, a reply and seventeen view sources) is searched and
// timed; the time and the table cells are printed (run with --nocapture), and the cells are bounded
// by the options times their count, the arithmetic the module states.
#[test]
fn an_act_sized_search_reports_its_cost() {
    let ladder = |items: usize, full: u32, summary: u32, pointer: u32| {
        crate::ladder::Leveled::new((0..items).map(|i| crate::ladder::Rungs::new([full + i as u32, summary, pointer]).expect("levels")).collect()) // JUSTIFIED: test fixture
    };
    let tools = ladder(23, 480, 120, 25);
    let menu = ladder(20, 40, 12, 11);
    let grounding = ladder(8, 900, 200, 30);
    let groups = [
        Group { id: "tools".into(), priority: 7, options: tools.options().iter().map(|o| o.cost).collect() },
        Group { id: "framing".into(), priority: 10, options: menu.options().iter().map(|o| o.cost + 1_900).collect() },
        Group { id: "grounding".into(), priority: 6, options: grounding.options().iter().map(|o| o.cost).collect() },
        group("required", 10, &[300]),
        group("newest", 10, &[781]),
    ];
    let mut continuous: Vec<FlexClaim> = (0..17)
        .map(|i| FlexClaim { source_id: format!("view/s{i}"), priority: 1 + (i % 9) as u8, floor_tokens: 200, min_tokens: 200, max_tokens: 3_000, required: true })
        .collect();
    continuous.push(FlexClaim { source_id: "reply".into(), priority: 10, floor_tokens: 446, min_tokens: 446, max_tokens: 4_000, required: true });
    let started = std::time::Instant::now();
    let found = search(30_566, &groups, &continuous).expect("fits");
    let elapsed = started.elapsed();
    let options: usize = groups.iter().map(|g| g.options.len()).sum();
    println!("an act-sized search: {} options, {} cells, {:?}", options, found.cells, elapsed);
    assert!(found.cells <= options * (options + groups.len() + 1), "cells {} for {options} options", found.cells);
}
