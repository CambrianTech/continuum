//! continuum-client — shared client library for the continuum substrate.
//!
//! Sits between the substrate (`core/continuum-core`) and every embodiment
//! that talks to it: the CLI (`apps/cli`), the language SDKs
//! (`sdk/{flutter,swift,kotlin}` via FFI), and any future apps. One Rust
//! crate, N language frontends — same connection / command / event API
//! everywhere.

pub mod airc_ipc;
pub mod command;
pub mod connection;
pub mod error;
pub mod event;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod mock;
pub mod transport;

pub use airc_ipc::AircIpcTransport;
pub use command::CommandClient;
pub use connection::Connection;
pub use error::ClientError;
pub use event::EventSubscriber;
#[cfg(any(test, feature = "test-fixtures"))]
pub use mock::MockTransport;
pub use transport::Transport;
