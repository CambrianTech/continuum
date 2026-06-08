//! ExternalWebviewAuthModule — OAuth 2.0 + PKCE via external browser
//!
//! Flow:
//!   1. `auth/oauth/start`    → generate PKCE verifier/challenge, record pending state,
//!                              spin up a temporary axum redirect-catcher on `redirect_port`,
//!                              open the system browser at the authorization URL.
//!   2. Browser redirects     → local catcher validates state, exchanges code for tokens
//!                              via the token endpoint, persists tokens to config.env.
//!   3. `auth/oauth/refresh`  → re-use stored refresh_token to obtain a fresh access_token.
//!   4. `auth/oauth/revoke`   → optional server-side revocation + delete from config.env.
//!
//! Tokens are persisted as `{PROVIDER_ID_UPPER}_ACCESS_TOKEN` etc. in
//! `~/.continuum/config.env`, following the existing `secrets.rs` convention.
//!
//! Commands:
//! - auth/oauth/start     — Begin OAuth flow (opens browser)
//! - auth/oauth/status    — Check token validity for a provider
//! - auth/oauth/refresh   — Force-refresh via refresh_token
//! - auth/oauth/revoke    — Revoke + delete tokens
//! - auth/oauth/providers — List registered providers
//! - auth/oauth/register  — Register a new provider at runtime

use crate::runtime::{
    CommandResult, CommandSchema, ModuleConfig, ModuleContext, ModulePriority, ParamSchema,
    ServiceModule,
};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use dashmap::DashMap;
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, RwLock};

// ============================================================================
// Public types
// ============================================================================

/// OAuth 2.0 provider configuration.
///
/// Each provider needs at minimum: `client_id`, `auth_url`, `token_url`, `scopes`,
/// and a `redirect_port` for the temporary localhost callback server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClientConfig {
    /// Unique provider identifier, e.g. `"github"`, `"google"`, `"huggingface"`.
    pub provider_id: String,
    /// OAuth application client ID.
    pub client_id: String,
    /// OAuth application client secret. `None` for public (PKCE-only) clients.
    pub client_secret: Option<String>,
    /// Authorization endpoint URL.
    pub auth_url: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// Space-separated OAuth scopes.
    pub scopes: String,
    /// localhost port for the temporary redirect-URI catcher (e.g. `47200`).
    pub redirect_port: u16,
    /// Optional token revocation endpoint URL.
    pub revoke_url: Option<String>,
}

impl OAuthClientConfig {
    fn redirect_uri(&self) -> String {
        format!("http://localhost:{}/oauth/callback", self.redirect_port)
    }
}

// ============================================================================
// Internal types
// ============================================================================

/// PKCE state kept alive while the user completes the browser auth flow.
struct PkceState {
    code_verifier: String,
    provider_id: String,
}

/// Access + refresh token bundle stored per provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenSet {
    access_token: String,
    refresh_token: Option<String>,
    /// Unix timestamp (seconds) at which `access_token` expires. `0` = unknown.
    expires_at: u64,
    token_type: String,
    scope: Option<String>,
}

impl TokenSet {
    /// Returns `true` if the access token is within 60 seconds of expiry.
    fn is_expired(&self) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at.saturating_sub(60)
    }
}

// ============================================================================
// ExternalWebviewAuthService
// ============================================================================

/// Core service — manages provider configs, PKCE sessions, and token cache.
///
/// Separated from the ServiceModule glue layer for independent testability.
pub struct ExternalWebviewAuthService {
    /// Registered providers keyed by `provider_id`.
    providers: Arc<RwLock<HashMap<String, OAuthClientConfig>>>,
    /// In-flight PKCE sessions keyed by the opaque `state` parameter.
    pending: Arc<DashMap<String, PkceState>>,
    /// Cached token sets keyed by `provider_id`.
    tokens: Arc<DashMap<String, TokenSet>>,
    http: Client,
}

