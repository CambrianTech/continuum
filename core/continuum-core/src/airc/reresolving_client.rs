//! A daemon client that re-resolves the airc socket when the daemon moves.
//!
//! ### The defect this closes (#3849)
//!
//! The daemon socket was resolved exactly ONCE, at boot, by
//! [`crate::airc::discover_airc_socket`]. `DaemonClient` holds only a
//! path and dials FRESH on every call, so there is no stale connection
//! to blame — but if the daemon was not reachable at that single boot
//! moment, the core kept a path that never worked again for the whole
//! process lifetime.
//!
//! Measured on BigMama 2026-09-07: the airc daemon died overnight, the
//! core booted without it, and `grid-capacity` offer publishing then
//! failed 1,216 CONSECUTIVE times over seven hours — including the five
//! hours the daemon was back up and its pipe verifiably accepted
//! connections. The node was not idle, it was UNADVERTISED: no capacity
//! offer reaches the grid, so no work is routed to it, while every
//! health surface reads fine. Restarting the core with the daemon live
//! took the failure rate to zero.
//!
//! ### Why this is not a fallback ([[no-fallbacks-ever]])
//!
//! `modules::airc::from_discovery` refuses to build a daemon transport
//! against state discovery has declared not-Healthy, precisely so the
//! substrate never "looks healthy" against a stale socket. This wrapper
//! does NOT reintroduce that: it never guesses a path and never
//! substitutes a degraded stand-in. On an unreachable error it re-asks
//! the SAME authoritative source discovery used (`airc ipc-endpoint`)
//! and retries once. If that re-ask fails, the original error is
//! returned with the re-resolution failure appended — loud, not masked.
//! Errors that are not reachability failures are returned untouched and
//! never trigger a re-resolve.
//!
//! Reachability is decided by MATCHING [`DaemonCallError::Unreachable`],
//! a variant classified where `airc_ipc::ClientError` is still typed —
//! never by substring-matching a message. airc ships on its own cadence,
//! so a reworded string would otherwise silently disable this recovery
//! and re-open #3849 with every test in both repos still green
//! ([[strong-typing-across-boundaries]]). Reviewed by IntelMac on #3850,
//! who caught exactly that door being left open.

use std::path::PathBuf;
use std::sync::Arc;

use airc_ipc::{DaemonClient, InboxRequest, InboxResponse, PublishRequest, PublishResponse};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::airc::daemon_transport::{AircDaemonClient, DaemonCallError};

/// Resolves the airc daemon's current socket path.
///
/// Exists so the retry path is testable without shelling out to
/// `airc ipc-endpoint`.
#[async_trait]
pub trait DaemonSocketResolver: Send + Sync {
    async fn resolve(&self) -> Result<PathBuf, String>;
}

/// Production resolver — asks airc itself, the same authoritative
/// source boot discovery uses. Never derives a path locally.
pub struct DiscoverySocketResolver;

#[async_trait]
impl DaemonSocketResolver for DiscoverySocketResolver {
    async fn resolve(&self) -> Result<PathBuf, String> {
        crate::airc::discover_airc_socket()
            .await
            .map_err(|error| error.to_string())
    }
}

/// Builds a client for a freshly resolved socket path.
pub type DaemonClientFactory = fn(PathBuf) -> Arc<dyn AircDaemonClient>;

/// The production factory: a real `airc_ipc::DaemonClient`.
pub fn live_daemon_client(socket: PathBuf) -> Arc<dyn AircDaemonClient> {
    Arc::new(DaemonClient::new(socket))
}

/// Wraps a daemon client so a reachability failure re-resolves the
/// socket and retries once, instead of failing forever.
pub struct ReresolvingDaemonClient {
    current: RwLock<Arc<dyn AircDaemonClient>>,
    resolver: Arc<dyn DaemonSocketResolver>,
    factory: DaemonClientFactory,
}

