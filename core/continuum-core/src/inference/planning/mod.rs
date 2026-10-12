//! INFERENCE PLANNING: how this node's serving capacity is shared out, as plans and leases.
//! The throughput lanes a job may use, the leases that hold them, and the service-rate
//! model that judges whether a model can seat its minds. These answer an inference
//! question, so cognition (and routing, provisioning, runtime) consumes them from here,
//! never the other way round (card d2aee969; they lived under cognition/ until 2026-10-12).

pub mod adaptive_throughput;
pub mod service_rate;
pub mod throughput_lease;
