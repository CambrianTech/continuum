//! `generate/*` — the command scaffolding generator.
//!
//! Dep-holding: the stateful [`GeneratorEngine`](crate::modules::generator::GeneratorEngine)
//! (workspace root + per-name locks) lives on
//! [`GeneratorModule`](crate::modules::generator::GeneratorModule); this command
//! captures an `Arc` of it so concurrent `generate/module` calls serialize on the
//! SAME name locks. The module's `commands()` constructs the runtime object with
//! that shared engine.

pub mod module;
