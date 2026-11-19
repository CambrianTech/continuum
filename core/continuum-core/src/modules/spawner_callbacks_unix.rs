//! Unix callbacks for deploy spawning — stubs, since no probe or permit is needed on this platform.

use std::path::Path;

/// Returns stubbed probe and permit functions (no-op on Unix).
pub fn make() -> (continuum_host::deploy_spawn::ProbeFn, continuum_host::deploy_spawn::PermitCheck) {
    let probe: continuum_host::deploy_spawn::ProbeFn = |_| None;
    let permit: continuum_host::deploy_spawn::PermitCheck = |_| Ok(());
    (probe, permit)
}
