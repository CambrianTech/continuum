//! Model Discovery — queries provider APIs for model metadata
//!
//! ALL HTTP I/O runs in the Rust process (off the Node.js main thread).
//! TypeScript sends provider configs via IPC → Rust fetches model lists →
//! returns discovered metadata via IPC → TypeScript populates ModelRegistry.
//!
//! Providers supported:
//! - OpenAI-compatible APIs (Groq, OpenAI, Fireworks, xAI, DeepSeek)
//! - Together AI (OpenAI-compatible but with `context_length` field)
//! - Google Gemini (REST API with different response format)
//! - Static providers (Anthropic — no listing API, pre-configured models)

use crate::{log_error, log_info};
use serde::{Deserialize, Serialize};

/// Provider config sent from TypeScript via IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: String,
    /// Pre-configured models for providers without a listing API (e.g. Anthropic)
    #[serde(default)]
    pub static_models: Vec<StaticModel>,
}

/// Static model definition (for providers that don't have a listing endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticModel {
    pub id: String,
    pub context_window: u32,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub cost_per_1k_tokens: Option<CostPer1kTokens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostPer1kTokens {
    pub input: f64,
    pub output: f64,
}

/// Discovered model metadata returned to TypeScript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(rename = "contextWindow")]
    pub context_window: u32,
    #[serde(rename = "maxOutputTokens")]
    pub max_output_tokens: Option<u32>,
    pub provider: String,
    pub capabilities: Option<Vec<String>>,
    #[serde(rename = "costPer1kTokens")]
    pub cost_per_1k_tokens: Option<CostPer1kTokens>,
    #[serde(rename = "discoveredAt")]
    pub discovered_at: u64,
}

/// Response from OpenAI-compatible /v1/models endpoint
#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAIModel {
    id: String,
    #[serde(default)]
    context_length: Option<u32>,
    #[serde(default)]
    context_window: Option<u32>,
    #[serde(default)]
    max_input_tokens: Option<u32>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

/// Response from Google Gemini API
#[derive(Debug, Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModel {
    /// e.g. "models/gemini-2.0-flash"
    name: String,
    #[serde(default)]
    input_token_limit: Option<u32>,
    #[serde(default)]
    output_token_limit: Option<u32>,
}

/// Discover models from all providers concurrently.
/// Returns all discovered models (empty vec per provider on failure, never errors overall).
pub async fn discover_all(providers: Vec<ProviderConfig>) -> Vec<DiscoveredModel> {
    let mut handles = Vec::with_capacity(providers.len());

    for config in providers {
        handles.push(tokio::spawn(
            async move { discover_provider(&config).await },
        ));
    }

    let mut all_models = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(models) => all_models.extend(models),
            Err(e) => {
                log_error!("models", "discovery", "Task panicked: {}", e);
            }
        }
    }

    all_models
}

/// Discover models from a single provider
async fn discover_provider(config: &ProviderConfig) -> Vec<DiscoveredModel> {
    // Static models (no HTTP needed)
    if !config.static_models.is_empty() {
        return use_static_models(config);
    }

    let result = if config.provider_id == "google" {
        fetch_gemini(config).await
    } else {
        // OpenAI-compatible: Groq, OpenAI, Fireworks, xAI, DeepSeek, Together
        fetch_openai_compatible(config).await
    };

    match result {
        Ok(models) => {
            log_info!(
                "models",
                "discovery",
                "Discovered {} models from {}",
                models.len(),
                config.provider_id
            );
            models
        }
        Err(e) => {
            log_error!(
                "models",
                "discovery",
                "Failed to discover models from {}: {}",
                config.provider_id,
                e
            );
            Vec::new()
        }
    }
}

