//! [`LateBound<T>`] — substrate-canonical install-once-at-boot,
//! read-many-at-runtime dependency injection slot.
//!
//! ## What it's for
//!
//! Post-#224 (`GLOBAL_EXECUTOR` deletion), every module that needs to
//! dispatch commands stores a `std::sync::OnceLock<Arc<CommandExecutor>>`
//! and overrides the `ServiceModule::install_executor` trait method
//! to populate it. Seven modules do this verbatim today (cognition,
//! channel, persona_instance_manager, training_trigger, sentinel,
//! ai_provider, plus the trait reference in service_module). Each
//! one writes:
//!
//! ```ignore
//! // field
//! executor: std::sync::OnceLock<Arc<CommandExecutor>>,
//!
//! // install
//! fn install_executor(&self, executor: Arc<CommandExecutor>) {
//!     let _ = self.executor.set(executor);
//! }
//!
//! // accessor in every handler that needs it
//! self.executor.get().ok_or_else(|| "X: not installed yet".to_string())
//! ```
//!
//! The pattern is correct but the boilerplate is begging for a typed
//! wrapper. `LateBound<T>` collapses it.
//!
//! ## Why it's not just `OnceLock<Arc<T>>`
//!
//! Three improvements over raw `OnceLock`:
//!
//! 1. **Type compression.** Callers write `LateBound<CommandExecutor>`
//!    not `OnceLock<Arc<CommandExecutor>>` — the `Arc` is implied by
//!    the "late-bound shared dependency" semantics.
//! 2. **Uniform error message.** `.require()` produces a consistent
//!    `"<slot_name>: dependency not installed yet"` error so operators
//!    grepping logs see one shape across every module.
//! 3. **`.cloned()` convenience.** Modules that spawn tasks (sentinel,
//!    channel) need to move the Arc into the spawn. Today they write
//!    `self.executor.get().cloned()`; the primitive exposes
//!    `.cloned()` directly.
//!
//! ## What it doesn't do
//!
//! - **No lazy init.** `install` is explicit, called at boot by the
//!   runtime. Read-side never creates the value on first miss —
//!   that's `OnceCell::get_or_init` and a different shape.
//! - **No re-install.** Following the existing `let _ = .set(x)`
//!   pattern, `install` silently ignores a second install. Operators
//!   shouldn't try to swap dependencies at runtime — if they do, the
//!   bug surfaces as "old value still in use," not as a panic.
//!   Tests can use `is_installed()` to assert idempotency.
//! - **No `Default` impl.** Every `LateBound<T>` needs a name so
//!   error messages are diagnostic. Forcing the name at construction
//!   prevents the "<unnamed slot>: not installed" confusion.

use std::sync::{Arc, OnceLock};

/// Install-once, read-many slot for a shared dependency.
///
/// `T` is the dependency type (not `Arc<T>`); `LateBound` handles the
/// Arc internally so callers don't repeat it.
pub struct LateBound<T> {
    slot: OnceLock<Arc<T>>,
    name: &'static str,
}

impl<T> LateBound<T> {
    /// Create an empty slot. `name` appears in `.require()` error
    /// messages — pick something diagnostic, typically the module
    /// name plus the dependency kind (e.g. `"sentinel::executor"`).
    pub const fn new(name: &'static str) -> Self {
        Self {
            slot: OnceLock::new(),
            name,
        }
    }

    /// Install the dependency. Silently ignores a second install
    /// (matches the existing `let _ = .set(x)` pattern across the
    /// substrate). Use `is_installed()` before to assert idempotency
    /// in tests if needed.
    pub fn install(&self, value: Arc<T>) {
        let _ = self.slot.set(value);
    }

    /// Borrow the installed dependency, or produce a diagnostic
    /// error. Preferred shape — uniform error message across modules.
    pub fn require(&self) -> Result<&Arc<T>, String> {
        self.slot
            .get()
            .ok_or_else(|| format!("{}: dependency not installed yet", self.name))
    }

    /// Borrow the installed dependency, returning `None` if not yet
    /// installed. Use for "soft" call sites where the absence is a
    /// legitimate state (e.g. boot-time before install runs).
    pub fn get(&self) -> Option<&Arc<T>> {
        self.slot.get()
    }

    /// Clone the installed Arc, returning `None` if not yet
    /// installed. Use when moving the dependency into a spawned task
    /// (`let exec = self.executor.cloned();` then `tokio::spawn(...)`).
    pub fn cloned(&self) -> Option<Arc<T>> {
        self.slot.get().cloned()
    }

