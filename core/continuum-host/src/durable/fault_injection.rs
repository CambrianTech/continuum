//! Injected faults for tests of code that fences through [`super::fence`].
//! Compiled only for tests and the `test-fixtures` feature, never into a release binary.

use std::cell::Cell;

thread_local! {
    static FAIL_FENCE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn fence_fails() -> bool {
    FAIL_FENCE.with(Cell::get)
}

/// Runs `f` with every durability fence on this thread failing, then restores.
pub fn with_failed_fence(f: impl FnOnce()) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL_FENCE.with(|fail| fail.set(false));
        }
    }
    FAIL_FENCE.with(|fail| fail.set(true));
    let _reset = Reset;
    f();
}
