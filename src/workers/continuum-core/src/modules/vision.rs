//! VisionModule — Content-addressed cache + event notification for vision descriptions.
//!
//! Handles: vision/description-get, vision/description-put, vision/description-status,
//!          vision/cache-stats, vision/cache-warm, vision/cache-evict
//!
//! Architecture (OS-level thinking):
//! - L1: In-process HashMap (RwLock, zero-copy reads, bounded by max entries)
//! - L2: TypeScript ORM persistence (vision_descriptions collection in default DB)
//! - Notification: MessageBus event on description ready (no polling, no promises)
//! - Work queue: Content-key dedup prevents duplicate inference jobs
//!
//! The TS VisionDescriptionCache reads through this module:
//! 1. Check L1 (Rust HashMap via IPC) — sub-ms
//! 2. Miss → check L2 (TS ORM query) — ~5ms
//! 3. Miss → trigger inference (LLaVA 60-70s), result written back to both layers
//!
//! On server restart, TS warms L1 from L2 via vision/cache-warm.
//! Descriptions survive across deploys. One LLaVA call per unique image, forever.

use crate::log_info;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::runtime::MessageBus;

// ============================================================================
// Types
// ============================================================================

/// Cached vision description — the result of processing one image.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedDescription {
    /// The description text
    description: String,
    /// Model that generated it (e.g., "candle/llava:7b")
    model: String,
    /// Provider (e.g., "candle", "anthropic")
    provider: String,
    /// Inference time in ms
    processing_time_ms: u64,
    /// Confidence score (0.0-1.0)
    confidence: f64,
    /// When this was cached (unix ms)
    cached_at: u64,
    /// Last time this was accessed (unix ms) — for LRU eviction
    last_accessed_at: u64,
}

/// Cache statistics for diagnostics
#[derive(Debug, Serialize)]
struct CacheStats {
    entries: usize,
    max_entries: usize,
    hits: u64,
    misses: u64,
    hit_rate: f64,
    evictions: u64,
}

// ============================================================================
// Module
// ============================================================================

/// Max L1 cache entries. Each is ~1KB (description text + metadata).
const MAX_CACHE_ENTRIES: usize = 2000;

pub struct VisionModule {
    /// Content-addressed cache: content_key → description
    cache: RwLock<HashMap<String, CachedDescription>>,
    /// Cache hit/miss counters
    hits: RwLock<u64>,
    misses: RwLock<u64>,
    evictions: RwLock<u64>,
    /// Event bus for publishing vision:description:ready
    bus: RwLock<Option<Arc<MessageBus>>>,
}

