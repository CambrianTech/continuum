//! continuum-hands: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! engine, the kernel and crates below them; core re-exports these modules under their old names.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_engine::{
    ai, capacity, concurrency, docker_tier, docker_tier_pool, entity_schemas, governor, gpu,
    identity, inference, inference_capability, logging, model_registry, orm, paging, paths,
    plasticity, policy, probe_query, probe_stream, resource_broker, resources, routing, runtime,
    sdk_codegen, sensory, serving_pin_store, serving_tier_down, system_resources, ConcurrencyPolicy,
    TokioConcurrencyPolicy,
};

pub mod code;
pub mod commands;
pub mod modules;

#[cfg(test)]
mod test_acting;

pub use continuum_engine::{
    action_command, clog_debug, clog_error, clog_info, clog_warn, log_debug, log_error, log_info,
    log_warn, register_command, register_event, register_outcome, register_stateless_command,
    stack, time_async, time_section,
};
