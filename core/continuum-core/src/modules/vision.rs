//! VisionModule — Content-addressed cache + event notification for vision descriptions.
//!
//! The six `vision/*` verbs (description-get, description-put, description-status,
//! cache-stats, cache-warm, cache-evict) are typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s under `commands/vision/`,
//! capturing the shared [`VisionCache`] this module owns. The module shell wires the
//! event bus into the cache on init and exposes the commands via
//! [`ServiceModule::commands`]; its legacy `handle_command` fails loud.
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
//!
//! Wire keys here are snake_case (`content_key`, `processing_time_ms`, `idle_ms`) —
//! the typed params/results below carry snake_case field names with NO `rename_all`,
//! so the contract matches the established wire exactly.

use crate::log_info;
use crate::runtime::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use ts_rs::TS;

// ============================================================================
// Internal cache entry
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

// ============================================================================
// Command params — typed input contracts for the `vision/*` verbs.
// snake_case field names, NO rename_all → wire keys preserved verbatim.
// ============================================================================

/// Params for `vision/description-get` and `vision/description-status` — both
/// address one cache entry by its content key (compression: one shared type).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionKeyParams.ts"
)]
pub struct VisionKeyParams {
    /// Content-addressed key (e.g. SHA-256 of the image bytes).
    pub content_key: String,
}

/// Params for `vision/description-put` — store one description under a content key.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionPutParams.ts"
)]
pub struct VisionPutParams {
    /// Content-addressed key the description is stored under.
    pub content_key: String,
    /// The description text.
    pub description: String,
    /// Model that generated it (default `unknown`).
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// Provider that generated it (default `unknown`).
    #[serde(default)]
    #[ts(optional)]
    pub provider: Option<String>,
    /// Inference time in ms (default 0).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub processing_time_ms: Option<u64>,
    /// Confidence score 0.0–1.0 (default 0.85).
    #[serde(default)]
    #[ts(optional)]
    pub confidence: Option<f64>,
}

/// One persisted L2 row fed to `vision/cache-warm`. All fields optional except the
/// two that identify a description — a row missing `content_key`/`description` is
/// skipped (a corrupt row must not abort a bulk restore).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionWarmEntry.ts"
)]
pub struct VisionWarmEntry {
    #[serde(default)]
    #[ts(optional)]
    pub content_key: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub processing_time_ms: Option<u64>,
    #[serde(default)]
    #[ts(optional)]
    pub confidence: Option<f64>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub cached_at: Option<u64>,
}

/// Params for `vision/cache-warm` — bulk-restore L1 from persisted L2 rows.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionWarmParams.ts"
)]
pub struct VisionWarmParams {
    /// Persisted description rows to load into L1.
    pub entries: Vec<VisionWarmEntry>,
}

/// Params for `vision/cache-evict` — drop entries idle longer than `idle_ms`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionEvictParams.ts"
)]
pub struct VisionEvictParams {
    /// Evict entries not accessed within this many ms (default 1,800,000 = 30 min).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub idle_ms: Option<u64>,
}

/// Params for `vision/cache-stats` — no arguments.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionStatsParams.ts"
)]
pub struct VisionStatsParams {}

// ============================================================================
// Command results — typed output contracts (named TS types, never inline `any`).
// ============================================================================

/// Result of `vision/description-get`. `found=false` → all other fields absent.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionGetResult.ts"
)]
pub struct VisionGetResult {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub processing_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub confidence: Option<f64>,
}

/// Result of `vision/description-put`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionPutResult.ts"
)]
pub struct VisionPutResult {
    pub stored: bool,
}

/// Result of `vision/description-status`: `cached` or `none`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionStatusResult.ts"
)]
pub struct VisionStatusResult {
    pub status: String,
}

/// Result of `vision/cache-stats` — cache diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionCacheStatsResult.ts"
)]
pub struct VisionCacheStatsResult {
    #[ts(type = "number")]
    pub entries: usize,
    #[ts(type = "number")]
    pub max_entries: usize,
    #[ts(type = "number")]
    pub hits: u64,
    #[ts(type = "number")]
    pub misses: u64,
    pub hit_rate: f64,
    #[ts(type = "number")]
    pub evictions: u64,
}