impl ReresolvingDaemonClient {
    pub fn new(
        socket: PathBuf,
        resolver: Arc<dyn DaemonSocketResolver>,
        factory: DaemonClientFactory,
    ) -> Self {
        Self {
            current: RwLock::new(factory(socket)),
            resolver,
            factory,
        }
    }

    /// The wiring used at boot: a real daemon client that re-resolves
    /// through `airc ipc-endpoint`.
    pub fn against_live_daemon(socket: PathBuf) -> Self {
        Self::new(socket, Arc::new(DiscoverySocketResolver), live_daemon_client)
    }

    async fn snapshot(&self) -> Arc<dyn AircDaemonClient> {
        self.current.read().await.clone()
    }

    async fn reresolve(
        &self,
        original: &str,
    ) -> Result<Arc<dyn AircDaemonClient>, DaemonCallError> {
        let socket = self.resolver.resolve().await.map_err(|error| {
            DaemonCallError::Other(format!(
                "{original}; re-resolving the daemon socket failed: {error}"
            ))
        })?;
        let fresh = (self.factory)(socket);
        *self.current.write().await = fresh.clone();
        Ok(fresh)
    }
}

#[async_trait]
impl AircDaemonClient for ReresolvingDaemonClient {
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, DaemonCallError> {
        match self.snapshot().await.publish(request.clone()).await {
            Err(DaemonCallError::Unreachable(cause)) => {
                self.reresolve(&cause).await?.publish(request).await
            }
            outcome => outcome,
        }
    }

