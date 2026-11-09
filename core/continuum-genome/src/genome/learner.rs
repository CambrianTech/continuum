//! Learning in place: the genome's side of a mind learning on the engine that serves her
//! (ONE-RESIDENT-MODEL §9; card 4114a2c6). Training is the served forward pass with backprop on,
//! on the same resident weights, under the one budget: no separate trainer, window or reserve.

pub mod seat;
pub mod memory;
