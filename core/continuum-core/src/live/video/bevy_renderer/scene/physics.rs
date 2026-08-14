//! Physics adapter seam.
//!
//! Physics is a *designed-for* boundary: the scene graph can carry rigid bodies
//! + colliders as data ([`super::description::PhysicsDesc`]) so a very complex
//! scene is expressible, but the base engine ships **zero** physics simulation.
//! A `PhysicsBackend` is the adapter a real engine (Rapier/Avian) would
//! implement; the default [`NoopPhysicsBackend`] does nothing, so physics data
//! is inert — never silently half-simulated — until a backend is attached.
//!
//! This is the polymorphism-over-enums doctrine again: one trait, one impl now
//! (the honest no-op outlier), and adding real physics = one more `impl
//! PhysicsBackend` + one dependency, with zero edits to the scene graph, the
//! instantiate seam, or the description data.

use bevy::prelude::*;

use super::description::PhysicsDesc;

/// The adapter every physics engine implements to bind scene-graph physics data
/// onto live entities. `&self` (stateless): a real engine keeps its simulation
/// state in the ECS world / its own resource, inserting components via
/// `commands` — the trait only decides *what* to attach, not *where* state
/// lives.
pub trait PhysicsBackend: Send + Sync {
    /// Stable id for logging / selection.
    fn id(&self) -> &str;

    /// Bind a rigid body + collider onto an already-spawned entity. Called by
    /// [`super::instantiate`] for every node carrying a `PhysicsDesc`.
    fn attach(
        &self,
        commands: &mut Commands,
        entity: Entity,
        transform: &Transform,
        physics: &PhysicsDesc,
    );

    /// Advance the simulation by `dt` seconds. No-op for backends that don't
    /// simulate (the base engine).
    fn step(&self, dt: f32);
}

/// The honest no-op outlier: acknowledges the seam, simulates nothing. Physics
/// data on nodes is carried through instantiation and deliberately ignored, so
/// the base engine has zero physics dependency and a complex scene's physics
/// intent survives round-trips waiting for a real backend.
pub struct NoopPhysicsBackend;

impl PhysicsBackend for NoopPhysicsBackend {
    fn id(&self) -> &str {
        "noop"
    }

    fn attach(
        &self,
        _commands: &mut Commands,
        _entity: Entity,
        _transform: &Transform,
        _physics: &PhysicsDesc,
    ) {
        // Intentionally inert — the base engine does not simulate physics.
    }

    fn step(&self, _dt: f32) {}
}

/// Bevy resource holding the active physics backend. Defaults to the no-op so
/// the base app always has a backend to call without a `None` check.
#[derive(Resource)]
pub struct PhysicsBackendRegistry {
    backend: Box<dyn PhysicsBackend>,
}

impl Default for PhysicsBackendRegistry {
    fn default() -> Self {
        Self {
            backend: Box::new(NoopPhysicsBackend),
        }
    }
}

impl PhysicsBackendRegistry {
    /// Install a real physics backend (replacing the no-op).
    pub fn set(&mut self, backend: Box<dyn PhysicsBackend>) {
        self.backend = backend;
    }

    /// The active backend.
    pub fn backend(&self) -> &dyn PhysicsBackend {
        self.backend.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the default registry regressing away from a present
    // (no-op) backend to something that would force a None-check at every
    // instantiate call site.
    #[test]
    fn default_registry_has_noop_backend() {
        let reg = PhysicsBackendRegistry::default();
        assert_eq!(reg.backend().id(), "noop");
    }
}
