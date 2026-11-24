//! A room, as cognition meets it: an airc room. Every activity is one (a chat, a card's work, a
//! benchmark, a screen on the desktop), visible and joinable through airc outside continuum
//! too, so this is a layer over airc rooms and never a second store of them: the one feed of
//! their events (`feed`), her thread of each room she is in (`thread`), and what was said in one
//! as a turn reads it (`said`).

pub mod feed;
pub mod said;
pub mod thread;
