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
    /// Bind `port` (0: any free port) and serve on it, returning the port bound. Refused while
    /// a listener still serves; one that exited is joined first, so a reinitialize after a
    /// stop or a failure binds cleanly.
    async fn listen(&self, port: u16) -> Result<u16, String> {
        let mut slot = self.listener.lock().await;
        if let Some(live) = slot.as_ref().filter(|listener| listener.is_live()) {
            return Err(format!("the gateway is already listening on port {}", live.port()));
        }
        if let Some(ended) = slot.take() {
            ended.join().await?;
        }
        let listener = Listener::bind(port).await.inspect_err(|error| {
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

    /// Stop what is still in flight (its agents are told the node is stopping), wait for the
    /// listener to return, and join it. Cancelled by the runtime's bound, the listener is
    /// dropped, which aborts its task.
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
}