impl ExternalWebviewAuthService {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(DashMap::new()),
            tokens: Arc::new(DashMap::new()),
            http: Client::new(),
        }
    }

    /// Register an OAuth provider configuration.
    pub async fn register_provider(&self, config: OAuthClientConfig) {
        self.providers
            .write()
            .await
            .insert(config.provider_id.clone(), config);
    }

    /// Register built-in providers whose `CLIENT_ID` secrets are present in config.env.
    pub async fn load_defaults(&self) {
        use crate::secrets::get_secret;

        if let Some(id) = get_secret("GITHUB_CLIENT_ID") {
            self.register_provider(OAuthClientConfig {
                provider_id: "github".into(),
                client_id: id.to_string(),
                client_secret: get_secret("GITHUB_CLIENT_SECRET").map(str::to_string),
                auth_url: "https://github.com/login/oauth/authorize".into(),
                token_url: "https://github.com/login/oauth/access_token".into(),
                scopes: "read:user repo".into(),
                redirect_port: 47200,
                revoke_url: None,
            })
            .await;
        }

        if let Some(id) = get_secret("HUGGINGFACE_CLIENT_ID") {
            self.register_provider(OAuthClientConfig {
                provider_id: "huggingface".into(),
                client_id: id.to_string(),
                client_secret: get_secret("HUGGINGFACE_CLIENT_SECRET").map(str::to_string),
                auth_url: "https://huggingface.co/oauth/authorize".into(),
                token_url: "https://huggingface.co/oauth/token".into(),
                scopes: "openid profile read-repos manage-repos".into(),
                redirect_port: 47201,
                revoke_url: None,
            })
            .await;
        }

        if let Some(id) = get_secret("GOOGLE_CLIENT_ID") {
            self.register_provider(OAuthClientConfig {
                provider_id: "google".into(),
                client_id: id.to_string(),
                client_secret: get_secret("GOOGLE_CLIENT_SECRET").map(str::to_string),
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                token_url: "https://oauth2.googleapis.com/token".into(),
                scopes: "openid email profile".into(),
                redirect_port: 47202,
                revoke_url: Some("https://oauth2.googleapis.com/revoke".into()),
            })
            .await;
        }
    }

    // ─── PKCE ──────────────────────────────────────────────────────────────

    fn generate_code_verifier() -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    fn code_challenge(verifier: &str) -> String {
        let hash = Sha256::digest(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    }

    fn generate_state() -> String {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    // ─── OAuth flow ────────────────────────────────────────────────────────

    /// Start an OAuth 2.0 + PKCE authorization flow.
    ///
    /// Opens the system browser and spins up a temporary redirect-catcher.
    /// Returns the authorization URL and metadata; the token exchange happens
    /// asynchronously when the browser redirects back.
    pub async fn start_flow(&self, provider_id: &str) -> Result<Value, String> {
        let config = {
            let providers = self.providers.read().await;
            providers
                .get(provider_id)
                .ok_or_else(|| {
                    format!(
                        "Unknown OAuth provider: '{provider_id}'. \
                         Register it with auth/oauth/register or add {}_CLIENT_ID to config.env.",
                        provider_id.to_uppercase()
                    )
                })?
                .clone()
        };

        let code_verifier = Self::generate_code_verifier();
        let code_challenge = Self::code_challenge(&code_verifier);
        let state = Self::generate_state();

        let auth_url = build_auth_url(&config, &code_challenge, &state);

        self.pending.insert(
            state.clone(),
            PkceState {
                code_verifier,
                provider_id: provider_id.to_string(),
            },
        );

        // Spin up the redirect-catcher, wait for it to bind.
        let (ready_tx, ready_rx) = oneshot::channel::<Result<u16, String>>();
        let pending_arc = self.pending.clone();
        let tokens_arc = self.tokens.clone();
        let http_arc = self.http.clone();
        let config_arc = config.clone();

        tokio::spawn(async move {
            if let Err(e) =
                run_redirect_catcher(config_arc, pending_arc, tokens_arc, http_arc, ready_tx).await
            {
                eprintln!("[auth] Redirect catcher error: {e}");
            }
        });

        let actual_port = tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx)
            .await
            .map_err(|_| "Redirect server did not start within 5 seconds".to_string())?
            .map_err(|_| "Redirect server channel dropped".to_string())??;

        open_browser(&auth_url);

        Ok(json!({
            "provider_id": provider_id,
            "auth_url": auth_url,
            "redirect_uri": config.redirect_uri(),
            "redirect_port": actual_port,
            "state": state,
            "message": format!(
                "Browser opened for {} authentication. Listening on localhost:{} for callback.",
                provider_id, actual_port
            ),
        }))
    }

    /// Return the current token status for `provider_id`.
    pub fn token_status(&self, provider_id: &str) -> Value {
        match self.tokens.get(provider_id) {
            None => json!({
                "provider_id": provider_id,
                "authenticated": false,
            }),
            Some(ts) => json!({
                "provider_id": provider_id,
                "authenticated": true,
                "expired": ts.is_expired(),
                "has_refresh_token": ts.refresh_token.is_some(),
                "expires_at": ts.expires_at,
                "scope": ts.scope,
            }),
        }
    }

    /// Refresh the access token for `provider_id` using its stored refresh_token.
    pub async fn refresh_token(&self, provider_id: &str) -> Result<Value, String> {
        let config = {
            let providers = self.providers.read().await;
            providers
                .get(provider_id)
                .ok_or_else(|| format!("Unknown OAuth provider: '{provider_id}'"))?
                .clone()
        };

        let refresh_token = self
            .tokens
            .get(provider_id)
            .and_then(|ts| ts.refresh_token.clone())
            .ok_or_else(|| {
                format!(
                    "No refresh_token stored for '{provider_id}'. \
                     Run auth/oauth/start to authenticate first."
                )
            })?;

        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token),
            ("client_id", config.client_id.clone()),
        ];
        if let Some(secret) = &config.client_secret {
            form.push(("client_secret", secret.clone()));
        }

        let response = self
            .http
            .post(&config.token_url)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Token refresh request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Token refresh failed (HTTP {status}): {body}"));
        }

        let token_resp: Value = response
            .json()
            .await
            .map_err(|e| format!("Token refresh response parse error: {e}"))?;

        let token_set = parse_token_response(&token_resp);
        persist_tokens(provider_id, &token_set)?;
        self.tokens.insert(provider_id.to_string(), token_set);

        Ok(json!({ "provider_id": provider_id, "refreshed": true }))
    }

    /// Revoke tokens server-side (if a revocation endpoint is configured) and
    /// delete them from config.env and the in-memory cache.
    pub async fn revoke_tokens(&self, provider_id: &str) -> Result<Value, String> {
        let config = {
            let providers = self.providers.read().await;
            providers
                .get(provider_id)
                .ok_or_else(|| format!("Unknown OAuth provider: '{provider_id}'"))?
                .clone()
        };

        if let (Some(revoke_url), Some(ts)) = (
            &config.revoke_url,
            self.tokens.get(provider_id).map(|r| r.clone()),
        ) {
            let _ = self
                .http
                .post(revoke_url)
                .form(&[("token", &ts.access_token)])
                .send()
                .await;
        }

        self.tokens.remove(provider_id);
        delete_tokens_from_config(provider_id)?;

        Ok(json!({ "provider_id": provider_id, "revoked": true }))
    }

    /// Load any tokens previously persisted to config.env into the in-memory cache.
    ///
    /// Called during `initialize()`. The `secrets` module uses `OnceLock` so it reads
    /// the file once at startup — tokens written in a previous session are available here.
    pub fn load_persisted_tokens(&self) {
        use crate::secrets::get_secret;

        for provider_id in ["github", "huggingface", "google"] {
            let prefix = provider_id.to_uppercase();
            if let Some(access_token) = get_secret(&format!("{prefix}_ACCESS_TOKEN")) {
                let token_set = TokenSet {
                    access_token: access_token.to_string(),
                    refresh_token: get_secret(&format!("{prefix}_REFRESH_TOKEN"))
                        .map(str::to_string),
                    expires_at: get_secret(&format!("{prefix}_TOKEN_EXPIRES_AT"))
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    token_type: "bearer".to_string(),
                    scope: get_secret(&format!("{prefix}_TOKEN_SCOPE")).map(str::to_string),
                };
                self.tokens.insert(provider_id.to_string(), token_set);
            }
        }
    }
}

