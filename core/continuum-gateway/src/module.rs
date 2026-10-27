//! The gateway as a `ServiceModule`: one listener on a stable port, and the commands that
//! report it and print what each agent sets.

use async_trait::async_trait;
use continuum_engine::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use serde_json::{json, Value};

/// The gateway's port when nothing configures one: beside the desktop's 8975.
const DEFAULT_PORT: u16 = 8976;

pub struct GatewayModule {
    port: std::sync::OnceLock<u16>,
}

impl Default for GatewayModule {
    fn default() -> Self {
        Self { port: std::sync::OnceLock::new() }
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

    /// Bind on loopback; a node that cannot bind says why and serves its residents anyway.
    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        let port = configured_port()?;
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                continuum_base::probe!(class = "gateway.unbound", port = port as u64, error = %error,
                    "the gateway could not bind its port; agents cannot reach this node until it does");
                return Ok(());
            }
        };
        let _ = self.port.set(port);
        continuum_base::probe!(class = "gateway.listening", port = port as u64, "agents can call this node's engine");
        tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, crate::server::router()).await {
                continuum_base::probe!(class = "gateway.stopped", error = %error, "the gateway's listener ended");
            }
        });
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        let base = self.port.get().map(|port| format!("http://127.0.0.1:{port}"));
        match command {
            "gateway/status" => Ok(CommandResult::Json(json!({
                "listening": base.is_some(), "url": base, "models": crate::route::served_models().await,
            }))),
            "gateway/env" => match base {
                Some(base) => Ok(CommandResult::Json(env(&base))),
                None => Err("the gateway is not listening; see probe gateway.unbound".into()),
            },
            _ => Err(format!("unknown gateway command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
