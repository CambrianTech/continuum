//! llama.cpp Backend — wraps llama-server HTTP API for fast inference.
//!
//! Uses llama.cpp's OpenAI-compatible `/v1/completions` endpoint.
//! Bypasses candle's tensor pipeline entirely — text in, text out.
//! Achieves 30-63 tok/s (vs candle's 4 tok/s) on Apple Silicon.
//!
//! llama-server handles: Metal/CUDA offload, quantized matmul, KV cache,
//! sampling, LoRA loading. We just send HTTP requests.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::runtime;

/// Configuration for the llama.cpp server.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: String,
    /// Host to bind llama-server to
    pub host: String,
    /// Port for llama-server
    pub port: u16,
    /// Number of GPU layers to offload (-1 = all)
    pub n_gpu_layers: i32,
    /// Context length
    pub context_length: usize,
    /// Optional LoRA adapter path
    pub lora_path: Option<String>,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            host: "127.0.0.1".into(),
            port: 8012,
            n_gpu_layers: -1,
            context_length: 32768,
            lora_path: None,
        }
    }
}

/// llama.cpp inference backend.
///
/// Manages a llama-server child process and communicates via HTTP.
/// Implements text generation without touching candle's tensor pipeline.
pub struct LlamaCppBackend {
    config: LlamaCppConfig,
    server_process: Mutex<Option<Child>>,
    base_url: String,
    model_id: String,
}

impl LlamaCppBackend {
    /// Connect to an already-running llama-server (no process management).
    pub fn from_running(config: LlamaCppConfig) -> Self {
        let base_url = format!("http://{}:{}", config.host, config.port);
        let model_id = std::path::Path::new(&config.model_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());
        Self {
            config,
            server_process: Mutex::new(None),
            base_url,
            model_id,
        }
    }

    /// Create a new backend and start llama-server.
    pub fn new(config: LlamaCppConfig) -> Result<Self, String> {
        let base_url = format!("http://{}:{}", config.host, config.port);
        let model_id = std::path::Path::new(&config.model_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let mut backend = Self {
            config,
            server_process: Mutex::new(None),
            base_url,
            model_id,
        };

        backend.start_server()?;
        backend.wait_for_ready()?;

        Ok(backend)
    }

    /// Start the llama-server process.
    fn start_server(&mut self) -> Result<(), String> {
        let log = runtime::logger("llama.cpp");

        // Find llama-server binary
        let server_bin = which_llama_server()
            .ok_or("llama-server not found. Install: brew install llama.cpp")?;

        let mut cmd = Command::new(&server_bin);
        cmd.arg("-m").arg(&self.config.model_path)
            .arg("--host").arg(&self.config.host)
            .arg("--port").arg(self.config.port.to_string())
            .arg("-ngl").arg(self.config.n_gpu_layers.to_string())
            .arg("-c").arg(self.config.context_length.to_string())
            .arg("--log-disable")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        if let Some(ref lora) = self.config.lora_path {
            cmd.arg("--lora").arg(lora);
        }

        log.info(&format!("Starting llama-server: {} on port {}",
            self.config.model_path, self.config.port));

        let child = cmd.spawn()
            .map_err(|e| format!("Failed to start llama-server: {e}"))?;

        *self.server_process.lock().unwrap() = Some(child);
        Ok(())
    }

    /// Wait for llama-server to be ready (health endpoint).
    fn wait_for_ready(&self) -> Result<(), String> {
        let log = runtime::logger("llama.cpp");
        let start = Instant::now();
        let timeout = Duration::from_secs(60);
        let health_url = format!("{}/health", self.base_url);

        loop {
            if start.elapsed() > timeout {
                return Err("llama-server failed to start within 60s".into());
            }

            match ureq_get(&health_url) {
                Ok(status) if status == 200 => {
                    log.info(&format!("llama-server ready in {:.1}s",
                        start.elapsed().as_secs_f64()));
                    return Ok(());
                }
                _ => std::thread::sleep(Duration::from_millis(500)),
            }
        }
    }

    /// Generate text using the completions API.
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
        stop: &[&str],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llama.cpp");
        let start = Instant::now();

        let stop_json: Vec<String> = stop.iter().map(|s| format!("\"{}\"", s)).collect();
        let body = format!(
            r#"{{"prompt":"{}","n_predict":{},"temperature":{},"stop":[{}],"stream":false}}"#,
            prompt.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"),
            max_tokens,
            temperature,
            stop_json.join(",")
        );

        let url = format!("{}/completion", self.base_url);
        let response = ureq_post(&url, &body)
            .map_err(|e| format!("llama-server request failed: {e}"))?;

        // Parse response
        let content: String = extract_json_field(&response, "content")
            .unwrap_or_default();
        let tokens_predicted: usize = extract_json_field_usize(&response, "tokens_predicted")
            .unwrap_or(0);

        let elapsed = start.elapsed();
        let tok_s = if elapsed.as_millis() > 0 {
            (tokens_predicted as f64 / elapsed.as_millis() as f64) * 1000.0
        } else { 0.0 };

        log.info(&format!("Generated {} tokens in {:?} ({:.1} tok/s)",
            tokens_predicted, elapsed, tok_s));

        Ok((content, tokens_predicted))
    }

    /// Stop the server process.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.server_process.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
        }
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Drop for LlamaCppBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─── Minimal HTTP helpers (no external deps) ────────────────────────────────

fn ureq_get(url: &str) -> Result<u16, String> {
    let output = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "--connect-timeout", "2", url])
        .output()
        .map_err(|e| format!("curl failed: {e}"))?;
    let status: u16 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);
    Ok(status)
}

fn ureq_post(url: &str, body: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-s", "-X", "POST", "-H", "Content-Type: application/json",
               "-d", body, "--connect-timeout", "5", "--max-time", "300", url])
        .output()
        .map_err(|e| format!("curl failed: {e}"))?;
    if !output.status.success() {
        return Err(format!("curl error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn extract_json_field(json: &str, field: &str) -> Option<String> {
    // Simple JSON field extraction — no serde dependency
    let key = format!("\"{}\":", field);
    let start = json.find(&key)? + key.len();
    let rest = json[start..].trim_start();
    if rest.starts_with('"') {
        // String value
        let inner = &rest[1..];
        let end = inner.find('"')?;
        Some(inner[..end].replace("\\n", "\n").replace("\\\"", "\""))
    } else {
        // Number or other
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

fn extract_json_field_usize(json: &str, field: &str) -> Option<usize> {
    extract_json_field(json, field)?.parse().ok()
}

fn which_llama_server() -> Option<String> {
    for name in ["llama-server", "llama.cpp-server"] {
        if let Ok(output) = Command::new("which").arg(name).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    // Check common install locations
    for path in [
        "/opt/homebrew/bin/llama-server",
        "/usr/local/bin/llama-server",
    ] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}