// ============================================================================
// Redirect-catcher (temporary axum server)
// ============================================================================

/// Bind a temporary axum server on `config.redirect_port`, signal `ready_tx` when bound,
/// wait for exactly one OAuth callback, then shut down.
async fn run_redirect_catcher(
    config: OAuthClientConfig,
    pending: Arc<DashMap<String, PkceState>>,
    tokens: Arc<DashMap<String, TokenSet>>,
    http: Client,
    ready_tx: oneshot::Sender<Result<u16, String>>,
) -> Result<(), String> {
    use axum::{extract::Query, response::Html, routing::get, Router};
    use std::collections::HashMap as QMap;

    let listener =
        match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", config.redirect_port)).await {
            Ok(l) => l,
            Err(e) => {
                let _ = ready_tx.send(Err(format!(
                    "Cannot bind redirect server on port {}: {e}",
                    config.redirect_port
                )));
                return Err(e.to_string());
            }
        };

    let actual_port = listener
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(config.redirect_port);

    // Signal readiness before starting to serve.
    let _ = ready_tx.send(Ok(actual_port));

    // Shared shutdown signal — fired by the callback handler.
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let done_tx = Arc::new(Mutex::new(Some(done_tx)));

    let config_arc = Arc::new(config);
    let pending_arc = pending;
    let tokens_arc = tokens;
    let http_arc = http;

    let app = Router::new().route(
        "/oauth/callback",
        get({
            let config = config_arc.clone();
            let pending = pending_arc.clone();
            let tokens = tokens_arc.clone();
            let http = http_arc.clone();
            let done = done_tx.clone();
            move |Query(params): Query<QMap<String, String>>| {
                let config = config.clone();
                let pending = pending.clone();
                let tokens = tokens.clone();
                let http = http.clone();
                let done = done.clone();
                async move {
                    let result = handle_callback(params, config, pending, tokens, http).await;
                    // Trigger graceful shutdown after the first callback.
                    if let Some(tx) = done.lock().await.take() {
                        let _ = tx.send(());
                    }
                    match result {
                        Ok(msg) => Html(format!(
                            "<!DOCTYPE html><html><body>\
                             <h2>Authentication successful</h2><p>{msg}</p>\
                             <script>setTimeout(()=>window.close(),2000)</script>\
                             </body></html>"
                        )),
                        Err(e) => Html(format!(
                            "<!DOCTYPE html><html><body>\
                             <h2>Authentication failed</h2><p>{e}</p>\
                             </body></html>"
                        )),
                    }
                }
            }
        }),
    );

    // Serve until callback received or 5-minute hard timeout.
    tokio::select! {
        result = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = done_rx.await;
        }) => {
            if let Err(e) = result {
                eprintln!("[auth] Redirect server error: {e}");
            }
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {
            eprintln!("[auth] OAuth callback timed out after 5 minutes for provider");
        }
    }

    Ok(())
}

