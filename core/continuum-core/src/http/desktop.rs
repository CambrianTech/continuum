//! The desktop display manager — the core serves its own UI, always current.
//!
//! Joel, 2026-08-30, after a stale preview server rendered a broken sidebar:
//! "this should have worked. Should work like a Display Manager (such as GDM,
//! LightDM, or SDDM)." The session layer must be ALWAYS THERE, always serving
//! the current build, with browsers attaching and detaching freely — never a
//! hand-run script racing a hand-run server.
//!
//! So the core serves the built web desktop statically:
//! - `CONTINUUM_UI_DIST` names the dist directory (the start script builds it
//!   and exports the path); absent/missing = the display manager stays off
//!   with a probe naming the fix — an honestly-disabled projection, never a
//!   broken one.
//! - `CONTINUUM_UI_PORT` (default 8975) on 127.0.0.1 — beside the WS ingress
//!   (8974) and the call plane (8790). One `continuum start` brings up all
//!   three; `continuum desktop` opens the door.
//! - The zero-config client needs no query params: it defaults to the core's
//!   own WS port and mints its identity.
//!
//! SPA fallback: unknown paths serve index.html (client-side routing), real
//! files serve as themselves. Served from disk per request — a redeploy that
//! rewrites dist is picked up on the next load, no server restart, and the
//! stale-index race dies because index.html and its hashed bundles come from
//! the same generation of the directory on every full reload.

use std::path::PathBuf;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// Spawn the display manager if a dist directory is configured and present.
/// Detached; failures probe loudly and never block boot.
pub fn spawn_if_configured(rt: &tokio::runtime::Handle) {
    let Some(dist) = crate::config_env::read("CONTINUUM_UI_DIST").map(PathBuf::from) else {
        crate::probe!(
            class = "desktop.dm.unconfigured",
            "no CONTINUUM_UI_DIST — the core serves no desktop this boot \
             (continuum start sets it after building the web client)"
        );
        return;
    };
    if !dist.join("index.html").is_file() {
        crate::probe!(
            class = "desktop.dm.dist_missing",
            dist = %dist.display(),
            "CONTINUUM_UI_DIST has no index.html — build the web client \
             (npm run build -w @continuum/web) or re-run continuum start"
        );
        return;
    }
    let port: u16 = crate::config_env::read("CONTINUUM_UI_PORT")
        .and_then(|v| v.parse().ok())
        .unwrap_or(8975); // unwrap_or: the documented default port, beside WS 8974
    let index = dist.join("index.html");
    rt.spawn(async move {
        let app = Router::new().fallback_service(
            ServeDir::new(&dist).fallback(ServeFile::new(index)),
        );
        let bind = format!("127.0.0.1:{port}");
        let listener = match tokio::net::TcpListener::bind(&bind).await {
            Ok(l) => l,
            Err(e) => {
                crate::probe!(
                    class = "desktop.dm.bind_failed",
                    bind = %bind,
                    error = %e,
                    "desktop display manager could not bind — another server holds the port"
                );
                return;
            }
        };
        crate::probe!(
            class = "desktop.dm.online",
            url = %format!("http://{bind}/"),
            dist = %dist.display(),
            "desktop display manager online — the core serves its own UI (continuum desktop opens it)"
        );
        if let Err(e) = axum::serve(listener, app).await {
            crate::probe!(
                class = "desktop.dm.exited",
                error = %e,
                "desktop display manager exited"
            );
        }
    });
}
