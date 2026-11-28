//! Physical sequences charged when fitting a serving pool.
#[derive(Debug, Copy, Clone)]
pub enum StreamLayout {
    Roster,
    /// The in-process Core: a stream per mind it binds at once, every one a whole window (object model 4.11; THE-LANE-
    /// ON-SAGE.md, open question 1). How many is the fit's (`whole_window_streams` in serving): never a window
    /// divided to seat another mind.
    PersonaScheduler,
}

impl StreamLayout {
    /// The sequences a roster's pool is fitted for: one a mind; the Core's base is its one whole stream.
    pub fn sequences(self, roster: u32) -> u32 {
        match self {
            Self::Roster => roster.max(1),
            Self::PersonaScheduler => 1,
        }
    }

    pub fn minimum_window(self, roster_minimum: u32) -> u32 {
        match self {
            Self::Roster => roster_minimum,
            Self::PersonaScheduler => 0,
        }
    }

    /// Each sequence's window, for `sequences` of them in `pool`.
    pub fn window(self, pool: u64, sequences: u32, trained: u32) -> u32 {
        let share = pool / u64::from(sequences.max(1));
        match self {
            Self::Roster => share.min(u64::from(u32::MAX)) as u32,
            // a whole stream's share, bounded by the model's trained context; the pinned native context pads its
            // stream to 256 tokens
            Self::PersonaScheduler => {
                let tokens = share.min(u64::from(trained)) as u32;
                tokens - tokens % 256
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamLayout;

    // what this catches: the Core's context divided among streams again (2026-11-21: two streams halved it, and
    // seven turns died at the half's end while the other half held nobody decoding). One stream holds the whole
    // pool up to the trained context; streams beyond it are the fit's, each given a whole window.
    #[test]
    fn the_core_holds_the_whole_pool_in_one_stream_up_to_the_trained_context() {
        let layout = StreamLayout::PersonaScheduler;
        assert_eq!(layout.sequences(5), 1, "the roster never divides the Core");
        assert_eq!(layout.window(511, 1, 98_304), 256);
        assert_eq!(layout.window(121_600, 1, 262_144), 121_600 - 121_600 % 256, "the whole pool");
        assert_eq!(layout.window(121_600, 1, 98_304), 98_304, "the trained context bounds it");
        assert_eq!(layout.window(121_600, 1, 32_767), 32_512);
        assert_eq!(layout.window(4 * 262_144, 4, 262_144), 262_144, "four streams the fit admitted, each whole");
    }
}