/// Validate the OAuth callback parameters, exchange code for tokens, and persist.
async fn handle_callback(
    params: HashMap<String, String>,
    config: Arc<OAuthClientConfig>,
    pending: Arc<DashMap<String, PkceState>>,
    tokens: Arc<DashMap<String, TokenSet>>,
    http: Client,
) -> Result<String, String> {
    // Surface provider errors before anything else.
    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|s| s.as_str())
            .unwrap_or("(no description)");
        return Err(format!("Provider error: {error} — {desc}"));
    }

    let code = params
        .get("code")
        .ok_or("OAuth callback missing 'code' parameter")?
        .clone();

    let state = params
        .get("state")
        .ok_or("OAuth callback missing 'state' parameter")?
        .clone();

    // Retrieve and remove the pending PKCE record — validates state to prevent CSRF.
    let (_, pkce) = pending
        .remove(&state)
        .ok_or("Unknown or expired OAuth state — possible CSRF or replay")?;

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("redirect_uri", config.redirect_uri()),
        ("client_id", config.client_id.clone()),
        ("code_verifier", pkce.code_verifier),
    ];
    if let Some(secret) = &config.client_secret {
        form.push(("client_secret", secret.clone()));
    }

    let response = http
        .post(&config.token_url)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed (HTTP {status}): {body}"));
    }

    let token_resp: Value = response
        .json()
        .await
        .map_err(|e| format!("Token exchange response parse error: {e}"))?;

    let token_set = parse_token_response(&token_resp);
    persist_tokens(&pkce.provider_id, &token_set)?;
    tokens.insert(pkce.provider_id.clone(), token_set);

    Ok(format!(
        "Authenticated with {}. You can close this window.",
        pkce.provider_id
    ))
}

