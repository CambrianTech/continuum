//! Physical sequences charged when fitting a serving pool.
#[derive(Debug, Copy, Clone)]
pub enum StreamLayout {
    Roster,
    /// The in-process Core (CONTINUUM-ENGINE 4.3): one KV pool that every seated mind grows into, and a
    /// sequence (with its fixed recurrent cost) for each mind on the roster.
    PersonaScheduler,
}

impl StreamLayout {
    pub fn sequences(self, roster: u32) -> u32 {
        roster.max(1)
    }

    pub fn minimum_window(self, roster_minimum: u32) -> u32 {
        match self {
            Self::Roster => roster_minimum,
            Self::PersonaScheduler => 0,
        }
    }

    pub fn window(self, pool: u64, roster: u32, trained: u32) -> u32 {
        match self {
            Self::Roster => (pool / u64::from(self.sequences(roster))).min(u64::from(u32::MAX)) as u32,
            // The whole pool, never a share: every seated mind may grow into all of it. Bounded by the
            // model's trained context; the native context pads to 256 tokens.
            Self::PersonaScheduler => {
                let tokens = pool.min(u64::from(trained)) as u32;
                tokens - tokens % 256
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StreamLayout;

    // what this catches: the Core's window divided among its minds again (2026-11-21: two streams
    // halved it, and seven turns died at the half's end while the other half held nobody decoding),
    // and the Core seating one mind, so every switch copies her whole state out and the next one in
    // (63-259 ms on the M5, 2026-11-24). Every mind on the roster has a seat; each may grow into the
    // whole pool, bounded only by the model's trained context.
    #[test]
    fn the_core_seats_its_roster_in_one_pool_none_of_it_divided() {
        let layout = StreamLayout::PersonaScheduler;
        for roster in [1, 5] {
            assert_eq!(layout.sequences(roster), roster, "a seat for every mind on the roster");
            assert_eq!(layout.window(511, roster, 98_304), 256);
            assert_eq!(layout.window(121_600, roster, 262_144), 121_600 - 121_600 % 256, "the whole pool");
            assert_eq!(layout.window(121_600, roster, 98_304), 98_304, "the trained context bounds it");
            assert_eq!(layout.window(121_600, roster, 32_767), 32_512);
        }
    }
}
