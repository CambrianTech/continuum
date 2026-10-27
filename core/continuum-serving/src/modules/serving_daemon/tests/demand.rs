#![cfg(test)]

use super::*;

#[test]
fn a_burst_of_leased_in_calls_on_one_tick_does_not_move_the_lane_count() {
    let w = DOWNSHIFT_SUSTAINED_TICKS as usize;
    assert!(w >= 2, "the guard only means something with a window");
    assert_eq!(
        sustained_leased_in(1, 3),
        0,
        "a fresh gauge never relaunches"
    );
    assert_eq!(
        sustained_leased_in(3, 0),
        0,
        "a burst that vanished is not demand"
    );
    assert_eq!(sustained_leased_in(3, 0), 0, "two ticks is still a burst");
    assert_eq!(
        sustained_leased_in(3, 3),
        3,
        "three consecutive ticks: a load"
    );
    assert_eq!(
        sustained_leased_in(3, 3),
        3,
        "the window is the LAST w ticks: [0, 3, 3, 3] keeps [3, 3, 3]"
    );
    assert_eq!(
        sustained_leased_in(3, 2),
        2,
        "the minimum, not the max: a dip is honored"
    );
    assert_eq!(
        sustained_leased_in(3, 1),
        1,
        "a falling load falls at once: [3, 3, 3, 1] keeps [3, 3, 1]"
    );
}