    /// `true` iff the dependency has been installed.
    pub fn is_installed(&self) -> bool {
        self.slot.get().is_some()
    }

    /// The name passed to `new()`. Useful for diagnostic logging
    /// without re-deriving it from the type.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// Compile-time pin: `LateBound<T>` is `Send + Sync` whenever the
/// underlying `OnceLock<Arc<T>>` is. Substrate modules are shared
/// across the dispatch tasks, so a regression that broke this would
/// surface as cryptic trait-bound errors at every call site. Pinning
/// it here makes the contract explicit and defends against future
/// refactors (e.g. swapping the OnceLock for a non-Sync cell).
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LateBound<()>>();
    // Send-but-not-Sync inner T is OK because we only ever expose
    // `&Arc<T>` to callers — Arc<T>: Sync requires T: Send + Sync
    // already, so this is conservative.
    assert_send_sync::<LateBound<crate::runtime::CommandExecutor>>();
};

impl<T> std::fmt::Debug for LateBound<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LateBound")
            .field("name", &self.name)
            .field("installed", &self.is_installed())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Dep {
        value: u32,
    }

    // what this catches: get/require return None/Err before install.
    // A regression that pre-filled the slot with a default would
    // silently mask "module command runs before runtime installs the
    // executor" bugs — the bug class GLOBAL_EXECUTOR's deletion
    // (#224) was meant to surface.
    #[test]
    fn empty_slot_returns_none_and_diagnostic_error() {
        let lb: LateBound<Dep> = LateBound::new("test::dep");
        assert!(!lb.is_installed());
        assert!(lb.get().is_none());
        assert!(lb.cloned().is_none());
        let err = lb.require().unwrap_err();
        assert!(err.contains("test::dep"), "error must name the slot: {err}");
        assert!(
            err.contains("not installed"),
            "error must say 'not installed': {err}"
        );
    }

    // what this catches: install populates the slot and subsequent
    // get/require/cloned all return the SAME Arc. Arc::ptr_eq proves
    // we're not accidentally cloning the inner T into a fresh Arc.
    #[test]
    fn install_then_accessors_return_same_arc() {
        let lb: LateBound<Dep> = LateBound::new("test::dep");
        let dep = Arc::new(Dep { value: 42 });
        lb.install(dep.clone());

        assert!(lb.is_installed());
        let got = lb.get().expect("installed");
        assert!(
            Arc::ptr_eq(got, &dep),
            "get() must return the same Arc that was installed"
        );
        assert_eq!(got.value, 42);

        let required = lb.require().expect("installed");
        assert!(Arc::ptr_eq(required, &dep));

        let cloned = lb.cloned().expect("installed");
        assert!(Arc::ptr_eq(&cloned, &dep));
    }

    // what this catches: second install silently no-ops (matching
    // the substrate's existing `let _ = self.executor.set(x)`
    // pattern). The ORIGINAL install wins. A regression that swapped
    // the slot would change observed dependencies mid-flight, which
    // is exactly the bug we don't want — substrate dependencies are
    // boot-once.
    #[test]
    fn second_install_is_silent_noop_original_wins() {
        let lb: LateBound<Dep> = LateBound::new("test::dep");
        let first = Arc::new(Dep { value: 1 });
        let second = Arc::new(Dep { value: 2 });
        lb.install(first.clone());
        lb.install(second);

        let got = lb.get().expect("installed");
        assert!(
            Arc::ptr_eq(got, &first),
            "first install must win; second is silently ignored"
        );
        assert_eq!(got.value, 1);
    }

    // what this catches: name() returns the value passed to new().
    // Trivial but pinning the public API.
    #[test]
    fn name_is_preserved() {
        let lb: LateBound<Dep> = LateBound::new("my::module::executor");
        assert_eq!(lb.name(), "my::module::executor");
    }

    // what this catches: Debug impl includes the name AND the
    // installed flag. Operators reading log dumps need both to
    // diagnose "which slot, populated or not?"
    #[test]
    fn debug_includes_name_and_installed_flag() {
        let lb: LateBound<Dep> = LateBound::new("test::dep");
        let s = format!("{lb:?}");
        assert!(s.contains("test::dep"), "Debug must include name: {s}");
        assert!(
            s.contains("installed: false"),
            "Debug must include install state: {s}"
        );

        lb.install(Arc::new(Dep { value: 0 }));
        let s = format!("{lb:?}");
        assert!(
            s.contains("installed: true"),
            "Debug must reflect install: {s}"
        );
    }
}
