//! Windows: a named pipe whose name is derived from the path.

use std::io;
use std::path::Path;

use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions};

pub(super) type ServerStream = NamedPipeServer;
pub(super) type ClientStream = NamedPipeClient;

pub(super) struct Listener {
    name: String,
    next: NamedPipeServer,
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

pub(super) fn bind(path: &Path) -> io::Result<Listener> {
    let name = pipe_name(path);
    let next = ServerOptions::new().first_pipe_instance(true).create(&name)?;
    Ok(Listener { name, next })
}

pub(super) async fn connect(path: &Path) -> io::Result<ClientStream> {
    ClientOptions::new().open(pipe_name(path))
}

pub(super) fn remove(_path: &Path) {
    // A named pipe leaves nothing on disk: it ends with its last handle.
}
