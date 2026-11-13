//! INFERENCE PLANNING: how this node's serving capacity is shared out, as plans and leases.
//! The throughput lanes a job may use, the leases that hold them, and the service-rate
//! model that judges whether a model can seat its minds. These answer an inference
//! question, so cognition (and routing, provisioning, runtime) consumes them from here,
//! never the other way round (card d2aee969; they lived under cognition/ until 2026-10-12).

pub mod adaptive_throughput;
pub mod contention;
pub mod service_rate;
pub mod serving_plan;
pub mod training_plan;
pub mod window_allocator;

/// Bare-minimum served window for a model to be runnable at ALL — a hardware
/// reality floor, NOT a serving target or a cheapening cap. The served window is
/// sized UP from here to the largest that fits the host budget, capped by BOTH
/// the model's own trained `context_window` AND the DEMAND ceiling
/// (`BOOTSTRAP_WORKING_SET` / measured p95) — a lane is provisioned for what
/// personas actually USE, never maximized to fill RAM. A model whose weights + KV
/// at even this floor won't fit the GPU budget is simply not a serving option on
/// this host (→ `fits_on_gpu = false`, honest degrade — never a silent shrink).
// context-budget-exempt: the hardware FLOOR the whole serving stack sizes UP from — the one substrate-owned minimum every other bound derives against, and never a cap on anything
pub const MIN_SERVE_CTX: u32 = 2048;
