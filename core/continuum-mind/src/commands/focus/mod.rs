//! `focus/*` — the persona's self-determination control surface (#91): the tools
//! through which she steers her own attention allocation. Backed by the by-persona
//! `crate::cognition::focus::registry()`, written here and read by the never-stop serve
//! loop's wake floor.
//!
//! Today: `focus/mute` (per-lane hush + snooze, already honored live by the wake floor)
//! and `focus/nudge` (relative lean on the focus *scalar* β, honored by the focus kernel
//! `FocusState::allocate`), and `focus/continue` (her continuation: where she leaves
//! unfinished work and what she expects next, read by her perception region's wakes). These ARE her agency seam — any mind drives focus through the
//! command surface, no bolt-on ML policy adapter
//! ([[commands-are-agency-algs-are-pathways]]). The sticky-cursor verb (`focus/attend`)
//! and the scalar's perceptual consumer (lane-level RAG breadth) land with multi-lane
//! perception (#43) — a knob is built once a pathway honors it, and the kernel honors β.
//!
//! All stateless (they resolve the global registry, hold no module state), so each is
//! a self-registering `action_command!` with zero wiring beyond its `pub mod` here.

pub mod continue_;
pub mod mute;
pub mod nudge;

use crate::cognition::focus::FocusState;
use crate::sdk_codegen::CommandError;

/// Her focus state, locked for a verb. A poisoned lock is a prior panic mid-mutation; the
/// focus registry's policy is to propagate it, never swallow it, so the verb refuses with
/// a typed internal error instead of panicking the command dispatcher.
fn lock_focus(
    handle: &std::sync::Mutex<FocusState>,
) -> Result<std::sync::MutexGuard<'_, FocusState>, CommandError> {
    handle.lock().map_err(|_| {
        CommandError::Internal("focus state poisoned by a prior panic mid-mutation; not read or written".into())
    })
}