// ============================================================================
// Auth URL builder
// ============================================================================

fn build_auth_url(config: &OAuthClientConfig, code_challenge: &str, state: &str) -> String {
    // Manual percent-encoding for the query string values that contain special chars.
    let encode = |s: &str| {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };

    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        config.auth_url,
        encode(&config.client_id),
        encode(&config.redirect_uri()),
        encode(&config.scopes),
        encode(state),
        encode(code_challenge),
    )
}

// ============================================================================
// Token persistence helpers
// ============================================================================

fn parse_token_response(resp: &Value) -> TokenSet {
    let expires_at = resp["expires_in"].as_u64().map(|secs| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now + secs
    });

    TokenSet {
        access_token: resp["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: resp["refresh_token"].as_str().map(str::to_string),
        expires_at: expires_at.unwrap_or(0),
        token_type: resp["token_type"].as_str().unwrap_or("bearer").to_string(),
        scope: resp["scope"].as_str().map(str::to_string),
    }
}

/// Write/overwrite token keys in `~/.continuum/config.env`.
/// Preserves all other content; only the token keys are updated or appended.
fn persist_tokens(provider_id: &str, token_set: &TokenSet) -> Result<(), String> {
    let prefix = provider_id.to_uppercase().replace('-', "_");
    let mut updates = vec![
        (
            format!("{prefix}_ACCESS_TOKEN"),
            token_set.access_token.clone(),
        ),
        (
            format!("{prefix}_TOKEN_EXPIRES_AT"),
            token_set.expires_at.to_string(),
        ),
    ];
    if let Some(rt) = &token_set.refresh_token {
        updates.push((format!("{prefix}_REFRESH_TOKEN"), rt.clone()));
    }
    if let Some(scope) = &token_set.scope {
        updates.push((format!("{prefix}_TOKEN_SCOPE"), scope.clone()));
    }
    write_config_env_keys(updates)
}

/// Delete token keys for `provider_id` from `~/.continuum/config.env`.
fn delete_tokens_from_config(provider_id: &str) -> Result<(), String> {
    let prefix = provider_id.to_uppercase().replace('-', "_");
    let to_delete: HashSet<String> = [
        format!("{prefix}_ACCESS_TOKEN"),
        format!("{prefix}_REFRESH_TOKEN"),
        format!("{prefix}_TOKEN_EXPIRES_AT"),
        format!("{prefix}_TOKEN_SCOPE"),
    ]
    .into_iter()
    .collect();

    let config_path = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".continuum")
        .join("config.env");

    if !config_path.exists() {
        return Ok(());
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Cannot read config.env: {e}"))?;

    let filtered: Vec<&str> = content
        .lines()
        .filter(|line| {
            if let Some((key, _)) = line.trim().split_once('=') {
                !to_delete.contains(key.trim())
            } else {
                true
            }
        })
        .collect();

    fs::write(&config_path, filtered.join("\n") + "\n")
        .map_err(|e| format!("Cannot write config.env: {e}"))?;

    Ok(())
}