impl VisionModule {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::with_capacity(256)),
            hits: RwLock::new(0),
            misses: RwLock::new(0),
            evictions: RwLock::new(0),
            bus: RwLock::new(None),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Get a description by content key. Bumps last_accessed_at on hit.
    fn handle_get(&self, params: Value) -> Result<CommandResult, String> {
        let content_key = params["content_key"]
            .as_str()
            .ok_or("Missing content_key")?;

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());

        if let Some(entry) = cache.get_mut(content_key) {
            entry.last_accessed_at = Self::now_ms();
            let result = json!({
                "found": true,
                "description": entry.description,
                "model": entry.model,
                "provider": entry.provider,
                "processing_time_ms": entry.processing_time_ms,
                "confidence": entry.confidence,
            });

            // Increment hits
            let mut hits = self.hits.write().unwrap_or_else(|e| e.into_inner());
            *hits += 1;

            Ok(CommandResult::Json(result))
        } else {
            // Increment misses
            let mut misses = self.misses.write().unwrap_or_else(|e| e.into_inner());
            *misses += 1;

            Ok(CommandResult::Json(json!({ "found": false })))
        }
    }

    /// Store a description. Publishes vision:description:ready event.
    fn handle_put(&self, params: Value) -> Result<CommandResult, String> {
        let content_key = params["content_key"]
            .as_str()
            .ok_or("Missing content_key")?
            .to_string();
        let description = params["description"]
            .as_str()
            .ok_or("Missing description")?
            .to_string();
        let model = params["model"].as_str().unwrap_or("unknown").to_string();
        let provider = params["provider"].as_str().unwrap_or("unknown").to_string();
        let processing_time_ms = params["processing_time_ms"].as_u64().unwrap_or(0);
        let confidence = params["confidence"].as_f64().unwrap_or(0.85);

        let now = Self::now_ms();
        let entry = CachedDescription {
            description: description.clone(),
            model: model.clone(),
            provider: provider.clone(),
            processing_time_ms,
            confidence,
            cached_at: now,
            last_accessed_at: now,
        };

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());

        // LRU eviction if at capacity
        if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(&content_key) {
            self.evict_lru(&mut cache);
        }

        cache.insert(content_key.clone(), entry);

        // Publish event — any TS consumer watching for this key gets notified
        let bus_guard = self.bus.read().unwrap_or_else(|e| e.into_inner());
        if let Some(bus) = bus_guard.as_ref() {
            bus.publish_async_only(
                "vision:description:ready",
                json!({
                    "content_key": content_key,
                    "description": description,
                    "model": model,
                    "provider": provider,
                }),
            );
        }

        Ok(CommandResult::Json(json!({ "stored": true })))
    }

    /// Check status of a content key: "cached", "none"
    fn handle_status(&self, params: Value) -> Result<CommandResult, String> {
        let content_key = params["content_key"]
            .as_str()
            .ok_or("Missing content_key")?;

        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let status = if cache.contains_key(content_key) {
            "cached"
        } else {
            "none"
        };

        Ok(CommandResult::Json(json!({ "status": status })))
    }

    /// Cache statistics
    fn handle_stats(&self) -> Result<CommandResult, String> {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let hits = *self.hits.read().unwrap_or_else(|e| e.into_inner());
        let misses = *self.misses.read().unwrap_or_else(|e| e.into_inner());
        let evictions = *self.evictions.read().unwrap_or_else(|e| e.into_inner());
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        let stats = CacheStats {
            entries: cache.len(),
            max_entries: MAX_CACHE_ENTRIES,
            hits,
            misses,
            hit_rate,
            evictions,
        };

        CommandResult::json(&stats)
    }

    /// Bulk warm cache from persisted L2 data.
    /// Called by TS on startup to restore descriptions from ORM.
    fn handle_warm(&self, params: Value) -> Result<CommandResult, String> {
        let entries = params["entries"]
            .as_array()
            .ok_or("Missing entries array")?;

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        let now = Self::now_ms();
        let mut loaded = 0u64;

        for entry in entries {
            let content_key = match entry["content_key"].as_str() {
                Some(k) => k.to_string(),
                None => continue,
            };
            let description = match entry["description"].as_str() {
                Some(d) => d.to_string(),
                None => continue,
            };

            if cache.len() >= MAX_CACHE_ENTRIES {
                break; // Don't overflow during warm
            }

            cache.insert(
                content_key,
                CachedDescription {
                    description,
                    model: entry["model"].as_str().unwrap_or("unknown").to_string(),
                    provider: entry["provider"].as_str().unwrap_or("unknown").to_string(),
                    processing_time_ms: entry["processing_time_ms"].as_u64().unwrap_or(0),
                    confidence: entry["confidence"].as_f64().unwrap_or(0.85),
                    cached_at: entry["cached_at"].as_u64().unwrap_or(now),
                    last_accessed_at: now,
                },
            );
            loaded += 1;
        }

        log_info!(
            "vision",
            "cache-warm",
            "Warmed {} entries from L2 persistence (total={})",
            loaded,
            cache.len()
        );

        Ok(CommandResult::Json(json!({
            "warmed": loaded,
            "total": cache.len(),
        })))
    }

    /// Manual eviction — remove entries not accessed within idle_ms.
    fn handle_evict(&self, params: Value) -> Result<CommandResult, String> {
        let idle_ms = params["idle_ms"].as_u64().unwrap_or(30 * 60 * 1000); // 30 min default
        let cutoff = Self::now_ms().saturating_sub(idle_ms);

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        let before = cache.len();

        cache.retain(|_, entry| entry.last_accessed_at >= cutoff);

        let evicted = before - cache.len();
        if evicted > 0 {
            let mut ev = self.evictions.write().unwrap_or_else(|e| e.into_inner());
            *ev += evicted as u64;
        }

        log_info!(
            "vision",
            "cache-evict",
            "Evicted {} idle entries (cutoff={}ms, remaining={})",
            evicted,
            idle_ms,
            cache.len()
        );

        Ok(CommandResult::Json(json!({
            "evicted": evicted,
            "remaining": cache.len(),
        })))
    }

    /// LRU eviction: remove the least recently accessed entry.
    fn evict_lru(&self, cache: &mut HashMap<String, CachedDescription>) {
        let oldest_key = cache
            .iter()
            .min_by_key(|(_, v)| v.last_accessed_at)
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest_key {
            cache.remove(&key);
            let mut ev = self.evictions.write().unwrap_or_else(|e| e.into_inner());
            *ev += 1;
        }
    }
}

