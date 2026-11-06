//! Output intent resolved by the dispatch owner after an exact prompt quote.
//! Resolution is arithmetic evidence, never endpoint custody or permission to send.

use std::num::NonZeroU32;

/// Explicit caller authority. An estimate is not an output ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputIntent {
    Fixed(NonZeroU32),
    UseAdmittedRoom { required_reply: NonZeroU32 },
}

/// Resolution evidence; the executable attempt must bind it to owned custody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct QuotedReplyRoom {
    pub prompt: u32,
    pub window: u32,
    pub reply: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsufficientReplyRoom {
    pub prompt: u32,
    pub window: u32,
    pub minimum: u32,
}

impl OutputIntent {
    /// Strict feasibility: P + R < min(physical, admitted). Capacity inputs must
    /// come from the selected endpoint and grant owners on this exact attempt.
    pub fn resolve(self, prompt: u32, physical: u32, admitted: u32) -> Result<QuotedReplyRoom, InsufficientReplyRoom> {
        let window = physical.min(admitted);
        let minimum = match self {
            Self::Fixed(reply) => reply.get(),
            Self::UseAdmittedRoom { required_reply } => required_reply.get(),
        };
        let available = window.checked_sub(prompt).and_then(|n| n.checked_sub(1));
        match available {
            Some(room) if room >= minimum => {
                let reply = match self {
                    Self::Fixed(reply) => reply.get(),
                    Self::UseAdmittedRoom { .. } => room,
                };
                Ok(QuotedReplyRoom { prompt, window, reply })
            }
            _ => Err(InsufficientReplyRoom { prompt, window, minimum }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positive(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).expect("positive fixture")
    }

    #[test]
    fn recorded_prediction_errors_resolve_without_estimate_authority() {
        for (prompt, window, predicted, required, actual) in [
            (27221, 32512, 5857, 4534, 5290),
            (18702, 22435, 3769, 3558, 3732),
        ] {
            assert!(OutputIntent::Fixed(positive(predicted)).resolve(prompt, window, window).is_err());
            let intent = OutputIntent::UseAdmittedRoom { required_reply: positive(required) };
            assert_eq!(intent.resolve(prompt, window, window), Ok(QuotedReplyRoom { prompt, window, reply: actual }));
            assert!(intent.resolve(window - required, window, window).is_err());
            assert_eq!(intent.resolve(0, window, window).expect("empty prompt fits").reply, window - 1);
        }
    }

    #[test]
    fn exhaustive_small_domain_matches_independent_feasible_set() {
        for physical in 0..20_u32 {
            for admitted in 0..20_u32 {
                for prompt in 0..22_u32 {
                    for required in 1..22_u32 {
                        let feasible: Vec<u32> = (required..22)
                            .filter(|reply| u64::from(prompt) + u64::from(*reply) < u64::from(physical.min(admitted)))
                            .collect();
                        let adaptive = OutputIntent::UseAdmittedRoom { required_reply: positive(required) }
                            .resolve(prompt, physical, admitted);
                        assert_eq!(adaptive.map(|r| r.reply).ok(), feasible.last().copied());
                        let fixed = OutputIntent::Fixed(positive(required)).resolve(prompt, physical, admitted);
                        assert_eq!(fixed.map(|r| r.reply).ok(), feasible.first().copied());
                    }
                }
            }
        }
    }

    #[test]
    fn capacity_intersection_and_integer_extremes_are_safe() {
        let intent = OutputIntent::UseAdmittedRoom { required_reply: positive(1) };
        assert_eq!(intent.resolve(0, u32::MAX, u32::MAX).expect("max window").reply, u32::MAX - 1);
        assert!(intent.resolve(u32::MAX, u32::MAX, u32::MAX).is_err());
        assert!(intent.resolve(u32::MAX - 1, u32::MAX, u32::MAX).is_err());
        assert!(intent.resolve(0, 0, u32::MAX).is_err());
        assert!(OutputIntent::Fixed(positive(u32::MAX)).resolve(0, u32::MAX, u32::MAX).is_err());
        for window in 2..100 {
            let a = intent.resolve(0, window, 100).expect("fits");
            let b = intent.resolve(0, 100, window).expect("fits");
            assert_eq!(a, b);
            assert_eq!(intent.resolve(0, window + 1, 100).expect("larger fits").reply, a.reply + 1);
        }
    }
}
