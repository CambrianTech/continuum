//! The host OS seam. Each module is one capability whose behaviour differs by OS. Its `mod.rs`
//! holds the contract and the logic once; its `unix.rs` and `windows.rs` hold only the calls that
//! differ, chosen once at the module boundary. Nothing without an OS difference belongs here, and
//! no code outside this crate branches on the OS for these capabilities.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub mod browser_install;
pub mod confine;
pub mod core_handoff;
pub mod core_ipc;
pub mod durable;
pub mod file_identity;
pub mod local_stream;
pub mod output_handoff;
pub mod own_group;
pub mod positioned;
pub mod process_class;
pub mod process_exit;
pub mod process_presence;
pub mod process_usage;
pub mod read_pin;
pub mod sandbox;
pub mod shutdown_signal;
pub mod stack_sample;
