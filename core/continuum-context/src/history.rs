//! What a mind's context needs of a room's history: how many of the room's newest lines her thread
//! of it holds. The codec declares it, so the thread never sizes itself (Codex on #6241).

/// The room's newest lines her thread holds: in a room of four residents about her own last eight,
/// the self-history the repetition cluster of three needs with slack, and twice the 16-line chorus
/// window the old ring held. An estimate from those two needs, not a measurement; it becomes a
/// claim in the act's one allocation when history is sized from the model's grant.
pub const ROOM_HISTORY_NEED: usize = 32;
