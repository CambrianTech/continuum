//! `auth/<...>` — the external-browser OAuth surface as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//!
//! The OAuth verbs live under the `oauth/` subnamespace so the file tree mirrors
//! the wire name (`commands/auth/oauth/start.rs` ⟺ `auth/oauth/start`).

pub mod oauth;

pub use oauth::command_objects;