/// Update or append `key=value` pairs in `~/.continuum/config.env`.
/// Existing lines with matching keys are replaced in-place; new keys are appended.
fn write_config_env_keys(updates: Vec<(String, String)>) -> Result<(), String> {
    let config_path = dirs::home_dir()
        .ok_or("Cannot determine home directory")?
        .join(".continuum")
        .join("config.env");

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create .continuum directory: {e}"))?;
    }

    let existing = if config_path.exists() {
        fs::read_to_string(&config_path).map_err(|e| format!("Cannot read config.env: {e}"))?
    } else {
        String::new()
    };

    let update_map: HashMap<String, String> = updates.into_iter().collect();
    let mut written: HashSet<String> = HashSet::new();

    let mut new_lines: Vec<String> = existing
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return line.to_string();
            }
            if let Some((key, _)) = trimmed.split_once('=') {
                let key = key.trim();
                if let Some(val) = update_map.get(key) {
                    written.insert(key.to_string());
                    return format!("{key}={val}");
                }
            }
            line.to_string()
        })
        .collect();

    for (key, val) in &update_map {
        if !written.contains(key) {
            new_lines.push(format!("{key}={val}"));
        }
    }

    fs::write(&config_path, new_lines.join("\n") + "\n")
        .map_err(|e| format!("Cannot write config.env: {e}"))?;

    Ok(())
}

// ============================================================================
// Browser launcher
// ============================================================================

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();

    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .spawn();
}

// ============================================================================
// ExternalWebviewAuthModule — ServiceModule glue
// ============================================================================

pub struct ExternalWebviewAuthModule {
    service: Arc<ExternalWebviewAuthService>,
}

impl ExternalWebviewAuthModule {
    pub fn new() -> Self {
        Self {
            service: Arc::new(ExternalWebviewAuthService::new()),
        }
    }
}

