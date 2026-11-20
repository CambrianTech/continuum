//! A fake engine served at a root no other fixture can share.
//!
//! The slot directory, its pools and endpoints are process-wide and keyed by an engine's root.
//! A fixture rooted at a bare `http://127.0.0.1:{port}` hands its state (a suspended endpoint,
//! an adopted ledger, a resident) to whichever later test the OS gives the same port, so a test
//! could pass or fail on what another test left behind (#6042 CI, 2026-11-16: a checkpoint that
//! never sent its save). Every client URL is built as `{root}/...`, so serving the app under a
//! path unique to this fixture keeps each root its own, whatever port it gets.
#![cfg(any(test, feature = "test-fixtures"))]

/// A fake engine: its root is unique to it, its port is the one the OS gave it, and dropping it
/// stops serving.
pub struct FixtureEngine {
    pub root: String,
    pub address: std::net::SocketAddr,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl FixtureEngine {
    /// Serve `app` (routed as an engine is: `/props`, `/slots/{slot}`, ...) under a root no other
    /// fixture in this process can name.
    pub async fn serve(app: axum::Router) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let prefix = format!("/fixture-{}", uuid::Uuid::new_v4());
        let root = format!("http://{address}{prefix}");
        let server = tokio::spawn(async move { axum::serve(listener, axum::Router::new().nest(&prefix, app)).await });
        Ok(Self { root, address, server })
    }
}

impl Drop for FixtureEngine {
    fn drop(&mut self) {
        self.server.abort();
    }
}
