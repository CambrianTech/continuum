//! Typed OpenAI-compatible endpoint construction — the strict boundary between
//! "a configured base URL" (any shape) and "a request URL" (exact protocol path).
//!
//! The boundary used to be a stringly-typed convention: a `base_url: String`
//! plus `format!("{}/v1/...")` repeated at every request site. Nothing enforced
//! the convention, and two sources fed the field with DIFFERENT shapes:
//!   - the registry catalog stores host roots — `https://api.openai.com`,
//!     and even a meaningful path prefix like Groq's `https://api.groq.com/openai`;
//!   - Contract A's serving snapshot stores the `/v1` URL
//!     `http://127.0.0.1:58057/v1`, because that is the user-facing OpenAI shape
//!     `serving/status` renders.
//! With no boundary, the snapshot shape reached the `/v1`-appending sites and
//! doubled to `/v1/v1/chat/completions` → llama-server 404 → every persona turn
//! silently abstained ("Asha mute").
//!
//! [`OpenAiBase`] makes the boundary strict: it absorbs any input shape at
//! construction, normalizes ONCE to the host root, and is the ONLY way to reach
//! a request URL — via typed accessors, never string concatenation. Past the
//! constructor the shape is unambiguous, and the protocol's path layout (OpenAI
//! under `/v1`, llama.cpp's `/lora-adapters` deliberately NOT under `/v1`) lives
//! in exactly one place where it cannot be doubled or mistyped.

/// A normalized OpenAI-compatible API base: the host root that endpoint paths
/// hang off. Construct with [`OpenAiBase::new`] from any configured or
/// runtime-override URL shape; request URLs are reachable only through the typed
/// accessor methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiBase {
    /// Host root, normalized: no trailing slash, no trailing `/v1`. A meaningful
    /// path PREFIX (Groq's `/openai`) is preserved.
    root: String,
}

impl OpenAiBase {
    /// Normalize any base-URL shape to the host root. Removes a trailing slash,
    /// then a trailing `/v1` version segment — and *only* that segment, so a
    /// meaningful path prefix survives. Both feeding shapes converge here:
    /// ```text
    ///   catalog   https://api.openai.com        ->  https://api.openai.com
    ///   catalog   https://api.groq.com/openai   ->  https://api.groq.com/openai
    ///   snapshot  http://127.0.0.1:58057/v1     ->  http://127.0.0.1:58057
    /// ```
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim_end_matches('/');
        let root = trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string();
        Self { root }
    }

    /// The bare host root (no `/v1`) — for endpoints outside the versioned
    /// namespace.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// `POST /v1/chat/completions`
    pub fn chat_completions(&self) -> String {
        self.v1("chat/completions")
    }

    /// `GET /v1/models`
    pub fn models(&self) -> String {
        self.v1("models")
    }

    /// `POST /v1/embeddings`
    pub fn embeddings(&self) -> String {
        self.v1("embeddings")
    }

    /// `GET /lora-adapters` — llama.cpp's LoRA catalog, deliberately NOT under
    /// `/v1`. Encoded here so the one site that needs the un-versioned path can't
    /// drift back to a `/v1` assumption.
    pub fn lora_adapters(&self) -> String {
        format!("{}/lora-adapters", self.root)
    }

    /// `GET /props` — llama.cpp's server-properties endpoint (also NOT under
    /// `/v1`). Reports `total_slots` (the `--parallel` count), which the
    /// adapter's slot-affinity table sizes itself from — discovered, never
    /// hardcoded.
    pub fn props(&self) -> String {
        format!("{}/props", self.root)
    }

    /// Build a `/v1/<path>` URL against the normalized root.
    fn v1(&self, path: &str) -> String {
        format!("{}/v1/{path}", self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE Asha-mute bug. The serving snapshot's base_url is
    // the `…:58057/v1` URL; fed verbatim to a `/v1`-appending builder it doubled
    // to `/v1/v1/chat/completions` → llama-server 404 → every persona turn
    // abstained. new() must collapse the snapshot shape to the host root so the
    // typed accessor yields a single `/v1`.
    #[test]
    fn normalizes_snapshot_v1_shape_to_single_v1_endpoint() {
        let base = OpenAiBase::new("http://127.0.0.1:58057/v1");
        assert_eq!(base.root(), "http://127.0.0.1:58057");
        assert_eq!(
            base.chat_completions(),
            "http://127.0.0.1:58057/v1/chat/completions"
        );
        // a trailing slash on the snapshot shape collapses identically
        assert_eq!(
            OpenAiBase::new("http://127.0.0.1:58057/v1/").chat_completions(),
            "http://127.0.0.1:58057/v1/chat/completions"
        );
    }

    // what this catches: a bare catalog host must pass through untouched and gain
    // exactly one `/v1` per endpoint.
    #[test]
    fn bare_catalog_host_gains_one_v1() {
        let base = OpenAiBase::new("https://api.openai.com");
        assert_eq!(base.models(), "https://api.openai.com/v1/models");
        assert_eq!(base.embeddings(), "https://api.openai.com/v1/embeddings");
    }

    // what this catches: a meaningful path PREFIX (Groq's `/openai`) must be
    // preserved — only the redundant `/v1` version segment is stripped, never an
    // arbitrary last path component.
    #[test]
    fn preserves_meaningful_path_prefix() {
        let base = OpenAiBase::new("https://api.groq.com/openai");
        assert_eq!(
            base.chat_completions(),
            "https://api.groq.com/openai/v1/chat/completions"
        );
    }

    // what this catches: llama.cpp's LoRA catalog is NOT under `/v1`; the typed
    // accessor must hang it off the bare root regardless of input shape.
    #[test]
    fn lora_adapters_is_not_under_v1() {
        assert_eq!(
            OpenAiBase::new("http://127.0.0.1:58057/v1").lora_adapters(),
            "http://127.0.0.1:58057/lora-adapters"
        );
    }
}
