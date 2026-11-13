//! The gateway as a `ServiceModule`: one listener on a stable port that the module owns
//! through its whole lifecycle ([`crate::listener`]), and the commands that report it and
//! print what each agent sets.

use async_trait::async_trait;
use continuum_engine::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use serde_json::{json, Value};

use crate::listener::Listener;

/// The gateway's port when nothing configures one: beside the desktop's 8975.
const DEFAULT_PORT: u16 = 8976;

#[derive(Default)]
pub struct GatewayModule {
    /// The listener: `None` before `initialize` and after `shutdown`. Whether it serves is
    /// read from its task each time (`Listener::is_live`), never remembered.
    listener: tokio::sync::Mutex<Option<Listener>>,
}

impl GatewayModule {
    /// Bind `port` (0: any free port) and serve the gateway on it, returning the port bound.
    async fn listen(&self, port: u16) -> Result<u16, String> {
        self.listen_with(port, crate::server::router).await
    }

    /// Bind `port` and serve `routes` on it. Refused while a listener still serves; one that
    /// exited is joined first, so a reinitialize after a stop or a failure binds cleanly.
    async fn listen_with(&self, port: u16, routes: impl FnOnce(crate::stop::Signal) -> axum::Router) -> Result<u16, String> {
        let mut slot = self.listener.lock().await;
        if let Some(live) = slot.as_ref().filter(|listener| listener.is_live()) {
            return Err(format!("the gateway is already listening on port {}", live.port()));
        }
        if let Some(ended) = slot.take() {
            ended.join().await?;
        }
        let listener = Listener::bind(port, routes).await.inspect_err(|error| {
            continuum_base::probe!(class = "gateway.unbound", port = port as u64, error = %error,
                "the gateway could not bind its port; agents cannot reach this node until it does");
        })?;
        let bound = listener.port();
        continuum_base::probe!(class = "gateway.listening", port = bound as u64, "agents can call this node's engine");
        *slot = Some(listener);
        Ok(bound)
    }

    /// Where agents reach the gateway, while its listener serves.
    async fn url(&self) -> Option<String> {
        let slot = self.listener.lock().await;
        slot.as_ref().filter(|listener| listener.is_live()).map(|listener| format!("http://127.0.0.1:{}", listener.port()))
    }
}

/// `CONTINUUM_GATEWAY_PORT` from the environment, then config.env, then the default.
fn configured_port() -> Result<u16, String> {
    match std::env::var("CONTINUUM_GATEWAY_PORT").ok().or_else(|| continuum_base::config_env::read("CONTINUUM_GATEWAY_PORT")) {
        Some(raw) => raw.trim().parse().map_err(|_| format!("CONTINUUM_GATEWAY_PORT={raw} is not a port")),
        None => Ok(DEFAULT_PORT),
    }
}

/// What each agent sets to call the gateway at `base`.
fn env(base: &str) -> Value {
    json!({
        "claude-code": {"ANTHROPIC_BASE_URL": base, "ANTHROPIC_AUTH_TOKEN": "continuum"},
        "codex": {"OPENAI_BASE_URL": format!("{base}/v1")},
        "grok-build": {"--xai-api-base-url": format!("{base}/v1")},
        "hermes": {"OPENAI_BASE_URL": format!("{base}/v1")},
        "opencode": {"baseURL": format!("{base}/v1")},
    })
}

