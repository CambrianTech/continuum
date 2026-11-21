//! Physical sequences charged when fitting a serving pool.
#[derive(Debug, Copy, Clone)]
pub enum StreamLayout {
    Roster,
    PersonaScheduler,
}

impl StreamLayout {
    pub fn sequences(self, roster: u32) -> u32 {
        match self {
            Self::Roster => roster.max(1),
            Self::PersonaScheduler => continuum_persona_llama::STREAMS as u32,
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
            // The pinned native context pads each physical stream to 256 tokens.
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

    #[test]
    fn native_pool_charges_two_streams_and_fits_native_padding() {
        let layout = StreamLayout::PersonaScheduler;
        for roster in [1, 5] {
            assert_eq!(layout.sequences(roster), 2);
            assert_eq!(layout.window(511, roster, 98_304), 0);
            assert_eq!(layout.window(121_600, roster, 32_767), 32_512);
            assert_eq!(layout.window(121_600, roster, 98_304), 60_672);
            assert_eq!(layout.window(121_600, roster, 32_768), 32_768);
        }
    }
}
