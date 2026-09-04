//! Event codegen — the symmetric other half of the SDK (the second universal
//! primitive). Commands are Rust-sourced via [`CommandSpec`](super::CommandSpec);
//! events are Rust-sourced via [`EventSpec`] the same way, so BOTH primitives
//! flow from one Rust declaration to every language SDK and can't drift.
//!
//! An [`EventSpec`] declares a class name + its ts-rs `Payload`. The generator
//! walks the registry and emits the `EventMap` (class → payload) the typed
//! `Events.subscribe`/`emit` infer from, plus a string-free `EventApi`
//! (`api.onContractProposed(...)` / `api.emitContractProposed(...)`) — the event
//! twin of the command accessors.
//!
//! ## Composes with the runtime event registry (no parallel allocator)
//!
//! Events already have a runtime home: `EventClassRegistry` + `EventClassConfig`
//! (channel strategy, schema version, unknown-schema policy). `EventSpec` is the
//! CODEGEN view of an event — class + payload type — NOT a second routing
//! registry. The richer runtime config stays in `EventClassConfig`; tying one
//! `EventSpec` declaration into BOTH the runtime registry and this generator (so
//! a class is declared exactly once) is the convergence follow-up, mirroring how
//! a `CommandSpec` will eventually also drive runtime dispatch.

use ts_rs::{Config, Dependency, TS};

use super::{render_import_lines, TypeRef};

/// The single source of truth for ONE event class — its wire name + the ts-rs
/// `Payload` type subscribers receive / emitters send. Same shape as
/// [`CommandSpec`](super::CommandSpec), for the event primitive.
pub trait EventSpec {
    /// The event class name (e.g. `"contract:proposed"`).
    const CLASS: &'static str;
    /// The typed payload carried by this event.
    type Payload: TS + 'static;
}

/// A runtime-value snapshot of an [`EventSpec`] the generator walks.
#[derive(Debug, Clone)]
pub struct EventDescriptor {
    pub class: &'static str,
    pub payload: TypeRef,
}

impl EventDescriptor {
    /// Snapshot an `EventSpec` into a descriptor (type-level, no instance).
    pub fn of<E: EventSpec>() -> Self {
        let cfg = Config::default();
        let payload_dep = Dependency::from_ty::<E::Payload>(&cfg)
            .expect("event Payload must be a named TS type (struct/enum)");
        Self {
            class: E::CLASS,
            payload: TypeRef::from_dependency(&payload_dep),
        }
    }

    /// The camelCase accessor stem from the class name — `contract:proposed` →
    /// `contractProposed`, `grid:peer:joined` → `gridPeerJoined`. Splits on the
    /// event separators (`:`/`/`/`-`/`_`); the class STRING lives only in the
    /// generated body, never at a call site.
    fn accessor_stem(&self) -> String {
        let mut out = String::new();
        let mut capitalize = false;
        for ch in self.class.chars() {
            match ch {
                ':' | '/' | '-' | '_' => capitalize = true,
                c if capitalize => {
                    out.extend(c.to_uppercase());
                    capitalize = false;
                }
                c => out.push(c),
            }
        }
        // Uppercase the first char for the `on<Stem>` / `emit<Stem>` method names.
        let mut chars = out.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => out,
        }
    }
}

/// Self-registered event (the event twin of `CommandRegistration`).
pub struct EventRegistration {
    descriptor_fn: fn() -> EventDescriptor,
}

impl EventRegistration {
    pub const fn new(descriptor_fn: fn() -> EventDescriptor) -> Self {
        Self { descriptor_fn }
    }
}

inventory::collect!(EventRegistration);

/// Self-register an `EventSpec` into the auto-discovered event registry. ONE line
/// at the event's own declaration site:
/// ```ignore
/// register_event!(ContractProposedEvent);
/// ```
#[macro_export]
macro_rules! register_event {
    ($ev:ty) => {
        inventory::submit! {
            $crate::sdk_codegen::events::EventRegistration::new(
                || $crate::sdk_codegen::events::EventDescriptor::of::<$ev>(),
            )
        }
    };
}

/// The event registry — assembled from every `register_event!` submission,
/// sorted by class for deterministic output, with a hard-fail on duplicates (same
/// anti-drift guarantee as commands).
pub fn event_registry() -> Vec<EventDescriptor> {
    let mut descriptors: Vec<EventDescriptor> = inventory::iter::<EventRegistration>()
        .map(|reg| (reg.descriptor_fn)())
        .collect();
    descriptors.sort_by(|a, b| a.class.cmp(b.class));
    if let Some(dup) = descriptors.windows(2).find(|w| w[0].class == w[1].class) {
        panic!(
            "sdk_codegen: duplicate event CLASS '{}' — two EventSpec impls claim it.",
            dup[0].class
        );
    }
    descriptors
}

