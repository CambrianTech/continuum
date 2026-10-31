//! The upper half of the test environment: the crate's own source, read by the tests that
//! guard its shape, and the tests that exercise the kernel TOGETHER with a module above it
//! (they cannot live inside a kernel file: the kernel crate has nothing above it to name).
//! The home lock moved down to `paths::test_home` with the kernel tests that need it.
pub(crate) mod source_tree;
mod kernel_integration;
mod grid_acl;
mod mind_integration;
mod registry_contract;
mod loose_id_guard;
mod module_wiring_guard;
