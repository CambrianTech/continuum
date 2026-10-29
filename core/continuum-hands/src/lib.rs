//! continuum-hands: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! engine, the kernel and crates below them; core re-exports these modules under their old names.

pub use continuum_engine::{
    ai, capacity, concurrency, docker_tier, docker_tier_pool, entity_schemas, governor, gpu,
    identity, inference, inference_capability, logging, message_processor, model_registry, orm,
    paging, paths, plasticity, policy, priority_queue, probe_query, probe_stream, resource_broker,
    resources, routing, runtime, sdk_codegen, sensory, serving_pin_store, serving_tier_down,
    system_resources, ConcurrencyPolicy, ConcurrentPriorityQueue, ConcurrentProcessor,
    MessageProcessor, Prioritized, PriorityWrapper, TokioConcurrencyPolicy,
};

pub mod code;
pub mod commands;
pub mod modules;

#[cfg(test)]
mod test_acting;
