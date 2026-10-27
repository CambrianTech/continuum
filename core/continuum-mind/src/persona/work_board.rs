//! A citizen's WORK BOARD as her mind uses it: the board's facts, the boards she can see,
//! the state-advance seam with the room's review gate, her submissions, and the bridge that
//! turns a card transition into a bus event. The board itself is airc's; the `work/*`
//! commands (`modules::work`) are one surface over these, and her own loop is another.

pub mod advance;
pub mod events;
pub mod horizon;
pub mod review;
pub mod submissions;
