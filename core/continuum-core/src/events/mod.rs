//! Event-class registry — the Rust-truth layer for cross-environment
//! event metadata that decides which transport tier carries each event.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §2.2 + §6.2 (continuum#1439).
//!
//! Continuum-side TS reads through the IPC binding (`bindings/modules/events.ts`)
//! and the thin shim at `src/system/events/shared/EventClass.ts`. Per the
//! native-truth-thin-SDK-per-language pattern, this module is the single
//! canonical source of EventClass declarations + lookups; the TS side
//! caches reads locally for the hot emit-path but never mutates without
//! going through the IPC.

pub mod event_class;
pub mod event_class_registry;

pub use event_class::{
    resolve_event_class_config, EventClassChannelStrategy, EventClassConfig,
    EventClassDeclareError, EventClassUnknownSchemaPolicy, ResolvedEventClassConfig,
};
pub use event_class_registry::{
    declare_event_class, event_class_registry, list_event_classes, lookup_event_class,
    resolve_event_class_channel, EventClassChannelResolveError, EventClassRegistry,
    EventClassRegistryError,
};
