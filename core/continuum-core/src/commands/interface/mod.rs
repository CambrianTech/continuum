//! `interface/*` — commands that drive or observe a render surface.
//!
//! `capture` screenshots a server-driven target (web / iOS sim / Android emulator)
//! the persona is building — distinct from the client-routed `interface/screenshot`
//! that lives in `crate::interface` and captures a connected client's own UI.

pub mod capture;
