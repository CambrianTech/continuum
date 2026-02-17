//! Command Parameter Extraction
//!
//! Typed helpers for extracting parameters from IPC JSON values.
//! Eliminates repetitive `params.get("x").and_then(|v| v.as_str()).ok_or("Missing x")`
//! patterns across all ServiceModule command handlers.
//!
//! ONE source of truth for parameter extraction — every module uses this.

use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

/// Wrapper around serde_json::Value for typed parameter extraction.
///
/// Usage:
/// ```ignore
/// let p = Params::new(&params);
/// let persona_id = p.uuid("persona_id")?;
/// let text = p.str("response_text")?;
/// let verbose = p.bool_or("verbose", false);
/// let items: Vec<Item> = p.json("items")?;
/// ```
pub struct Params<'a>(pub &'a Value);

impl<'a> Params<'a> {
    pub fn new(value: &'a Value) -> Self {
        Self(value)
    }

    // ================================================================
    // String
    // ================================================================

    /// Required string parameter. Returns error if missing or not a string.
    pub fn str(&self, key: &str) -> Result<&'a str, String> {
        self.0.get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("Missing {key}"))
    }

    /// Optional string parameter. Returns None if missing.
    pub fn str_opt(&self, key: &str) -> Option<&'a str> {
        self.0.get(key).and_then(|v| v.as_str())
    }

    /// Optional string with default.
    pub fn str_or<'b>(&'a self, key: &str, default: &'b str) -> &'b str where 'a: 'b {
        self.str_opt(key).unwrap_or(default)
    }

    /// String with alias fallback (e.g. "system_prompt" or "systemPrompt").
    pub fn str_opt_alias(&self, key1: &str, key2: &str) -> Option<&'a str> {
        self.str_opt(key1).or_else(|| self.str_opt(key2))
    }

    // ================================================================
    // UUID
    // ================================================================

    /// Required UUID parameter. Parses from string.
    pub fn uuid(&self, key: &str) -> Result<Uuid, String> {
        let s = self.str(key)?;
        Uuid::parse_str(s).map_err(|e| format!("Invalid {key}: {e}"))
    }

    /// Optional UUID parameter.
    pub fn uuid_opt(&self, key: &str) -> Option<Uuid> {
        self.str_opt(key).and_then(|s| Uuid::parse_str(s).ok())
    }

    // ================================================================
    // Integers
    // ================================================================

    /// Required u64 parameter.
    pub fn u64(&self, key: &str) -> Result<u64, String> {
        self.0.get(key)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| format!("Missing {key}"))
    }

    /// Optional u64 parameter with default.
    pub fn u64_or(&self, key: &str, default: u64) -> u64 {
        self.0.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
    }

    /// Optional u64 (returns None if missing).
    pub fn u64_opt(&self, key: &str) -> Option<u64> {
        self.0.get(key).and_then(|v| v.as_u64())
    }

    /// Optional u32 (returns None if missing or value exceeds u32::MAX).
    pub fn u32_opt(&self, key: &str) -> Option<u32> {
        self.u64_opt(key).and_then(|n| u32::try_from(n).ok())
    }

    /// Optional i64.
    pub fn i64_opt(&self, key: &str) -> Option<i64> {
        self.0.get(key).and_then(|v| v.as_i64())
    }

    /// i64 with default.
    pub fn i64_or(&self, key: &str, default: i64) -> i64 {
        self.i64_opt(key).unwrap_or(default)
    }

    // ================================================================
    // Floats
    // ================================================================

    /// Required f64 parameter.
    pub fn f64(&self, key: &str) -> Result<f64, String> {
        self.f64_opt(key).ok_or_else(|| format!("Missing {key}"))
    }

    /// Optional f64 (returns None if missing).
    pub fn f64_opt(&self, key: &str) -> Option<f64> {
        self.0.get(key).and_then(|v| v.as_f64())
    }

    /// f64 with default (returns default if missing).
    pub fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.f64_opt(key).unwrap_or(default)
    }

    /// Required f32 parameter.
    pub fn f32(&self, key: &str) -> Result<f32, String> {
        self.f64(key).map(|f| f as f32)
    }

    /// Optional f32 (returns None if missing).
    pub fn f32_opt(&self, key: &str) -> Option<f32> {
        self.f64_opt(key).map(|f| f as f32)
    }

    /// Optional f64 parameter as f32 with default.
    pub fn f32_or(&self, key: &str, default: f32) -> f32 {
        self.f64_opt(key).map(|f| f as f32).unwrap_or(default)
    }

    /// Optional f64 with alias fallback (e.g. "top_p" or "topP").
    pub fn f64_opt_alias(&self, key1: &str, key2: &str) -> Option<f64> {
        self.f64_opt(key1).or_else(|| self.f64_opt(key2))
    }

    // ================================================================
    // Bool
    // ================================================================

    /// Optional bool (returns None if missing).
    pub fn bool_opt(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(|v| v.as_bool())
    }

    /// Optional bool parameter with default.
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.bool_opt(key).unwrap_or(default)
    }

    // ================================================================
    // Arrays
    // ================================================================

    /// Required array parameter.
    pub fn array(&self, key: &str) -> Result<&'a Vec<Value>, String> {
        self.array_opt(key).ok_or_else(|| format!("Missing {key}"))
    }

    /// Optional array parameter.
    pub fn array_opt(&self, key: &str) -> Option<&'a Vec<Value>> {
        self.0.get(key).and_then(|v| v.as_array())
    }

    // ================================================================
    // Typed deserialization (serde)
    // ================================================================

    /// Required typed parameter via serde deserialization.
    /// Use for complex types: `let items: Vec<Item> = p.json("items")?;`
    pub fn json<T: DeserializeOwned>(&self, key: &str) -> Result<T, String> {
        let v = self.0.get(key).ok_or_else(|| format!("Missing {key}"))?;
        serde_json::from_value(v.clone()).map_err(|e| format!("Invalid {key}: {e}"))
    }

    /// Optional typed parameter via serde deserialization, with default.
    pub fn json_or<T: DeserializeOwned + Default>(&self, key: &str) -> T {
        self.0.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Optional typed parameter via serde deserialization.
    pub fn json_opt<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.0.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    // ================================================================
    // Alias helpers (camelCase ↔ snake_case)
    // ================================================================

    /// Optional u64 with alias fallback.
    pub fn u64_opt_alias(&self, key1: &str, key2: &str) -> Option<u64> {
        self.u64_opt(key1).or_else(|| self.u64_opt(key2))
    }

    /// Optional string with alias, mapped to owned String.
    pub fn string_opt_alias(&self, key1: &str, key2: &str) -> Option<String> {
        self.str_opt_alias(key1, key2).map(String::from)
    }

    // ================================================================
    // Raw access
    // ================================================================

    /// Raw value access for complex types.
    pub fn value(&self, key: &str) -> Option<&'a Value> {
        self.0.get(key)
    }

    /// The underlying Value reference.
    pub fn inner(&self) -> &'a Value {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_str_required() {
        let v = json!({"name": "test"});
        let p = Params::new(&v);
        assert_eq!(p.str("name").unwrap(), "test");
        assert!(p.str("missing").is_err());
    }

    #[test]
    fn test_str_or() {
        let v = json!({"name": "test"});
        let p = Params::new(&v);
        assert_eq!(p.str_or("name", "default"), "test");
        assert_eq!(p.str_or("missing", "default"), "default");
    }

    #[test]
    fn test_uuid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let v = json!({"id": id});
        let p = Params::new(&v);
        assert_eq!(p.uuid("id").unwrap().to_string(), id);
        assert!(p.uuid("missing").is_err());
    }

    #[test]
    fn test_uuid_opt() {
        let v = json!({"id": "550e8400-e29b-41d4-a716-446655440000"});
        let p = Params::new(&v);
        assert!(p.uuid_opt("id").is_some());
        assert!(p.uuid_opt("missing").is_none());
    }

    #[test]
    fn test_bool_or() {
        let v = json!({"flag": true});
        let p = Params::new(&v);
        assert!(p.bool_or("flag", false));
        assert!(!p.bool_or("missing", false));
    }

    #[test]
    fn test_u64_or() {
        let v = json!({"count": 42});
        let p = Params::new(&v);
        assert_eq!(p.u64_or("count", 0), 42);
        assert_eq!(p.u64_or("missing", 99), 99);
    }

    #[test]
    fn test_u64_opt() {
        let v = json!({"count": 42});
        let p = Params::new(&v);
        assert_eq!(p.u64_opt("count"), Some(42));
        assert_eq!(p.u64_opt("missing"), None);
    }

    #[test]
    fn test_u32_opt() {
        let v = json!({"line": 100, "big": 5_000_000_000u64});
        let p = Params::new(&v);
        assert_eq!(p.u32_opt("line"), Some(100));
        assert_eq!(p.u32_opt("missing"), None);
        // Values exceeding u32::MAX return None instead of silently truncating
        assert_eq!(p.u32_opt("big"), None);
    }

    #[test]
    fn test_bool_opt() {
        let v = json!({"flag": true, "off": false});
        let p = Params::new(&v);
        assert_eq!(p.bool_opt("flag"), Some(true));
        assert_eq!(p.bool_opt("off"), Some(false));
        assert_eq!(p.bool_opt("missing"), None);
    }

    #[test]
    fn test_i64_or() {
        let v = json!({"offset": -10});
        let p = Params::new(&v);
        assert_eq!(p.i64_or("offset", 0), -10);
        assert_eq!(p.i64_or("missing", -1), -1);
    }

    #[test]
    fn test_f64_required() {
        let v = json!({"temp": 0.7});
        let p = Params::new(&v);
        assert!((p.f64("temp").unwrap() - 0.7).abs() < 0.001);
        assert!(p.f64("missing").is_err());
    }

    #[test]
    fn test_f64_or() {
        let v = json!({"temp": 0.9});
        let p = Params::new(&v);
        assert!((p.f64_or("temp", 0.5) - 0.9).abs() < 0.001);
        assert!((p.f64_or("missing", 0.5) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_array_naming() {
        let v = json!({"items": [1, 2, 3]});
        let p = Params::new(&v);
        // array() is required, array_opt() is optional
        assert!(p.array("items").is_ok());
        assert!(p.array("missing").is_err());
        assert!(p.array_opt("items").is_some());
        assert!(p.array_opt("missing").is_none());
    }

    #[test]
    fn test_f64_opt() {
        let v = json!({"temp": 0.7});
        let p = Params::new(&v);
        assert!((p.f64_opt("temp").unwrap() - 0.7).abs() < 0.001);
        assert!(p.f64_opt("missing").is_none());
    }

    #[test]
    fn test_f32_opt() {
        let v = json!({"temp": 0.7});
        let p = Params::new(&v);
        assert!((p.f32_opt("temp").unwrap() - 0.7).abs() < 0.01);
        assert!(p.f32_opt("missing").is_none());
    }

    #[test]
    fn test_json_required() {
        let v = json!({"items": ["a", "b", "c"]});
        let p = Params::new(&v);
        let items: Vec<String> = p.json("items").unwrap();
        assert_eq!(items, vec!["a", "b", "c"]);
        assert!(p.json::<Vec<String>>("missing").is_err());
    }

    #[test]
    fn test_json_or_default() {
        let v = json!({"items": ["a", "b"]});
        let p = Params::new(&v);
        let items: Vec<String> = p.json_or("items");
        assert_eq!(items, vec!["a", "b"]);
        let empty: Vec<String> = p.json_or("missing");
        assert!(empty.is_empty());
    }

    #[test]
    fn test_str_opt_alias() {
        let v = json!({"systemPrompt": "hello"});
        let p = Params::new(&v);
        assert_eq!(p.str_opt_alias("system_prompt", "systemPrompt"), Some("hello"));
        assert_eq!(p.str_opt_alias("system_prompt", "sys_prompt"), None);
    }

    #[test]
    fn test_f64_opt_alias() {
        let v = json!({"topP": 0.9});
        let p = Params::new(&v);
        assert!((p.f64_opt_alias("top_p", "topP").unwrap() - 0.9).abs() < 0.001);
        assert!(p.f64_opt_alias("top_p", "tp").is_none());
    }
}
