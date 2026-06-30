//! `focus/*` — the persona's self-determination control surface (#91): the tools
//! through which she steers her own attention allocation. Backed by the by-persona
//! `crate::persona::focus::registry()`, written here and read by the never-stop serve
//! loop's wake floor.
//!
//! Today: `focus/mute` (per-lane hush + snooze). The focus *scalar* and sticky cursor
//! gain their own tools as their consumers land (RAG cross-thread breadth, tool-catalog
//! expansion) — a control knob is built only once something honors it.
//!
//! All stateless (they resolve the global registry, hold no module state), so each is
//! a self-registering `action_command!` with zero wiring beyond its `pub mod` here.

pub mod mute;
