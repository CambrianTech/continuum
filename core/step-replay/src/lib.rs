//! step-replay: the causal contract's validator (object model 5.7, "a step in time she can always
//! see"). Every act a mind took must reach her next prompt as her step: the call she made and its
//! outcome, bound by the call's id. A mind that cannot see her last step can only repeat it, and the
//! fault is the system's, never hers.
//!
//! [`capture`] reads a node's prompt captures into each mind's acts; [`contract`] checks consecutive
//! acts and counts the ways a step goes missing.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod capture;
pub mod contract;