#[async_trait]
impl ServiceModule for VisionModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "vision",
            priority: ModulePriority::Normal,
            command_prefixes: &["vision/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let mut bus = self.bus.write().unwrap_or_else(|e| e.into_inner());
        *bus = Some(ctx.bus.clone());
        log_info!("vision", "init", "VisionModule initialized (max_entries={})", MAX_CACHE_ENTRIES);
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "vision/description-get" => self.handle_get(params),
            "vision/description-put" => self.handle_put(params),
            "vision/description-status" => self.handle_status(params),
            "vision/cache-stats" => self.handle_stats(),
            "vision/cache-warm" => self.handle_warm(params),
            "vision/cache-evict" => self.handle_evict(params),
            _ => Err(format!("Unknown vision command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_put_and_get() {
        let module = VisionModule::new();

        // Put a description
        let put_result = module
            .handle_command(
                "vision/description-put",
                json!({
                    "content_key": "abc123",
                    "description": "A cat sitting on a keyboard",
                    "model": "llava:7b",
                    "provider": "candle",
                    "processing_time_ms": 65000,
                    "confidence": 0.85,
                }),
            )
            .await;
        assert!(put_result.is_ok());

        // Get it back
        let get_result = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "abc123" }),
            )
            .await;
        assert!(get_result.is_ok());
        if let Ok(CommandResult::Json(json)) = get_result {
            assert_eq!(json["found"], true);
            assert_eq!(json["description"], "A cat sitting on a keyboard");
            assert_eq!(json["model"], "llava:7b");
        }
    }

    #[tokio::test]
    async fn test_miss() {
        let module = VisionModule::new();
        let result = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "nonexistent" }),
            )
            .await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["found"], false);
        }
    }

    #[tokio::test]
    async fn test_stats() {
        let module = VisionModule::new();

        // One put, one hit, one miss
        let _ = module
            .handle_command(
                "vision/description-put",
                json!({
                    "content_key": "key1",
                    "description": "test",
                }),
            )
            .await;

        let _ = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "key1" }),
            )
            .await; // hit

        let _ = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "key2" }),
            )
            .await; // miss

        let stats = module
            .handle_command("vision/cache-stats", json!({}))
            .await;
        assert!(stats.is_ok());
        if let Ok(CommandResult::Json(json)) = stats {
            assert_eq!(json["entries"], 1);
            assert_eq!(json["hits"], 1);
            assert_eq!(json["misses"], 1);
            assert_eq!(json["hit_rate"], 0.5);
        }
    }

    #[tokio::test]
    async fn test_warm() {
        let module = VisionModule::new();

        let result = module
            .handle_command(
                "vision/cache-warm",
                json!({
                    "entries": [
                        { "content_key": "a", "description": "image a", "model": "llava", "provider": "candle" },
                        { "content_key": "b", "description": "image b", "model": "llava", "provider": "candle" },
                        { "content_key": "c", "description": "image c", "model": "llava", "provider": "candle" },
                    ]
                }),
            )
            .await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["warmed"], 3);
            assert_eq!(json["total"], 3);
        }

        // Verify all three are accessible
        let get = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "b" }),
            )
            .await;
        if let Ok(CommandResult::Json(json)) = get {
            assert_eq!(json["found"], true);
            assert_eq!(json["description"], "image b");
        }
    }

    #[tokio::test]
    async fn test_eviction() {
        let module = VisionModule::new();

        // Add an entry with old timestamp
        {
            let mut cache = module.cache.write().unwrap();
            cache.insert(
                "old_key".to_string(),
                CachedDescription {
                    description: "old".to_string(),
                    model: "test".to_string(),
                    provider: "test".to_string(),
                    processing_time_ms: 0,
                    confidence: 0.5,
                    cached_at: 0,
                    last_accessed_at: 0, // Very old
                },
            );
            cache.insert(
                "new_key".to_string(),
                CachedDescription {
                    description: "new".to_string(),
                    model: "test".to_string(),
                    provider: "test".to_string(),
                    processing_time_ms: 0,
                    confidence: 0.5,
                    cached_at: VisionModule::now_ms(),
                    last_accessed_at: VisionModule::now_ms(),
                },
            );
        }

        // Evict entries idle for more than 1ms (the old one)
        let result = module
            .handle_command(
                "vision/cache-evict",
                json!({ "idle_ms": 1000 }), // 1 second
            )
            .await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["evicted"], 1);
            assert_eq!(json["remaining"], 1);
        }

        // Old key gone, new key remains
        let old = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "old_key" }),
            )
            .await;
        if let Ok(CommandResult::Json(json)) = old {
            assert_eq!(json["found"], false);
        }

        let new = module
            .handle_command(
                "vision/description-get",
                json!({ "content_key": "new_key" }),
            )
            .await;
        if let Ok(CommandResult::Json(json)) = new {
            assert_eq!(json["found"], true);
        }
    }
}
