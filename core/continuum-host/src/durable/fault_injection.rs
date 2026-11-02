//! Test-only durability faults, explicitly carried across blocking-worker boundaries.
use std::cell::Cell;

thread_local! {
    static FAIL_FENCE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn fence_fails() -> bool {
    FAIL_FENCE.with(Cell::get)
}

/// Captured fixture state. Capturing alone does not affect another thread.
#[derive(Clone, Copy)]
pub struct FenceFault(bool);

impl FenceFault {
    pub fn capture() -> Self {
        Self(fence_fails())
    }

    /// Install this fixture only for the operation and restore the previous state,
    /// including on unwind. Nested fixture scopes cannot clear an outer fault.
    pub fn run<R>(self, operation: impl FnOnce() -> R) -> R {
        struct Reset(bool);
        impl Drop for Reset {
            fn drop(&mut self) {
                FAIL_FENCE.with(|fault| fault.set(self.0));
            }
        }
        let previous = FAIL_FENCE.with(|fault| fault.replace(self.0));
        let _reset = Reset(previous);
        operation()
    }
}

pub fn with_failed_fence<R>(operation: impl FnOnce() -> R) -> R {
    FenceFault(true).run(operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_scope_restores_the_outer_fault() {
        assert!(!fence_fails());
        with_failed_fence(|| {
            with_failed_fence(|| assert!(fence_fails()));
            assert!(fence_fails());
        });
        assert!(!fence_fails());
    }

    #[test]
    fn captured_fault_is_explicit_and_scoped_on_worker() -> Result<(), String> {
        let captured = with_failed_fence(FenceFault::capture);
        let worker = std::thread::spawn(move || {
            assert!(!fence_fails());
            captured.run(|| assert!(fence_fails()));
            assert!(!fence_fails());
        });
        worker
            .join()
            .map_err(|_| "fault fixture worker panicked".to_string())?;
        assert!(!fence_fails());
        Ok(())
    }
}
