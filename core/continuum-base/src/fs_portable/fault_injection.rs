//! Injected faults for tests of code that publishes through [`super::sync_publication`].
//! Compiled only for tests and the `test-fixtures` feature, never into a release binary.

use std::cell::Cell;

thread_local! {
    static FAIL_PUBLICATION_FENCE: Cell<bool> = const { Cell::new(false) };
}

/// Whether the current thread asked the next publication fence to fail.
pub(super) fn publication_fence_fails() -> bool {
    FAIL_PUBLICATION_FENCE.with(Cell::get)
}

/// Run `f` with every publication fence on this thread failing, then restore.
pub fn with_failed_publication_fence(f: impl FnOnce()) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAIL_PUBLICATION_FENCE.with(|fail| fail.set(false));
        }
    }
    FAIL_PUBLICATION_FENCE.with(|fail| fail.set(true));
    let _reset = Reset;
    f();
}
