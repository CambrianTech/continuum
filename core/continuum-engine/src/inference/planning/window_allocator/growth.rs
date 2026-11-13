//! How far a mind's act grows past her typical act, measured from her own turns.
//!
//! A target is her requirement with room to grow into. That room used to be one factor for every
//! mind on every node (`SENT_HEADROOM = 1.25`): a mind whose turns barely move was granted a
//! quarter more than she ever asks for, and a mind whose act doubles one turn in twenty was
//! granted a quarter. The room is now hers, read off the demands her view assembled.

use super::Tokens;

/// The growth of a mind's act over her typical act, from her recent untrimmed demands (the ring
/// `working_set::PersonaDemand::recent` keeps).
///
/// **Estimator**: the nearest-rank p95 of her recorded demands over their upper median (the same
/// median `working_set::typical_tokens` reads, so her requirement and her growth share a
/// denominator). On a 16-turn ring the nearest-rank p95 is the ring's largest demand.
///
/// **Sample floor**: the caller's full ring (`working_set::RECENT_TURNS`, one working session).
/// With fewer recorded demands the growth is [`Growth::Unmeasured`] and her target is her
/// requirement: the tail of a handful of turns is a guess, and a mind whose act outgrows its
/// grant is still raised by her overflow (`working_set::requirement_tokens`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Growth {
    /// Fewer recorded demands than the sample floor, or a requirement that is declared rather than
    /// measured: no growth is known, so nothing is added.
    Unmeasured,
    /// Her p95 demand (`tail`) over her median demand (`typical`); `tail >= typical`.
    Measured { tail: Tokens, typical: Tokens },
}

impl Growth {
    /// Her growth from `demands` (zero slots are unrecorded turns), measured once at least
    /// `sample_floor` demands are recorded.
    pub fn of(demands: &[u32], sample_floor: usize) -> Self {
        let mut recorded: Vec<u32> = demands.iter().copied().filter(|t| *t > 0).collect();
        if recorded.is_empty() || recorded.len() < sample_floor {
            return Self::Unmeasured;
        }
        recorded.sort_unstable();
        // Nearest rank: the smallest demand at or above 95% of the recorded ones. Never 0, since
        // at least one demand is recorded.
        let rank = (recorded.len() * 95).div_ceil(100);
        let tail = recorded[rank - 1];
        let typical = recorded[recorded.len() / 2];
        match (Tokens::new(tail), Tokens::new(typical)) {
            (Some(tail), Some(typical)) => Self::Measured { tail: tail.max(typical), typical },
            _ => Self::Unmeasured,
        }
    }

    /// `tokens` grown by this growth: never below `tokens`, and unmeasured adds nothing.
    pub fn grow(self, tokens: Tokens) -> Tokens {
        match self {
            Self::Unmeasured => tokens,
            Self::Measured { tail, typical } => {
                let grown = u64::from(tokens.get()) * u64::from(tail.get()) / u64::from(typical.get());
                // Past the token width is past any window: saturate there.
                let grown = u32::try_from(grown).map_or(u32::MAX, |g| g);
                Tokens::new(grown).map_or(tokens, |g| g.max(tokens))
            }
        }
    }

    /// The larger of two growths: a floor shared by several minds covers each one's growth.
    pub fn max(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unmeasured, g) | (g, Self::Unmeasured) => g,
            (Self::Measured { tail: a, typical: ta }, Self::Measured { tail: b, typical: tb }) => {
                // a / ta against b / tb, without division.
                match u64::from(a.get()) * u64::from(tb.get()) >= u64::from(b.get()) * u64::from(ta.get()) {
                    true => self,
                    false => other,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::test_tokens as t;

    /// A full working session: the ring `working_set` keeps.
    const RING: usize = 16;

    // what this catches: a growth read off too few turns, or a fleet-wide factor standing in for
    // hers. Under the floor nothing is added; at the floor her target is her median grown to her
    // p95 (the ring's largest demand on 16 turns), so a steady mind gets no room she never uses
    // and a spiky one gets the room her own spikes measured.
    #[test]
    fn her_growth_is_her_p95_over_her_median_once_her_ring_is_full() {
        assert_eq!(Growth::of(&[40_000; 15], RING), Growth::Unmeasured, "15 turns is under the floor");
        assert_eq!(Growth::of(&[0; RING], RING), Growth::Unmeasured, "an empty ring measured nothing");
        let steady = Growth::of(&[40_000; RING], RING);
        assert_eq!(steady.grow(t(40_000)), t(40_000), "a steady mind grows by nothing: {steady:?}");
        let mut spiky = [30_000u32; RING];
        spiky[3] = 90_000;
        let growth = Growth::of(&spiky, RING);
        assert_eq!(growth, Growth::Measured { tail: t(90_000), typical: t(30_000) });
        assert_eq!(growth.grow(t(30_000)), t(90_000), "her target holds the act she took");
        assert_eq!(Growth::Unmeasured.grow(t(30_000)), t(30_000), "unmeasured adds nothing");
    }

    // what this catches: a floor shared by several minds sized to the smaller growth.
    #[test]
    fn the_larger_growth_covers_both() {
        let small = Growth::Measured { tail: t(110), typical: t(100) };
        let large = Growth::Measured { tail: t(300), typical: t(200) };
        assert_eq!(small.max(large), large);
        assert_eq!(large.max(small), large);
        assert_eq!(Growth::Unmeasured.max(small), small);
    }
}
