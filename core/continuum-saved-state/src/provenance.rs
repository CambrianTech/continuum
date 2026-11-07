//! Which build wrote a saved value: recorded in every envelope as provenance, never compared. The
//! binary names its build once at boot; a process that never does (a test) records none.

use std::sync::OnceLock;

static BUILD: OnceLock<&'static str> = OnceLock::new();

/// Name the build this process runs (the core's `CONTINUUM_BUILD_GIT_SHA`), once, at boot. A
/// second call keeps the first: a process runs one build.
pub fn record_build(sha: &'static str) {
    if BUILD.set(sha).is_err() {
        continuum_base::probe!(class = "state.build_already_recorded", sha, "the build is recorded once per process; the first stands");
    }
}

pub(crate) fn build() -> Option<&'static str> {
    BUILD.get().copied()
}
