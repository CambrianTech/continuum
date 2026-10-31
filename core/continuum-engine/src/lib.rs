//! continuum-engine: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! kernel and crates below it; core re-exports these modules under their old names.

pub use continuum_kernel::{
    concurrency, docker_tier, docker_tier_pool, entity_schemas, governor, gpu, identity, logging,
    message_processor, orm, paging, paths, policy, priority_queue, probe_query, probe_stream,
    resource_broker, resources, routing, runtime, sdk_codegen, system_resources, ConcurrencyPolicy,
    ConcurrentPriorityQueue, ConcurrentProcessor, MessageProcessor, Prioritized, PriorityWrapper,
    TokioConcurrencyPolicy,
};

pub mod ai;
pub mod capacity;
pub mod inference;
pub mod inference_capability;
pub mod model_registry;
pub mod plasticity;
pub mod sensory;
pub mod serving_pin_store;
pub mod serving_tier_down;

pub use continuum_kernel::{
    action_command, clog_debug, clog_error, clog_info, clog_warn, log_debug, log_error, log_info,
    log_warn, register_command, register_event, register_outcome, register_stateless_command,
    stack, time_async, time_section,
};
