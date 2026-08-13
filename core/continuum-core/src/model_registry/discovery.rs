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

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/CostPer1kTokens.ts"
)]
pub struct CostPer1kTokens {
    pub input: f64,
    pub output: f64,
}

/// Discovered model metadata — the raw `/v1/models` (or provider-API) listing
/// before it is folded into a `Model` and registered in the live catalog.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/DiscoveredModel.ts"
)]
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
    /// OpenRouter-shape modality block. The base OpenAI `/v1/models` spec omits
    /// this; capability-rich gateways (OpenRouter, some Together rows) publish
    /// which raw modalities the model ingests/emits. Absent → no caps derived,
    /// NOT a name guess.
    #[serde(default)]
    architecture: Option<OpenAiArchitecture>,
    /// OpenRouter-shape list of accepted request params (e.g. `"tools"`,
    /// `"response_format"`). The authoritative statement that tool-calling is
    /// honored — the alternative to sniffing "does this id look like a
    /// function-calling model".
    #[serde(default)]
    supported_parameters: Vec<String>,
}

/// The modality block some OpenAI-compatible gateways attach to each listing
/// row. Values are the provider's own vocabulary (`"text"`, `"image"`,
/// `"audio"`, `"file"`); we read them literally rather than inferring modality
/// from the model id.
#[derive(Debug, Default, Deserialize)]
struct OpenAiArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

/// Map the authoritative fields of one OpenAI-compatible listing row to the
/// canonical [`Capability`](crate::model_registry::types::Capability) kebab
/// vocabulary. Reads ONLY what the API states — input `"image"`⇒`vision`,
/// input `"audio"`⇒`audio-input`, output `"audio"`⇒`audio-output`, a
/// `"tools"` param⇒`tool-use`. Returns `None` when the provider published
/// nothing capability-bearing (base OpenAI spec), so the catalog/static
/// override — not a guess — supplies caps for those models.
fn capabilities_from_openai(model: &OpenAIModel) -> Option<Vec<String>> {
    let mut caps = Vec::new();
    if let Some(arch) = &model.architecture {
        if arch.input_modalities.iter().any(|m| m == "image") {
            caps.push("vision".to_string());
        }
        if arch.input_modalities.iter().any(|m| m == "audio") {
            caps.push("audio-input".to_string());
        }
        if arch.output_modalities.iter().any(|m| m == "audio") {
            caps.push("audio-output".to_string());
        }
    }
    if model.supported_parameters.iter().any(|p| p == "tools") {
        caps.push("tool-use".to_string());
    }
    if caps.is_empty() {
        None
    } else {
        Some(caps)
    }
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
    /// The generation methods the model supports (e.g. `"generateContent"`,
    /// `"embedContent"`). Gemini's authoritative statement of what the model
    /// DOES — the alternative to guessing "is this an embedding model" from
    /// the id. Modalities aren't in this listing, so vision/audio caps come
    /// from the catalog override, not a guess.
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

/// Map a Gemini listing row's `supportedGenerationMethods` to the canonical
/// capability vocabulary. Only what the API states: `"embedContent"` ⇒
/// `embedding`. Returns `None` when nothing capability-bearing is present.
fn capabilities_from_gemini(model: &GeminiModel) -> Option<Vec<String>> {
    let mut caps = Vec::new();
    if model
        .supported_generation_methods
        .iter()
        .any(|m| m == "embedContent")
    {
        caps.push("embedding".to_string());
    }
    if caps.is_empty() {
        None
    } else {
        Some(caps)
    }
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
    let url = crate::ai::openai_endpoints::OpenAiBase::new(&config.base_url).models();

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

            // Derive caps from the row's authoritative modality/param fields
            // BEFORE moving `m.id` — absent fields yield None, never a guess.
            let capabilities = capabilities_from_openai(&m);
            Some(DiscoveredModel {
                model_id: m.id,
                context_window,
                max_output_tokens: m.max_tokens,
                provider: config.provider_id.clone(),
                capabilities,
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

            let capabilities = capabilities_from_gemini(&m);
            Some(DiscoveredModel {
                model_id,
                context_window,
                max_output_tokens: m.output_token_limit,
                provider: config.provider_id.clone(),
                capabilities,
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

    // what this catches: a capability-rich (OpenRouter-shape) listing row must
    // yield caps from its OWN modality/param fields — image input → vision,
    // audio in/out → audio-input/audio-output, a "tools" param → tool-use — in
    // the canonical kebab vocabulary so they round-trip into a Model's
    // Capability set. This is the cloud analog of mmproj hydration: the model
    // is sighted/hearing/tool-using because the API SAID SO, not because its id
    // looked multimodal.
    #[test]
    fn openai_capabilities_come_from_the_api_modality_fields() {
        let json = r#"{
            "id":"anthropic/claude-omni",
            "context_length":200000,
            "architecture":{
                "input_modalities":["text","image","audio"],
                "output_modalities":["text","audio"]
            },
            "supported_parameters":["tools","temperature"]
        }"#;
        let model: OpenAIModel = serde_json::from_str(json).unwrap();
        let caps = capabilities_from_openai(&model).expect("rich row must yield caps");
        assert!(caps.contains(&"vision".to_string()));
        assert!(caps.contains(&"audio-input".to_string()));
        assert!(caps.contains(&"audio-output".to_string()));
        assert!(caps.contains(&"tool-use".to_string()));
    }

    // what this catches: a plain OpenAI-spec row (no architecture block, no
    // supported_parameters) yields None, NOT an empty vec or a guessed cap. The
    // registry then leans on the catalog/static override for that model rather
    // than inventing capabilities the API never claimed — fail-honest, not
    // fabricate.
    #[test]
    fn openai_bare_row_yields_no_capabilities() {
        let json = r#"{"id":"gpt-4","context_length":128000}"#;
        let model: OpenAIModel = serde_json::from_str(json).unwrap();
        assert!(capabilities_from_openai(&model).is_none());
    }

    // what this catches: Gemini's authoritative capability signal is
    // supportedGenerationMethods, not the id — an "embedContent" method makes
    // the model an embedder. A model listing only generateContent yields no
    // derived caps (its modalities come from the catalog, absent from the API).
    #[test]
    fn gemini_embedding_capability_comes_from_generation_methods() {
        let embed = r#"{"name":"models/text-embedding-004","inputTokenLimit":2048,"supportedGenerationMethods":["embedContent"]}"#;
        let m: GeminiModel = serde_json::from_str(embed).unwrap();
        assert_eq!(
            capabilities_from_gemini(&m),
            Some(vec!["embedding".to_string()])
        );

        let chat = r#"{"name":"models/gemini-2.0-flash","inputTokenLimit":1048576,"supportedGenerationMethods":["generateContent"]}"#;
        let m2: GeminiModel = serde_json::from_str(chat).unwrap();
        assert!(capabilities_from_gemini(&m2).is_none());
    }
}
