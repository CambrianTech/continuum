//! Caller authority for resolving output room against the selected engine's exact prompt count.
//! This policy neither acquires a lease nor authorizes dispatch: those remain the grant owner's
//! and transport's duties. Resolution must be repeated after an engine or grant changes.

/// What the caller permits the dispatch owner to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyRoom {
    /// Preserve this allowance exactly, or refuse. No implicit adaptation for ordinary callers.
    Fixed(u32),
    /// The caller explicitly permits reducing its ceiling, but never its required reply room.
    Act(ActReplyRoom),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActReplyRoom {
    minimum: u32,
    ceiling: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidActRoom {
    ZeroMinimum,
    MinimumAboveCeiling,
}

impl ActReplyRoom {
    pub fn new(minimum: u32, ceiling: u32) -> Result<Self, InvalidActRoom> {
        if minimum == 0 { return Err(InvalidActRoom::ZeroMinimum); }
        if minimum > ceiling { return Err(InvalidActRoom::MinimumAboveCeiling); }
        Ok(Self { minimum, ceiling })
    }
}

/// A receipt of this resolution, not a grant reservation or terminal generation receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl ReplyRoom {
    /// The engine requires prompt + reply < window. `window` must be the intersection of the
    /// current physical sequence and the caller's admitted lease, never a guessed larger pool.
    pub fn resolve(self, prompt: u32, window: u32) -> Result<QuotedReplyRoom, InsufficientReplyRoom> {
        let (minimum, ceiling) = match self {
            Self::Fixed(reply) => (reply, reply),
            Self::Act(room) => (room.minimum, room.ceiling),
        };
        let room = window.checked_sub(prompt).and_then(|n| n.checked_sub(1));
        match room {
            Some(room) if room >= minimum => Ok(QuotedReplyRoom { prompt, window, reply: room.min(ceiling) }),
            _ => Err(InsufficientReplyRoom { prompt, window, minimum }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kimi_density_error_requires_explicit_authority_to_resize() {
        assert!(ReplyRoom::Fixed(5857).resolve(27221, 32512).is_err());
        let policy = ReplyRoom::Act(ActReplyRoom::new(4534, 5857).expect("valid measured claim"));
        assert_eq!(policy.resolve(27221, 32512), Ok(QuotedReplyRoom { prompt: 27221, window: 32512, reply: 5290 }));
        assert!(policy.resolve(28000, 32512).is_err(), "never reduce the required reply claim");
        assert!(policy.resolve(27221, 30000).is_err(), "a smaller grant must be resolved again");
        assert_eq!(policy.resolve(20000, 32512).expect("room fits").reply, 5857, "no growth beyond caller authority");
    }

    #[test]
    fn strict_boundary_and_invalid_claims_are_refused() {
        assert_eq!(ActReplyRoom::new(0, 100), Err(InvalidActRoom::ZeroMinimum));
        assert_eq!(ActReplyRoom::new(101, 100), Err(InvalidActRoom::MinimumAboveCeiling));
        assert!(ReplyRoom::Fixed(1).resolve(99, 100).is_err());
        assert!(ReplyRoom::Fixed(0).resolve(100, 100).is_err());
        assert!(ReplyRoom::Fixed(u32::MAX).resolve(1, u32::MAX).is_err());
        assert_eq!(ReplyRoom::Fixed(0).resolve(99, 100).expect("one free cell").reply, 0);
    }
}