/// Result of `vision/cache-warm`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionWarmResult.ts"
)]
pub struct VisionWarmResult {
    #[ts(type = "number")]
    pub warmed: u64,
    #[ts(type = "number")]
    pub total: usize,
}

/// Result of `vision/cache-evict`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vision/VisionEvictResult.ts"
)]
pub struct VisionEvictResult {
    #[ts(type = "number")]
    pub evicted: usize,
    #[ts(type = "number")]
    pub remaining: usize,
}

// ============================================================================
// VisionCache — the content-addressed cache the typed commands capture.
// ============================================================================

/// Max L1 cache entries. Each is ~1KB (description text + metadata).
const MAX_CACHE_ENTRIES: usize = 2000;

/// Owns the L1 content-addressed cache + hit/miss/eviction counters + the event bus.
/// The `vision/*` commands hold an `Arc<VisionCache>` and are thin wrappers over these
/// methods. Typed params enforce required keys at the boundary, so the methods are
/// infallible (they return their typed result, never `Result`).
pub struct VisionCache {
    /// Content-addressed cache: content_key → description
    cache: RwLock<HashMap<String, CachedDescription>>,
    /// Cache hit/miss counters
    hits: RwLock<u64>,
    misses: RwLock<u64>,
    evictions: RwLock<u64>,
    /// Event bus for publishing vision:description:ready
    bus: RwLock<Option<Arc<MessageBus>>>,
}

impl Default for VisionCache {
    fn default() -> Self {
        Self {
            cache: RwLock::new(HashMap::with_capacity(256)),
            hits: RwLock::new(0),
            misses: RwLock::new(0),
            evictions: RwLock::new(0),
            bus: RwLock::new(None),
        }
    }
}