    async fn inbox(
        &self,
        request: InboxRequest,
    ) -> Result<InboxResponse, DaemonCallError> {
        match self.snapshot().await.inbox(request.clone()).await {
            Err(DaemonCallError::Unreachable(cause)) => {
                self.reresolve(&cause).await?.inbox(request).await
            }
            outcome => outcome,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const UNREACHABLE: &str =
        "daemon not reachable: The system cannot find the file specified. (os error 2)";
    /// The re-resolved client answers with a sentinel rather than a real
    /// `InboxResponse`: what these tests assert is WHICH client served
    /// the call, and a sentinel proves that without fabricating a wire
    /// response whose shape is airc's to define, not ours.
    const SECOND_CLIENT: &str = "reached the re-resolved client";

    struct Canned {
        unreachable: bool,
        message: &'static str,
    }

    impl Canned {
        fn error(&self) -> DaemonCallError {
            if self.unreachable {
                DaemonCallError::Unreachable(self.message.to_string())
            } else {
                DaemonCallError::Other(self.message.to_string())
            }
        }
    }

    #[async_trait]
    impl AircDaemonClient for Canned {
        async fn publish(&self, _: PublishRequest) -> Result<PublishResponse, DaemonCallError> {
            Err(self.error())
        }
        async fn inbox(&self, _: InboxRequest) -> Result<InboxResponse, DaemonCallError> {
            Err(self.error())
        }
    }

    /// Maps a resolved path to a client, so a test can say "after
    /// re-resolution the daemon is at a different path" without a
    /// stateful factory (the factory is a plain `fn`).
    fn factory(path: PathBuf) -> Arc<dyn AircDaemonClient> {
        if path.ends_with("moved.sock") {
            Arc::new(Canned {
                unreachable: false,
                message: SECOND_CLIENT,
            })
        } else if path.ends_with("rejects.sock") {
            Arc::new(Canned {
                unreachable: false,
                message: "publish rejected: not a member of that channel",
            })
        } else {
            Arc::new(Canned {
                unreachable: true,
                message: UNREACHABLE,
            })
        }
    }

    struct Resolver {
        calls: AtomicUsize,
        answer: Result<&'static str, &'static str>,
    }

    impl Resolver {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                answer: Ok("moved.sock"),
            })
        }
        fn failing() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                answer: Err("`airc ipc-endpoint` did not exit within 2s"),
            })
        }
    }

    #[async_trait]
    impl DaemonSocketResolver for Resolver {
        async fn resolve(&self) -> Result<PathBuf, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer
                .map(PathBuf::from)
                .map_err(|error| error.to_string())
        }
    }

    fn request() -> InboxRequest {
        InboxRequest {
            since: None,
            channel: None,
            limit: None,
            kinds: None,
        }
    }

    // what this catches: regression for #3849 — the boot-resolved socket
    // was kept for the process lifetime, so a daemon restart meant every
    // subsequent call failed FOREVER (1,216 consecutive capacity-publish
    // failures over 7h while the daemon was up and connectable). An
    // unreachable error must re-resolve and retry against the new path.
    #[tokio::test]
    async fn an_unreachable_daemon_re_resolves_and_retries_once() {
        let resolver = Resolver::ok();
        let client =
            ReresolvingDaemonClient::new("dead.sock".into(), resolver.clone(), factory);

        let outcome = client.inbox(request()).await;

        assert_eq!(
            outcome.unwrap_err().to_string(),
            SECOND_CLIENT,
            "retry must use the re-resolved client"
        );
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1, "exactly one re-resolve");
    }

    // what this catches: re-resolving on ANY error would mask real faults
    // (a rejected publish means the daemon IS answering). Only
    // reachability failures may re-resolve.
    #[tokio::test]
    async fn a_non_reachability_error_never_re_resolves() {
        let resolver = Resolver::ok();
        let client =
            ReresolvingDaemonClient::new("rejects.sock".into(), resolver.clone(), factory);

        let outcome = client.inbox(request()).await;

        assert!(
            outcome.unwrap_err().to_string().contains("not a member"),
            "original error surfaces"
        );
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0, "no re-resolve attempted");
    }

    // what this catches: [[no-fallbacks-ever]] — when re-resolution
    // itself fails the caller must see BOTH the original reachability
    // failure and why the re-ask failed, never a masked success or a
    // lone generic error.
    #[tokio::test]
    async fn a_failed_re_resolution_reports_both_causes() {
        let resolver = Resolver::failing();
        let client =
            ReresolvingDaemonClient::new("dead.sock".into(), resolver.clone(), factory);

        let error = client.inbox(request()).await.unwrap_err().to_string();

        assert!(error.contains("not reachable"), "original cause kept: {error}");
        assert!(error.contains("ipc-endpoint"), "re-resolution cause kept: {error}");
    }

    // what this catches: IntelMac's #3850 review — the recovery must be
    // keyed to what the REAL client produces, not to this crate's copy of
    // a message. It exercises `airc_ipc::DaemonClient` against a socket
    // that does not exist and asserts the error classifies as
    // `Unreachable`. If airc ever changes how an absent daemon surfaces
    // (a new ClientError variant, a different io error path), this goes
    // red HERE instead of silently disabling the re-resolve in production
    // and re-opening #3849 with every other test still green.
    #[tokio::test]
    async fn the_real_clients_absent_daemon_error_classifies_as_unreachable() {
        let absent = std::env::temp_dir().join("continuum-3849-no-such-airc-socket.sock");
        let real = DaemonClient::new(absent);

        let error = AircDaemonClient::inbox(&real, request())
            .await
            .expect_err("dialing an absent daemon must fail");

        assert!(
            matches!(error, DaemonCallError::Unreachable(_)),
            "airc's absent-daemon error must classify as Unreachable, got: {error}"
        );
    }

    // what this catches: the second call must use the client cached by
    // the first re-resolution rather than re-resolving every time — the
    // retry is a repair, not a per-call discovery shell-out.
    #[tokio::test]
    async fn a_repaired_client_is_reused_by_later_calls() {
        let resolver = Resolver::ok();
        let client =
            ReresolvingDaemonClient::new("dead.sock".into(), resolver.clone(), factory);

        let _ = client.inbox(request()).await;
        let second = client.inbox(request()).await;

        assert_eq!(second.unwrap_err().to_string(), SECOND_CLIENT);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1, "no second re-resolve");
    }
}
