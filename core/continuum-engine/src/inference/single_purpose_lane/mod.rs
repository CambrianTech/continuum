//! A single-purpose lane: a second engine stood up beside serving for one purpose (a gene
//! eval, a training run). One owner for the questions every such lane asks before it starts:
//! does it fit in memory ([`memory`]), what window it runs at ([`window`]), and which device
//! ([`placement`]). Lifted out of `cognition::eval` (cards c180de5d, c42c4b3b) so the training
//! adapter that replaces the Python trainers asks them through the same code as gene eval; it
//! lives in the engine because every question it answers is a serving fact (memory, window,
//! device), so the genome's trainer and the mind's gene eval both reach it from below.

mod memory;
mod placement;
mod window;

pub use memory::*;
pub use placement::*;
pub use window::*;