#[async_trait]
impl ServiceModule for GatewayModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "gateway",
            priority: ModulePriority::Normal,
            command_prefixes: &["gateway/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    /// Bind on loopback, or fail: a node whose gateway cannot bind reports it as this
    /// module's initialization failure (the runtime boots the rest and names it).
    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        self.listen(configured_port()?).await.map(|_| ())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        let base = self.url().await;
        match command {
            "gateway/status" => Ok(CommandResult::Json(json!({
                "listening": base.is_some(), "url": base, "models": crate::route::served_models().await,
            }))),
            "gateway/env" => match base {
                Some(base) => Ok(CommandResult::Json(env(&base))),
                None => Err("the gateway is not listening; see probe gateway.unbound or gateway.stopped".into()),
            },
            _ => Err(format!("unknown gateway command: {command}")),
        }
    }

    /// Close admission and let the calls in flight finish, within the runtime's bound. The
    /// listener stays owned here; `shutdown` joins it.
    async fn drain(&self) -> Result<u32, String> {
        let exited = {
            let slot = self.listener.lock().await;
            let Some(listener) = slot.as_ref() else {
                return Ok(0);
            };
            listener.close();
            listener.exited()
        };
        exited.await;
        Ok(0)
    }

    /// Stop what is still in flight and cut every connection, so even an agent that stopped
    /// reading releases its generation, admission and capture lease (`crate::sever`); wait
    /// for the listener to return, which axum does only once every connection task ended,
    /// and join it. Cancelled by the runtime's bound, the listener is dropped, which aborts
    /// its task.
    async fn shutdown(&self) -> Result<(), String> {
        let Some(listener) = self.listener.lock().await.take() else {
            return Ok(());
        };
        listener.close();
        listener.abandon();
        listener.exited().await;
        listener.join().await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a listener the module cannot stop (the detached `axum::serve` of
    // 856ee2b69), a stop that leaves status saying `listening`, and a reinitialize that
    // collides with the old socket: initialize, stop (drain + shutdown, as the runtime
    // does), then initialize again on the SAME port.
    #[tokio::test]
    async fn a_stopped_gateway_reports_it_and_rebinds_its_port() {
        let gateway = GatewayModule::default();
        let port = gateway.listen(0).await.expect("test: binds a free port");
        assert_eq!(gateway.url().await, Some(format!("http://127.0.0.1:{port}")));
        assert!(gateway.listen(0).await.is_err(), "a second listener while one serves is refused");
        assert_eq!(gateway.drain().await, Ok(0), "nothing in flight: drained");
        assert_eq!(gateway.url().await, None, "admission closed: not listening");
        assert_eq!(gateway.shutdown().await, Ok(()));
        assert_eq!(gateway.listen(port).await, Ok(port), "the same port binds again after a stop");
        assert_eq!(gateway.shutdown().await, Ok(()), "a stop without a drain abandons and joins");
        assert!(gateway.handle_command("gateway/env", Value::Null).await.is_err(), "stopped: no env to print");
    }

    // what this catches: a port that cannot be bound logged and initialization reported Ok,
    // so the node said it was ready while no agent could reach it.
    #[tokio::test]
    async fn a_port_that_cannot_be_bound_fails_initialization() {
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("test: a port to occupy");
        let port = taken.local_addr().expect("test: its address").port();
        let gateway = GatewayModule::default();
        let refused = gateway.listen(port).await.expect_err("test: the port is taken");
        assert!(refused.contains(&port.to_string()), "names the port: {refused}");
        assert_eq!(gateway.url().await, None);
    }

    // what this catches: status remembered from the bind (the `OnceLock` port) instead of
    // read from the listener, so a listener that ended still reads as listening.
    #[tokio::test]
    async fn status_reads_false_once_the_listener_has_exited() {
        let gateway = GatewayModule::default();
        gateway.listen(0).await.expect("test: binds");
        let exited = {
            let slot = gateway.listener.lock().await;
            let listener = slot.as_ref().expect("test: listening");
            listener.close(); // the serve task returns, as it would on its own failure
            listener.exited()
        };
        exited.await;
        assert_eq!(gateway.url().await, None, "an exited listener is not listening");
        assert!(gateway.listen(0).await.is_ok(), "an exited listener is joined and a new one binds");
    }

    /// Sets its flag when dropped: stands for the capture lease, which rides in the same
    /// generation future and records the call when that future is dropped.
    struct Settles(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl Drop for Settles {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    // what this catches: an agent that stays CONNECTED but stops reading. Its socket fills,
    // hyper stops polling the body, and a stop the body would notice is never noticed, so the
    // generation it owns (its slot's admission, its capture lease) and the connection's task
    // outlive the stop (Codex on f2a3600fe). Over real HTTP: a long streamed answer through
    // the real `Answer` and SSE response, a client that never reads, then the module's drain
    // and shutdown each under the runtime's own per-phase bound, as `Runtime::shutdown` runs
    // them.
    #[tokio::test]
    async fn an_agent_that_stops_reading_is_cut_and_everything_it_held_settles_within_the_stop() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::io::AsyncWriteExt;

        use continuum_engine::ai::adapter::GenerationChunk;
        use continuum_engine::ai::stream_sinks::deliver;

        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let settled = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicUsize::new(0));
        let (held, recorded, counted) = (slots.clone(), settled.clone(), sent.clone());
        let routes = move |abandoned: crate::stop::Signal| {
            axum::Router::new().route(
                "/v1/long",
                axum::routing::get(move || {
                    let (held, recorded, counted, abandoned) = (held.clone(), recorded.clone(), counted.clone(), abandoned.clone());
                    async move {
                        let admission = held.try_acquire_owned().expect("test: one free slot");
                        let (sink, received) = tokio::sync::mpsc::unbounded_channel();
                        let generation = async move {
                            let _admission = admission;
                            let _lease = Settles(recorded);
                            let token = "x".repeat(64 * 1024);
                            for _ in 0..usize::MAX {
                                counted.fetch_add(1, Ordering::SeqCst);
                                deliver(&sink, GenerationChunk::Token(token.clone())).await;
                            }
                            Err(crate::route::RouteError::Engine("test: a decode longer than the test".into()))
                        };
                        let encoder = <crate::wire::chat::Chat as crate::wire::Wire>::streamer("qwen", "chatcmpl_1");
                        crate::server::streamed(crate::answer::Answer::new(encoder, generation, received, abandoned.raised()))
                    }
                }),
            )
        };
        let gateway = GatewayModule::default();
        let port = gateway.listen_with(0, routes).await.expect("test: binds");
        let mut agent = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.expect("test: connects");
        agent
            .write_all(b"GET /v1/long HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n")
            .await
            .expect("test: sends its request");
        // The agent never reads. Wait for the generation to stop advancing: both sockets full.
        let mut last = 0;
        let mut still = 0;
        for _ in 0..400 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            let now = sent.load(Ordering::SeqCst);
            still = if now == last && now > 1 { still + 1 } else { 0 };
            last = now;
            if still == 4 {
                break;
            }
        }
        assert_eq!(still, 4, "test: the stream must stall against a client that does not read");
        assert_eq!(slots.available_permits(), 0, "the stalled generation holds its slot");
        assert!(!settled.load(Ordering::SeqCst));

        let bound = continuum_engine::runtime::runtime::SHUTDOWN_PHASE;
        let drained = tokio::time::timeout(bound, gateway.drain()).await;
        assert!(drained.is_err(), "the agent is still connected: the drain runs to its bound");
        let stopped = tokio::time::timeout(bound, gateway.shutdown()).await;
        assert!(matches!(stopped, Ok(Ok(()))), "the stop completes within its bound: {stopped:?}");
        assert_eq!(slots.available_permits(), 1, "the generation's admission was released before the stop returned");
        assert!(settled.load(Ordering::SeqCst), "its capture lease settled before the stop returned");
        assert_eq!(
            tokio::runtime::Handle::current().metrics().num_alive_tasks(),
            0,
            "no connection task survives the stop"
        );
        drop(agent);
    }
}
