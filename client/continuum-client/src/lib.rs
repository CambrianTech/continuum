//! continuum-client — shared client library for the continuum substrate.
//!
//! Sits between the substrate (`core/continuum-core`) and every embodiment
//! that talks to it: the CLI (`apps/cli`), the language SDKs
//! (`sdk/{flutter,swift,kotlin}` via FFI), and any future apps. One Rust
//! crate, N language frontends — same connection / command / event API
//! everywhere.

pub mod command;
pub mod connection;
pub mod error;
pub mod event;
pub mod transport;

pub use command::CommandClient;
pub use connection::Connection;
pub use error::ClientError;
pub use event::EventSubscriber;
pub use transport::Transport;
