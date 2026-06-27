//! `commands/events/` — the event-class registry surface.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md); spec
//! GRID-BUS-ARCHITECTURE §2.2 (continuum#1439). The four verbs that used to live
//! in `EventsModule::handle_command` are now typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s — they hold no state (the
//! event-class registry is a process singleton reached through the free functions
//! in [`crate::events`]), so each self-registers via `register_stateless_command!`
//! with zero host-module ceremony.
//!
//! Access levels follow the read/mutate split: `events/declare-class` mutates the
//! wire-contract registry (substrate bootstrap, in-process) → `Internal`; the
//! lookups (`get-class`, `list-classes`, `resolve-channel`) are introspection →
//! `AiSafe`.

pub mod declare_class;
pub mod get_class;
pub mod list_classes;
pub mod resolve_channel;