impl Default for ExternalWebviewAuthModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for ExternalWebviewAuthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "auth",
            priority: ModulePriority::Normal,
            command_prefixes: &["auth/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        self.service.load_persisted_tokens();
        self.service.load_defaults().await;
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "auth/oauth/start" => {
                let provider_id = params["provider_id"]
                    .as_str()
                    .ok_or("Missing required parameter: provider_id")?;
                let result = self.service.start_flow(provider_id).await?;
                Ok(CommandResult::Json(result))
            }

            "auth/oauth/status" => {
                let provider_id = params["provider_id"]
                    .as_str()
                    .ok_or("Missing required parameter: provider_id")?;
                Ok(CommandResult::Json(self.service.token_status(provider_id)))
            }

            "auth/oauth/refresh" => {
                let provider_id = params["provider_id"]
                    .as_str()
                    .ok_or("Missing required parameter: provider_id")?;
                let result = self.service.refresh_token(provider_id).await?;
                Ok(CommandResult::Json(result))
            }

            "auth/oauth/revoke" => {
                let provider_id = params["provider_id"]
                    .as_str()
                    .ok_or("Missing required parameter: provider_id")?;
                let result = self.service.revoke_tokens(provider_id).await?;
                Ok(CommandResult::Json(result))
            }

            "auth/oauth/providers" => {
                let providers = self.service.providers.read().await;
                let list: Vec<Value> = providers
                    .values()
                    .map(|c| {
                        json!({
                            "provider_id": c.provider_id,
                            "auth_url": c.auth_url,
                            "scopes": c.scopes,
                            "redirect_port": c.redirect_port,
                            "has_revoke_url": c.revoke_url.is_some(),
                        })
                    })
                    .collect();
                Ok(CommandResult::Json(json!({ "providers": list })))
            }

            "auth/oauth/register" => {
                let config: OAuthClientConfig = serde_json::from_value(params)
                    .map_err(|e| format!("Invalid OAuth provider config: {e}"))?;
                self.service.register_provider(config).await;
                Ok(CommandResult::Json(json!({ "registered": true })))
            }

            _ => Err(format!("Unknown auth command: {command}")),
        }
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![
            CommandSchema {
                name: "auth/oauth/start",
                description: "Begin OAuth 2.0 + PKCE flow. Opens system browser at the \
                              authorization URL and spins up a temporary localhost redirect-catcher.",
                params: vec![ParamSchema {
                    name: "provider_id",
                    param_type: "string",
                    required: true,
                    description: "Provider to authenticate with: 'github', 'huggingface', \
                                  'google', or a custom registered provider.",
                }],
            },
            CommandSchema {
                name: "auth/oauth/status",
                description: "Check whether tokens exist for a provider and if they are expired.",
                params: vec![ParamSchema {
                    name: "provider_id",
                    param_type: "string",
                    required: true,
                    description: "Provider identifier to check.",
                }],
            },
            CommandSchema {
                name: "auth/oauth/refresh",
                description: "Refresh the access token using the stored refresh_token.",
                params: vec![ParamSchema {
                    name: "provider_id",
                    param_type: "string",
                    required: true,
                    description: "Provider identifier whose token to refresh.",
                }],
            },
            CommandSchema {
                name: "auth/oauth/revoke",
                description: "Revoke tokens server-side (if a revocation endpoint is configured) \
                              and delete them from config.env.",
                params: vec![ParamSchema {
                    name: "provider_id",
                    param_type: "string",
                    required: true,
                    description: "Provider identifier whose tokens to revoke.",
                }],
            },
            CommandSchema {
                name: "auth/oauth/providers",
                description: "List all registered OAuth providers.",
                params: vec![],
            },
            CommandSchema {
                name: "auth/oauth/register",
                description: "Register a new OAuth provider configuration at runtime.",
                params: vec![
                    ParamSchema {
                        name: "provider_id",
                        param_type: "string",
                        required: true,
                        description: "Unique provider identifier.",
                    },
                    ParamSchema {
                        name: "client_id",
                        param_type: "string",
                        required: true,
                        description: "OAuth application client ID.",
                    },
                    ParamSchema {
                        name: "client_secret",
                        param_type: "string",
                        required: false,
                        description: "OAuth application client secret (omit for public/PKCE-only clients).",
                    },
                    ParamSchema {
                        name: "auth_url",
                        param_type: "string",
                        required: true,
                        description: "Authorization endpoint URL.",
                    },
                    ParamSchema {
                        name: "token_url",
                        param_type: "string",
                        required: true,
                        description: "Token endpoint URL.",
                    },
                    ParamSchema {
                        name: "scopes",
                        param_type: "string",
                        required: true,
                        description: "Space-separated OAuth scopes.",
                    },
                    ParamSchema {
                        name: "redirect_port",
                        param_type: "number",
                        required: true,
                        description: "localhost port for the OAuth redirect URI (e.g. 47200).",
                    },
                    ParamSchema {
                        name: "revoke_url",
                        param_type: "string",
                        required: false,
                        description: "Optional token revocation endpoint URL.",
                    },
                ],
            },
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_challenge_known_vector() {
        // RFC 7636 §Appendix B — verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        // → challenge "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = ExternalWebviewAuthService::code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_code_verifier_length() {
        let v = ExternalWebviewAuthService::generate_code_verifier();
        // base64url of 32 bytes = 43 chars (URL_SAFE_NO_PAD)
        assert_eq!(v.len(), 43);
    }

    #[test]
    fn test_token_expiry() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let expired = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: now - 120, // 2 minutes ago
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(expired.is_expired());

        let valid = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: now + 3600, // 1 hour from now
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(!valid.is_expired());

        let unknown_expiry = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: 0,
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(!unknown_expiry.is_expired());
    }

    #[tokio::test]
    async fn test_register_and_list_providers() {
        let svc = ExternalWebviewAuthService::new();
        svc.register_provider(OAuthClientConfig {
            provider_id: "test_provider".into(),
            client_id: "test_client_id".into(),
            client_secret: None,
            auth_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            scopes: "read write".into(),
            redirect_port: 49999,
            revoke_url: None,
        })
        .await;

        let providers = svc.providers.read().await;
        assert!(providers.contains_key("test_provider"));
        assert_eq!(providers["test_provider"].client_id, "test_client_id");
    }

    #[tokio::test]
    async fn test_status_unauthenticated() {
        let svc = ExternalWebviewAuthService::new();
        let status = svc.token_status("github");
        assert_eq!(status["authenticated"], false);
    }

    #[test]
    fn test_build_auth_url_contains_pkce_params() {
        let config = OAuthClientConfig {
            provider_id: "test".into(),
            client_id: "my_client".into(),
            client_secret: None,
            auth_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            scopes: "read".into(),
            redirect_port: 47200,
            revoke_url: None,
        };
        let url = build_auth_url(&config, "test_challenge", "test_state");
        assert!(url.contains("code_challenge=test_challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=test_state"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=my_client"));
    }
}
