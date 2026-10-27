//! Windows: a named pipe whose name is derived from the path. A live owner keeps the name
//! (`first_pipe_instance` refuses a second server), and the pipe ends with its last handle.

use std::io;
use std::path::Path;

use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};

pub(super) type ServerStream = NamedPipeServer;
pub(super) type ClientStream = NamedPipeClient;

pub(super) struct Listener {
    name: String,
    next: NamedPipeServer,
}

/// Nothing on disk to own: the pipe ends with its last handle.
pub(super) struct Owned;

impl Owned {
    pub(super) fn remove(self) {}
}

/// The pipe name for `path`: its characters that are not alphanumeric become `_`.
fn pipe_name(path: &Path) -> String {
    let stem: String = path.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!(r"\\.\pipe\continuum{stem}")
}

impl Listener {
    pub(super) async fn accept(&mut self) -> io::Result<ServerStream> {
        self.next.connect().await?;
        let fresh = ServerOptions::new().create(&self.name)?;
        Ok(std::mem::replace(&mut self.next, fresh))
    }
}

pub(super) async fn bind(path: &Path) -> io::Result<(Listener, Owned)> {
    let name = pipe_name(path);
    let next = ServerOptions::new().first_pipe_instance(true).create(&name)?;
    Ok((Listener { name, next }, Owned))
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    ClientOptions::new().open(pipe_name(path))
}
