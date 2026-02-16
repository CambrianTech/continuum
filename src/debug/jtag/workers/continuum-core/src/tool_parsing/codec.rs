//! Tool name codec — bidirectional encode/decode for API constraints.
//!
//! API constraint: Anthropic/OpenAI require tool names matching `[a-zA-Z0-9_-]{1,64}`.
//! Our tools use slashes: `code/write`, `collaboration/chat/send`.
//!
//! Encode: `code/write` -> `code_write` (slashes -> underscore)
//! Decode: ANY model-produced variant -> original name (via reverse lookup)
//!
//! Models mangle names in unpredictable ways:
//!   code__write, $FUNCTIONS.code_write, code_write, code-write, etc.
//! The codec handles all of these by registering normalized variants at startup.

use std::collections::{HashMap, HashSet};
use parking_lot::RwLock;

pub struct ToolNameCodec {
    originals: RwLock<HashSet<String>>,
    reverse_map: RwLock<HashMap<String, String>>,
}

impl ToolNameCodec {
    pub fn new() -> Self {
        Self {
            originals: RwLock::new(HashSet::new()),
            reverse_map: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool name and all plausible encoded/mangled variants.
    pub fn register(&self, tool_name: &str) {
        let mut originals = self.originals.write();
        let mut reverse = self.reverse_map.write();

        originals.insert(tool_name.to_string());
        reverse.insert(tool_name.to_string(), tool_name.to_string());

        // Canonical: slash -> underscore
        let encoded = tool_name.replace('/', "_");
        reverse.insert(encoded, tool_name.to_string());

        // Double underscore (legacy encoding)
        let double = tool_name.replace('/', "__");
        reverse.insert(double, tool_name.to_string());

        // Hyphen variant
        reverse.insert(tool_name.replace('/', "-"), tool_name.to_string());

        // Dot variant
        reverse.insert(tool_name.replace('/', "."), tool_name.to_string());
    }

    /// Register multiple tool names at once.
    pub fn register_all(&self, tools: &[String]) {
        for name in tools {
            self.register(name);
        }
    }

    /// Encode a tool name for API transmission: slashes -> underscores.
    pub fn encode(&self, tool_name: &str) -> String {
        tool_name.replace('/', "_")
    }

    /// Decode any model-produced tool name variant back to the original.
    /// 5-step resolution: exact -> strip prefix -> normalize -> double-underscore -> single-underscore.
    pub fn decode(&self, raw: &str) -> String {
        let reverse = self.reverse_map.read();
        let originals = self.originals.read();

        // 1. Exact match
        if let Some(orig) = reverse.get(raw) {
            return orig.clone();
        }

        // 2. Strip known model prefixes ($FUNCTIONS., functions., $tools.)
        let cleaned = strip_model_prefix(raw);
        if let Some(orig) = reverse.get(cleaned) {
            return orig.clone();
        }

        // 3. Normalize all separators to underscore, lowercase
        let normalized = cleaned.replace(['-', '.', '_'], "_").to_lowercase();
        if let Some(orig) = reverse.get(&normalized) {
            return orig.clone();
        }

        // 4. Try reconstructing: double underscore -> slash
        let double_slashed = cleaned.replace("__", "/");
        if originals.contains(&double_slashed) {
            return double_slashed;
        }

        // 5. Try reconstructing: single underscore -> slash
        let single_slashed = cleaned.replace('_', "/");
        if originals.contains(&single_slashed) {
            return single_slashed;
        }

        // Last resort: best-effort double-underscore reconstruction
        double_slashed
    }

    /// Get count of registered tool names.
    pub fn count(&self) -> usize {
        self.originals.read().len()
    }
}

/// Strip model-added prefixes: $FUNCTIONS., functions., $tools., etc.
fn strip_model_prefix(raw: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "$FUNCTIONS.", "$functions.", "FUNCTIONS.", "functions.",
        "$tools.", "$TOOLS.", "tools.", "TOOLS.",
    ];
    for prefix in PREFIXES {
        if let Some(stripped) = raw.strip_prefix(prefix) {
            return stripped;
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec_with_tools() -> ToolNameCodec {
        let codec = ToolNameCodec::new();
        codec.register("code/write");
        codec.register("code/read");
        codec.register("code/search");
        codec.register("code/tree");
        codec.register("collaboration/chat/send");
        codec.register("collaboration/decision/vote");
        codec.register("ai/generate");
        codec
    }

    // ─── Encode ─────────────────────────────────────────────────

    #[test]
    fn encode_basic() {
        let codec = ToolNameCodec::new();
        assert_eq!(codec.encode("code/write"), "code_write");
        assert_eq!(codec.encode("collaboration/chat/send"), "collaboration_chat_send");
    }

    // ─── Decode: Step 1 (exact match) ───────────────────────────

    #[test]
    fn decode_exact_original() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("code/write"), "code/write");
    }

    #[test]
    fn decode_exact_encoded() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("code_write"), "code/write");
    }

    #[test]
    fn decode_exact_double_underscore() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("code__write"), "code/write");
    }

    #[test]
    fn decode_exact_hyphen() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("code-write"), "code/write");
    }

    #[test]
    fn decode_exact_dot() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("code.write"), "code/write");
    }

    // ─── Decode: Step 2 (strip prefix) ──────────────────────────

    #[test]
    fn decode_strip_functions_prefix() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("$FUNCTIONS.code_write"), "code/write");
    }

    #[test]
    fn decode_strip_tools_prefix() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("$tools.code_write"), "code/write");
    }

    #[test]
    fn decode_strip_lowercase_functions() {
        let codec = codec_with_tools();
        assert_eq!(codec.decode("functions.code_write"), "code/write");
    }

    // ─── Decode: Step 3 (normalize) ─────────────────────────────

    #[test]
    fn decode_case_insensitive() {
        let codec = codec_with_tools();
        // "CODE_WRITE" normalizes to "code_write" which is in reverse map
        assert_eq!(codec.decode("CODE_WRITE"), "code/write");
    }

    // ─── Decode: Steps 4-5 (reconstruct) ────────────────────────

    #[test]
    fn decode_double_underscore_reconstruct() {
        let codec = codec_with_tools();
        // collaboration__chat__send → collaboration/chat/send
        assert_eq!(codec.decode("collaboration__chat__send"), "collaboration/chat/send");
    }

    #[test]
    fn decode_single_underscore_reconstruct() {
        let codec = codec_with_tools();
        // collaboration_chat_send → collaboration/chat/send
        assert_eq!(codec.decode("collaboration_chat_send"), "collaboration/chat/send");
    }

    // ─── Decode: unknown ────────────────────────────────────────

    #[test]
    fn decode_unknown_returns_best_effort() {
        let codec = codec_with_tools();
        // Completely unknown tool — returns double-underscore reconstruction
        let result = codec.decode("totally__unknown__tool");
        assert_eq!(result, "totally/unknown/tool");
    }

    // ─── Multi-level paths ──────────────────────────────────────

    #[test]
    fn multi_level_roundtrip() {
        let codec = codec_with_tools();
        let original = "collaboration/decision/vote";
        let encoded = codec.encode(original);
        assert_eq!(encoded, "collaboration_decision_vote");
        let decoded = codec.decode(&encoded);
        assert_eq!(decoded, original);
    }

    // ─── Count ──────────────────────────────────────────────────

    #[test]
    fn count_registered() {
        let codec = codec_with_tools();
        assert_eq!(codec.count(), 7);
    }

    // ─── register_all ───────────────────────────────────────────

    #[test]
    fn register_all_batch() {
        let codec = ToolNameCodec::new();
        let tools = vec![
            "code/write".to_string(),
            "code/read".to_string(),
            "data/list".to_string(),
        ];
        codec.register_all(&tools);
        assert_eq!(codec.count(), 3);
        assert_eq!(codec.decode("code_write"), "code/write");
        assert_eq!(codec.decode("data_list"), "data/list");
    }
}
