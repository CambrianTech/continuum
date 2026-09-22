//! The `continuum` CLI's platform lifecycle: installing itself, registering a
//! service, launching and owning engine processes, and tearing them down.
//!
//! # Why this is a crate and not a module tree in the bin
//!
//! It was the latter until 2026-09-22. Being `mod`s inside the bin meant
//! `cargo test --bin continuum` linked the whole of `continuum-core` — 624,456
//! lines — to exercise 17 tests over 2,420 lines of pure functions. On Windows
//! the resulting PDB blew the linker's limit (LNK1140) and the tests could not
//! run at all.
//!
//! The extraction is not a refactor of the code: every module moved verbatim.
//! All five already had zero `crate::` / `continuum_core::` imports and zero
//! `super::` references outside their own test mods, so the boundary existed by
//! convention. This crate makes the compiler enforce it.
//!
//! **Keep it that way.** A dependency on `continuum-core` here restores the
//! link that made the tests unrunnable, and it would do so silently — the tests
//! would still pass locally and only Windows would notice.

pub mod install_cli;
pub mod launchd;
pub mod owned_engines;
pub mod supervisor_install;
// WINDOWS ONLY, exactly as the bin gated it. This module imports
// `std::os::windows` and `windows_sys`; exporting it unconditionally makes the
// crate fail to build on macOS and Linux. The `#[cfg(windows)]` that used to sit
// on `mod windows_launch;` in the bin has to move WITH the module — caught in
// review by Astra, who read the file's imports rather than the diff.
#[cfg(windows)]
pub mod windows_launch;
