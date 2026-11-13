//! `embedding/*` — text-level similarity commands in the ONE process-wide
//! embedding space.
//!
//! `embedding/similar` ranks text candidates against a query and
//! `embedding/groups` groups candidates that say the same thing. Both embed
//! through the provider and score through `vector_math`, the one cosine owner.
//! Each self-registers via `register_command!` — no module `commands()` wiring,
//! no `match` arm. The Node-era raw-vector ports (`top-k`, `similarity`,
//! `similarity-matrix`, `cluster`) were deleted: no caller scored vectors it
//! already had.

pub mod groups;
pub mod similar;