/// Fetch from OpenAI-compatible /v1/models endpoint
async fn fetch_openai_compatible(config: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let url = format!("{}/v1/models", config.base_url);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let data: OpenAIModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let now = timestamp_ms();
    let models: Vec<DiscoveredModel> = data
        .data
        .into_iter()
        .filter_map(|m| {
            let context_window = m
                .context_length
                .or(m.context_window)
                .or(m.max_input_tokens)?;

            // Only keep models with known context windows
            if context_window == 0 {
                return None;
            }

            Some(DiscoveredModel {
                model_id: m.id,
                context_window,
                max_output_tokens: m.max_tokens,
                provider: config.provider_id.clone(),
                capabilities: None,
                cost_per_1k_tokens: None,
                discovered_at: now,
            })
        })
        .collect();

    Ok(models)
}

/// Fetch from Google Gemini models API
async fn fetch_gemini(config: &ProviderConfig) -> Result<Vec<DiscoveredModel>, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={}",
        config.api_key
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let data: GeminiModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let now = timestamp_ms();
    let models: Vec<DiscoveredModel> = data
        .models
        .into_iter()
        .filter_map(|m| {
            let context_window = m.input_token_limit?;
            if context_window == 0 {
                return None;
            }

            // Strip "models/" prefix: "models/gemini-2.0-flash" → "gemini-2.0-flash"
            let model_id = m
                .name
                .strip_prefix("models/")
                .unwrap_or(&m.name)
                .to_string();

            Some(DiscoveredModel {
                model_id,
                context_window,
                max_output_tokens: m.output_token_limit,
                provider: config.provider_id.clone(),
                capabilities: None,
                cost_per_1k_tokens: None,
                discovered_at: now,
            })
        })
        .collect();

    Ok(models)
}

/// Return pre-configured models for providers without a listing API
fn use_static_models(config: &ProviderConfig) -> Vec<DiscoveredModel> {
    let now = timestamp_ms();
    config
        .static_models
        .iter()
        .map(|m| DiscoveredModel {
            model_id: m.id.clone(),
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            provider: config.provider_id.clone(),
            capabilities: if m.capabilities.is_empty() {
                None
            } else {
                Some(m.capabilities.clone())
            },
            cost_per_1k_tokens: m.cost_per_1k_tokens.clone(),
            discovered_at: now,
        })
        .collect()
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_models() {
        let config = ProviderConfig {
            provider_id: "anthropic".into(),
            api_key: "test".into(),
            base_url: "https://api.anthropic.com".into(),
            static_models: vec![StaticModel {
                id: "claude-sonnet-4-5-20250929".into(),
                context_window: 200000,
                max_output_tokens: Some(8192),
                capabilities: vec!["text-generation".into()],
                cost_per_1k_tokens: None,
            }],
        };

        let result = use_static_models(&config);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].model_id, "claude-sonnet-4-5-20250929");
        assert_eq!(result[0].context_window, 200000);
        assert_eq!(result[0].provider, "anthropic");
    }

    #[test]
    fn test_openai_model_parsing() {
        // Verify our struct handles various field names
        let json = r#"{"id":"gpt-4","context_length":128000,"max_tokens":4096}"#;
        let model: OpenAIModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "gpt-4");
        assert_eq!(model.context_length, Some(128000));
        assert_eq!(model.max_tokens, Some(4096));

        // Groq-style with context_window
        let json2 = r#"{"id":"llama-3.1-8b","context_window":131072}"#;
        let model2: OpenAIModel = serde_json::from_str(json2).unwrap();
        assert_eq!(model2.context_window, Some(131072));

        // Together-style with max_input_tokens
        let json3 = r#"{"id":"meta-llama/Meta-Llama-3.1-70B","max_input_tokens":128000}"#;
        let model3: OpenAIModel = serde_json::from_str(json3).unwrap();
        assert_eq!(model3.max_input_tokens, Some(128000));
    }

    #[test]
    fn test_gemini_model_parsing() {
        let json = r#"{"name":"models/gemini-2.0-flash","inputTokenLimit":1048576,"outputTokenLimit":8192}"#;
        let model: GeminiModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "models/gemini-2.0-flash");
        assert_eq!(model.input_token_limit, Some(1048576));
        assert_eq!(model.output_token_limit, Some(8192));
    }
}
