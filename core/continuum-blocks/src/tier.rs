//! [`TierKind`]: the memory a region lives in. Detected per node by `Topology` (UNIFIED-PAGING.md),
//! never listed by hand; the discrete GPU kinds (`Vram`, `Pinned`) arrive with their adapters (slice 4).

/// Which memory a region's bytes are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TierKind {
    /// One memory the CPU and the GPU both address (Apple silicon).
    Unified,
    /// Pageable host memory (a CPU-only node).
    Ram,
    /// A fast drive, through a file-backed mapping the OS pages.
    Flash,
}