/// Emit the TypeScript `EventMap` module — class → payload, the typed surface
/// `Events.subscribe`/`emit` infer from. Mirrors `generate_command_map`.
pub fn generate_event_map(events: &[EventDescriptor], import_base: &str) -> String {
    let refs: Vec<&TypeRef> = events.iter().map(|e| &e.payload).collect();
    let mut out = String::new();
    out.push_str(
        "// GENERATED from the Rust event registry (core/continuum-core sdk_codegen).\n\
         // DO NOT EDIT. Source of truth: each event's EventSpec (class + ts-rs\n\
         // Payload). Regenerate after an event changes.\n\n",
    );
    out.push_str(&render_import_lines(&refs, import_base));
    out.push('\n');
    out.push_str(
        "/** event class -> payload. Generated; the contract is Rust-origin. */\n\
         export interface EventMap {\n",
    );
    for e in events {
        out.push_str(&format!("  '{}': {};\n", e.class, e.payload.name));
    }
    out.push_str("}\n\n");
    out.push_str("export type EventClass = keyof EventMap;\n");
    out
}

/// Emit the typed, string-free `EventApi` — `on<Class>` (subscribe) + `emit<Class>`
/// per event, the event twin of `CommandApi`. The class string lives once, in the
/// generated body; call sites are typed off `EventMap`.
pub fn generate_event_api(
    events: &[EventDescriptor],
    event_map_module: &str,
    events_class_module: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED from the Rust event registry (core/continuum-core sdk_codegen).\n\
         // DO NOT EDIT. Typed accessors — call api.on<Class>/emit<Class>, no string key.\n\n",
    );
    out.push_str(&format!(
        "import {{ Events }} from '{events_class_module}';\n"
    ));
    out.push_str(&format!(
        "import type {{ EventHandlers, SubscribeOptions, Subscription }} from '{events_class_module}';\n"
    ));
    out.push_str(&format!(
        "import type {{ EventMap }} from '{event_map_module}';\n\n"
    ));
    out.push_str(
        "/**\n\
         * Typed event accessors. `onX` subscribes, `emitX` publishes — payloads +\n\
         * handlers typed off the generated EventMap, the class string baked in once.\n\
         */\n\
         export class EventApi {\n  constructor(private readonly events: Events) {}\n",
    );
    for e in events {
        let stem = e.accessor_stem();
        out.push_str(&format!(
            "\n  /** subscribe `{class}` */\n  on{stem}(handlers: EventHandlers<'{class}'>, \
             opts?: SubscribeOptions<'{class}'>): Subscription {{\n    \
             return this.events.subscribe('{class}', handlers, opts);\n  }}\n\
             \n  /** emit `{class}` */\n  emit{stem}(payload: EventMap['{class}']): Promise<void> {{\n    \
             return this.events.emit('{class}', payload);\n  }}\n",
            class = e.class,
            stem = stem,
        ));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: events are Rust-sourced end-to-end like commands — the
    // EventMap generates from registered EventSpecs against REAL ts-rs payloads
    // (the contract events), imports resolve to clean modules (no escape paths).
    #[test]
    fn generates_event_map_from_the_real_registry() {
        let registry = event_registry();
        assert!(!registry.is_empty(), "contract events are registered");
        let out = generate_event_map(&registry, "@protocol");

        assert!(
            out.contains("'contract:proposed': ContractProposedPayload;"),
            "real event keyed by class with its ts-rs payload:\n{out}"
        );
        assert!(out.contains("import type {"), "imports emitted");
        assert!(
            !out.contains("../../../"),
            "no ts-rs export escape path leaks"
        );
        assert!(
            out.contains("from '@protocol/contracts/"),
            "payload imports from its clean module under the TS root:\n{out}"
        );
        assert!(out.contains("export interface EventMap"));
        assert!(out.contains("export type EventClass = keyof EventMap;"));
    }

    // what this catches: the registry self-assembles from register_event! sites
    // (inventory) crate-wide, sorted for deterministic output.
    #[test]
    fn event_registry_is_auto_discovered_and_sorted() {
        let classes: Vec<&str> = event_registry().iter().map(|e| e.class).collect();
        assert!(classes.contains(&"contract:proposed"));
        assert!(classes.contains(&"contract:paid"));
        let mut sorted = classes.clone();
        sorted.sort();
        assert_eq!(classes, sorted, "registry sorted by class");
    }

    // what this catches: the typed string-free EventApi — on<Class>/emit<Class>
    // per event, class string only in the body, payload/handlers typed off EventMap.
    #[test]
    fn event_api_emits_typed_string_free_accessors() {
        let registry = event_registry();
        let out = generate_event_api(&registry, "./EventMap", "../Events");

        assert!(
            out.contains(
                "onContractProposed(handlers: EventHandlers<'contract:proposed'>, \
                 opts?: SubscribeOptions<'contract:proposed'>): Subscription {"
            ),
            "typed subscribe accessor, class camelCased:\n{out}"
        );
        assert!(
            out.contains(
                "emitContractProposed(payload: EventMap['contract:proposed']): Promise<void> {"
            ),
            "typed emit accessor:\n{out}"
        );
        assert!(
            out.contains("return this.events.subscribe('contract:proposed', handlers, opts);"),
            "class string lives once, in the body:\n{out}"
        );
        assert!(out.contains("import { Events } from '../Events';"));
        assert!(!out.contains("../../../"), "no escape path leaks");
    }
}