impl VisionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wire the event bus in (called from `VisionModule::initialize`).
    fn set_bus(&self, bus: Arc<MessageBus>) {
        *self.bus.write().unwrap_or_else(|e| e.into_inner()) = Some(bus);
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Get a description by content key. Bumps last_accessed_at on hit.
    pub fn get(&self, content_key: &str) -> VisionGetResult {
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());

        if let Some(entry) = cache.get_mut(content_key) {
            entry.last_accessed_at = Self::now_ms();
            let result = VisionGetResult {
                found: true,
                description: Some(entry.description.clone()),
                model: Some(entry.model.clone()),
                provider: Some(entry.provider.clone()),
                processing_time_ms: Some(entry.processing_time_ms),
                confidence: Some(entry.confidence),
            };
            *self.hits.write().unwrap_or_else(|e| e.into_inner()) += 1;
            result
        } else {
            *self.misses.write().unwrap_or_else(|e| e.into_inner()) += 1;
            VisionGetResult {
                found: false,
                description: None,
                model: None,
                provider: None,
                processing_time_ms: None,
                confidence: None,
            }
        }
    }

    /// Store a description. Publishes vision:description:ready event.
    pub fn put(&self, p: &VisionPutParams) -> VisionPutResult {
        let model = p.model.clone().unwrap_or_else(|| "unknown".to_string());
        let provider = p.provider.clone().unwrap_or_else(|| "unknown".to_string());
        let processing_time_ms = p.processing_time_ms.unwrap_or(0);
        let confidence = p.confidence.unwrap_or(0.85);

        let now = Self::now_ms();
        let entry = CachedDescription {
            description: p.description.clone(),
            model: model.clone(),
            provider: provider.clone(),
            processing_time_ms,
            confidence,
            cached_at: now,
            last_accessed_at: now,
        };

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());

        // LRU eviction if at capacity
        if cache.len() >= MAX_CACHE_ENTRIES && !cache.contains_key(&p.content_key) {
            self.evict_lru(&mut cache);
        }

        cache.insert(p.content_key.clone(), entry);

        // Publish event — any TS consumer watching for this key gets notified
        let bus_guard = self.bus.read().unwrap_or_else(|e| e.into_inner());
        if let Some(bus) = bus_guard.as_ref() {
            bus.publish_async_only(
                "vision:description:ready",
                json!({
                    "content_key": p.content_key,
                    "description": p.description,
                    "model": model,
                    "provider": provider,
                }),
            );
        }

        VisionPutResult { stored: true }
    }

    /// Check status of a content key: "cached", "none"
    pub fn status(&self, content_key: &str) -> VisionStatusResult {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let status = if cache.contains_key(content_key) {
            "cached"
        } else {
            "none"
        };
        VisionStatusResult {
            status: status.to_string(),
        }
    }

    /// Cache statistics
    pub fn stats(&self) -> VisionCacheStatsResult {
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

        VisionCacheStatsResult {
            entries: cache.len(),
            max_entries: MAX_CACHE_ENTRIES,
            hits,
            misses,
            hit_rate,
            evictions,
        }
    }

    /// Bulk warm cache from persisted L2 data.
    /// Called by TS on startup to restore descriptions from ORM.
    pub fn warm(&self, entries: &[VisionWarmEntry]) -> VisionWarmResult {
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        let now = Self::now_ms();
        let mut loaded = 0u64;

        for entry in entries {
            // Skip rows missing the two identifying fields — a corrupt row must not
            // abort the bulk restore.
            let (content_key, description) = match (&entry.content_key, &entry.description) {
                (Some(k), Some(d)) => (k.clone(), d.clone()),
                _ => continue,
            };

            if cache.len() >= MAX_CACHE_ENTRIES {
                break; // Don't overflow during warm
            }

            cache.insert(
                content_key,
                CachedDescription {
                    description,
                    model: entry.model.clone().unwrap_or_else(|| "unknown".to_string()),
                    provider: entry
                        .provider
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                    processing_time_ms: entry.processing_time_ms.unwrap_or(0),
                    confidence: entry.confidence.unwrap_or(0.85),
                    cached_at: entry.cached_at.unwrap_or(now),
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

        VisionWarmResult {
            warmed: loaded,
            total: cache.len(),
        }
    }

    /// Manual eviction — remove entries not accessed within idle_ms.
    pub fn evict(&self, idle_ms: u64) -> VisionEvictResult {
        let cutoff = Self::now_ms().saturating_sub(idle_ms);

        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        let before = cache.len();

        cache.retain(|_, entry| entry.last_accessed_at >= cutoff);

        let evicted = before - cache.len();
        if evicted > 0 {
            *self.evictions.write().unwrap_or_else(|e| e.into_inner()) += evicted as u64;
        }

        log_info!(
            "vision",
            "cache-evict",
            "Evicted {} idle entries (cutoff={}ms, remaining={})",
            evicted,
            idle_ms,
            cache.len()
        );

        VisionEvictResult {
            evicted,
            remaining: cache.len(),
        }
    }

    /// LRU eviction: remove the least recently accessed entry.
    fn evict_lru(&self, cache: &mut HashMap<String, CachedDescription>) {
        let oldest_key = cache
            .iter()
            .min_by_key(|(_, v)| v.last_accessed_at)
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest_key {
            cache.remove(&key);
            *self.evictions.write().unwrap_or_else(|e| e.into_inner()) += 1;
        }
    }
}

// ============================================================================
// Module shell
// ============================================================================

/// Thin `ServiceModule` shell owning the [`VisionCache`] the typed `vision/*`
/// commands capture. Wires the event bus into the cache on init.
pub struct VisionModule {
    cache: Arc<VisionCache>,
}

impl Default for VisionModule {
    fn default() -> Self {
        Self {
            cache: Arc::new(VisionCache::new()),
        }
    }
}

impl VisionModule {
    pub fn new() -> Self {
        Self::default()
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
        self.cache.set_bus(ctx.bus.clone());
        log_info!(
            "vision",
            "init",
            "VisionModule initialized (max_entries={})",
            MAX_CACHE_ENTRIES
        );
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Err(format!(
            "vision command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::vision::command_objects(self.cache.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: put then get round-trips a description through the cache
    // (the content-addressed store + hit path) via the typed service.
    #[test]
    fn put_and_get_round_trip() {
        let cache = VisionCache::new();
        cache.put(&VisionPutParams {
            content_key: "abc123".to_string(),
            description: "A cat sitting on a keyboard".to_string(),
            model: Some("llava:7b".to_string()),
            provider: Some("candle".to_string()),
            processing_time_ms: Some(65000),
            confidence: Some(0.85),
        });

        let got = cache.get("abc123");
        assert!(got.found);
        assert_eq!(
            got.description.as_deref(),
            Some("A cat sitting on a keyboard")
        );
        assert_eq!(got.model.as_deref(), Some("llava:7b"));
    }

    // what this catches: a miss returns found=false and increments the miss counter.
    #[test]
    fn miss_returns_not_found() {
        let cache = VisionCache::new();
        let got = cache.get("nonexistent");
        assert!(!got.found);
        assert!(got.description.is_none());
    }

    // what this catches: stats reflect one entry, one hit, one miss → 0.5 hit rate.
    #[test]
    fn stats_track_hits_and_misses() {
        let cache = VisionCache::new();
        cache.put(&VisionPutParams {
            content_key: "key1".to_string(),
            description: "test".to_string(),
            model: None,
            provider: None,
            processing_time_ms: None,
            confidence: None,
        });
        cache.get("key1"); // hit
        cache.get("key2"); // miss

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    // what this catches: bulk warm loads multiple L2 rows into L1 and they become
    // gettable; malformed rows (missing keys) are skipped, not fatal.
    #[test]
    fn warm_loads_entries() {
        let cache = VisionCache::new();
        let entries = vec![
            VisionWarmEntry {
                content_key: Some("a".to_string()),
                description: Some("image a".to_string()),
                model: Some("llava".to_string()),
                provider: Some("candle".to_string()),
                processing_time_ms: None,
                confidence: None,
                cached_at: None,
            },
            VisionWarmEntry {
                content_key: Some("b".to_string()),
                description: Some("image b".to_string()),
                model: Some("llava".to_string()),
                provider: Some("candle".to_string()),
                processing_time_ms: None,
                confidence: None,
                cached_at: None,
            },
            // Malformed row: missing description → skipped, not fatal.
            VisionWarmEntry {
                content_key: Some("c".to_string()),
                description: None,
                model: None,
                provider: None,
                processing_time_ms: None,
                confidence: None,
                cached_at: None,
            },
        ];
        let res = cache.warm(&entries);
        assert_eq!(res.warmed, 2);
        assert_eq!(res.total, 2);

        let got = cache.get("b");
        assert!(got.found);
        assert_eq!(got.description.as_deref(), Some("image b"));
    }

    // what this catches: idle-based eviction drops the old entry and keeps the fresh
    // one (the LRU/idle reclaim path).
    #[test]
    fn eviction_drops_idle_entries() {
        let cache = VisionCache::new();
        {
            let mut c = cache.cache.write().unwrap();
            c.insert(
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
            c.insert(
                "new_key".to_string(),
                CachedDescription {
                    description: "new".to_string(),
                    model: "test".to_string(),
                    provider: "test".to_string(),
                    processing_time_ms: 0,
                    confidence: 0.5,
                    cached_at: VisionCache::now_ms(),
                    last_accessed_at: VisionCache::now_ms(),
                },
            );
        }

        let res = cache.evict(1000);
        assert_eq!(res.evicted, 1);
        assert_eq!(res.remaining, 1);

        assert!(!cache.get("old_key").found);
        assert!(cache.get("new_key").found);
    }

    // what this catches: the legacy handle_command surface fails loud (the typed
    // registry owns vision/* now) rather than silently dispatching.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = VisionModule::new();
        let err = module
            .handle_command("vision/cache-stats", json!({}))
            .await
            .unwrap_err();
        assert!(err.contains("migrated to the typed registry"), "got: {err}");
    }
}
