//! The one canonical expert identity. Moved here from
//! continuum-core's `capacity::expert_residency` (which re-exports it)
//! so the windows-clean policy crate owns the type its plans are keyed
//! by.

/// One expert, addressed by (layer, index) — matches the GGUF tensor
/// naming and the router's per-layer expert space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExpertId {
    pub layer: u32,
    pub expert: u32,
}
