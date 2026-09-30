//! The `continuum` CLI's platform lifecycle: installing itself, registering a
//! service, launching and owning engine processes, and tearing them down.
//!
//! # Why this is a crate and not a module tree in the bin
//!
//! As `mod`s inside the bin, these tests linked all of `continuum-core` —
//! 624,456 lines — to exercise 2,420 lines of pure functions. On Windows the
//! resulting PDB exceeds the linker's limit (LNK1140) and they cannot run at all.
//!
//! Nothing here reaches into the core, and nothing here may. A dependency on
//! `continuum-core` restores the link that made these tests unrunnable, and it
//! does so silently: they would still pass locally and only Windows would notice.
//!
//! Platform-specific modules are gated HERE, at the crate boundary, not inside
//! the file — a module whose imports are OS-specific must not be exported on
//! an OS that lacks them, or the crate fails to build for every consumer.

/// The teardown DECISION — which process may be ended, in what order, and what a
/// refusal means. Pure, so it runs on every machine rather than only the one that
/// cannot link it.
pub mod elevated_teardown;
pub mod install_cli;
pub mod launchd;
pub mod owned_engines;
pub mod supervisor_install;

/// Windows process launch and ownership. Imports `std::os::windows` and
/// `windows_sys`, so it exists only on Windows.
#[cfg(windows)]
pub mod windows_launch;
