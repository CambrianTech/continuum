//! `command/*` — the generators that write commands.
//!
//! The consolidation directive (Joel 2026-06-25): *"if you're clever you write
//! tools/commands to write commands simply via a generator."* These commands ARE
//! that automation, dogfooding the very framework they emit onto:
//!
//! - [`new`] — `command/new` scaffolds a fresh command file on `action_command!`.
//! - [`migrate`] — `command/migrate` ports a legacy `handle_command` match arm into
//!   the same scaffold, transplanting the body so the bulk collapse is a fast crank.
//!
//! Both are stateless commands (they only touch the source tree), self-registering
//! with zero module ceremony. The shared internals — [`ident`] (name → identifiers),
//! [`scaffold`] (pure file rendering), [`wiring`] (the filesystem + mod-tree edits) —
//! are split so the rendering is unit-tested without disk and the effects live in one
//! place. See docs/architecture/COMMAND-ORGANIZATION.md.

pub mod ident;
pub mod migrate;
pub mod new;
pub mod scaffold;
pub mod wiring;
