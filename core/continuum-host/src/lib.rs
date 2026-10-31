//! The host OS seam. Each module is one capability whose behaviour differs by OS. Its `mod.rs`
//! holds the contract and the logic once; its `unix.rs` and `windows.rs` hold only the calls that
//! differ, chosen once at the module boundary. Nothing without an OS difference belongs here, and
//! no code outside this crate branches on the OS for these capabilities.

pub mod confine;
pub mod durable;
pub mod file_identity;
pub mod local_stream;
pub mod output_handoff;
pub mod positioned;
pub mod process_exit;
pub mod stack_sample;
