//! Physical sequences charged when fitting a serving pool.
#[derive(Debug, Copy, Clone)]
pub enum StreamLayout {
    Roster,
    /// The in-process Core: its streams (`persona_scheduler::STREAMS`), each a whole window (object model 4.11).
    PersonaScheduler,
}

impl StreamLayout {
    pub fn sequences(self, roster: u32) -> u32 {
        match self {
            Self::Roster => roster.max(1),
            Self::PersonaScheduler => crate::inference::backends::persona_scheduler::STREAMS.get(),
        }
    }

    pub fn minimum_window(self, roster_minimum: u32) -> u32 {
        match self {
            Self::Roster => roster_minimum,
            Self::PersonaScheduler => 0,
        }
    }

    pub fn window(self, pool: u64, roster: u32, trained: u32) -> u32 {
        let share = pool / u64::from(self.sequences(roster));
        match self {
            Self::Roster => share.min(u64::from(u32::MAX)) as u32,
            // The Core's whole pool, bounded by the model's trained context; the pinned native
            // context pads its stream to 256 tokens.
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

    // what this catches: the Core's context divided among streams again (2026-11-21: two streams
    // halved it, and seven turns died at the half's end while the other half held nobody decoding).
    // The mind swapped in gets the whole pool, bounded only by the model's trained context.
    #[test]
    fn the_core_holds_the_whole_pool_in_one_stream_up_to_the_trained_context() {
        let layout = StreamLayout::PersonaScheduler;
        for roster in [1, 5] {
            assert_eq!(layout.sequences(roster), 1, "the roster never divides the Core");
            assert_eq!(layout.window(511, roster, 98_304), 256);
            assert_eq!(layout.window(121_600, roster, 262_144), 121_600 - 121_600 % 256, "the whole pool");
            assert_eq!(layout.window(121_600, roster, 98_304), 98_304, "the trained context bounds it");
            assert_eq!(layout.window(121_600, roster, 32_767), 32_512);
        }
    }
}
