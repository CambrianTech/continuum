/// RagModule — Batched RAG context composition with parallel source loading.
///
/// Handles: rag/compose
///
/// Key optimization: Instead of TypeScript making N IPC calls (one per source),
/// this module receives ALL source requests in ONE call and runs them in parallel
/// using Rayon. This eliminates IPC round-trip overhead and leverages Rust's
/// superior parallel execution.
///
/// Dynamic sources are supported via RagSourceRequest which specifies:
/// - source_type: "memory" | "scene" | "widget" | "project" | "custom"
/// - params: Source-specific parameters (JSON)
///
/// This allows video games to pass scene/move context, VR apps to pass spatial
/// data, chat to pass conversation history - all in the same batched call.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::memory::PersonaMemoryManager;
use crate::logging::TimingGuard;
use crate::log_info;
use async_trait::async_trait;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::Instant;
use ts_rs::TS;

// ═══════════════════════════════════════════════════════════════════════════
// TYPES — Generated to TypeScript via ts-rs as discriminated unions
// ═══════════════════════════════════════════════════════════════════════════

// ─── Source-Specific Params (Strongly Typed) ─────────────────────────────────

/// Memory source params — for semantic memory recall
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/MemorySourceParams.ts")]
pub struct MemorySourceParams {
    /// Query text for semantic search
    #[ts(optional)]
    pub query_text: Option<String>,
    /// Specific layers to search (empty = all)
    #[ts(optional)]
    pub layers: Option<Vec<String>>,
}

/// Consciousness source params — temporal + cross-context awareness
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/ConsciousnessSourceParams.ts")]
pub struct ConsciousnessSourceParams {
    /// Current message for context
    #[ts(optional)]
    pub current_message: Option<String>,
    /// Skip semantic search (faster)
    #[serde(default)]
    pub skip_semantic_search: bool,
}

/// Scene source params — for video games, VR, 3D apps
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/SceneSourceParams.ts")]
pub struct SceneSourceParams {
    /// Scene/level identifier
    pub scene_id: String,
    /// Pre-computed scene description
    pub description: String,
    /// Visible objects in scene
    #[ts(optional)]
    pub objects: Option<Vec<String>>,
    /// Active characters/NPCs
    #[ts(optional)]
    pub characters: Option<Vec<String>>,
}

/// Project source params — for code/workspace context
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/ProjectSourceParams.ts")]
pub struct ProjectSourceParams {
    /// Project root path
    pub project_path: String,
    /// Pre-computed project summary
    #[ts(optional)]
    pub summary: Option<String>,
    /// Recent git changes
    #[ts(optional)]
    pub recent_changes: Option<Vec<String>>,
}

/// Custom section for passthrough content
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/CustomSection.ts")]
pub struct CustomSection {
    /// Section type label
    pub section_type: String,
    /// The content
    pub content: String,
    /// Relevance score 0.0-1.0
    #[ts(optional)]
    pub relevance: Option<f64>,
    /// Source reference for attribution
    #[ts(optional)]
    pub source_ref: Option<String>,
}

/// Custom source params — passthrough for extensions (only place with flexibility)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/CustomSourceParams.ts")]
pub struct CustomSourceParams {
    /// Pre-computed sections to pass through
    pub sections: Vec<CustomSection>,
}

// ─── Tagged Union: RagSourceRequest ──────────────────────────────────────────

/// RAG source request — discriminated union by source_type
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "source_type")]
#[ts(export, export_to = "../../../shared/generated/rag/RagSourceRequest.ts")]
pub enum RagSourceRequest {
    /// Memory recall source
    #[serde(rename = "memory")]
    Memory {
        budget_tokens: usize,
        params: MemorySourceParams,
    },
    /// Consciousness/temporal context source
    #[serde(rename = "consciousness")]
    Consciousness {
        budget_tokens: usize,
        params: ConsciousnessSourceParams,
    },
    /// Scene context (games, VR)
    #[serde(rename = "scene")]
    Scene {
        budget_tokens: usize,
        params: SceneSourceParams,
    },
    /// Project/code context
    #[serde(rename = "project")]
    Project {
        budget_tokens: usize,
        params: ProjectSourceParams,
    },
    /// Custom passthrough
    #[serde(rename = "custom")]
    Custom {
        budget_tokens: usize,
        params: CustomSourceParams,
    },
}

impl RagSourceRequest {
    pub fn budget_tokens(&self) -> usize {
        match self {
            Self::Memory { budget_tokens, .. } => *budget_tokens,
            Self::Consciousness { budget_tokens, .. } => *budget_tokens,
            Self::Scene { budget_tokens, .. } => *budget_tokens,
            Self::Project { budget_tokens, .. } => *budget_tokens,
            Self::Custom { budget_tokens, .. } => *budget_tokens,
        }
    }

    pub fn source_type(&self) -> &'static str {
        match self {
            Self::Memory { .. } => "memory",
            Self::Consciousness { .. } => "consciousness",
            Self::Scene { .. } => "scene",
            Self::Project { .. } => "project",
            Self::Custom { .. } => "custom",
        }
    }
}

// ─── Source-Specific Metadata (Strongly Typed) ───────────────────────────────

/// Memory source metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/MemorySourceMetadata.ts")]
pub struct MemorySourceMetadata {
    pub memory_count: usize,
    pub total_candidates: usize,
    pub layers: String,
    pub recall_time_ms: f64,
}

/// Temporal info for consciousness
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/ConsciousnessTemporalInfo.ts")]
pub struct ConsciousnessTemporalInfo {
    #[ts(optional)]
    pub last_active_context: Option<String>,
    pub time_away_ms: i64,
    pub was_interrupted: bool,
}

/// Consciousness source metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/ConsciousnessSourceMetadata.ts")]
pub struct ConsciousnessSourceMetadata {
    pub cross_context_count: usize,
    pub intention_count: usize,
    pub has_peripheral_activity: bool,
    pub temporal: ConsciousnessTemporalInfo,
    pub build_time_ms: f64,
}

/// Empty metadata for simple sources
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/EmptyMetadata.ts")]
pub struct EmptyMetadata {}

// ─── Tagged Union: RagSourceMetadata ─────────────────────────────────────────

/// RAG source metadata — discriminated union by source_type
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "source_type")]
#[ts(export, export_to = "../../../shared/generated/rag/RagSourceMetadata.ts")]
pub enum RagSourceMetadata {
    #[serde(rename = "memory")]
    Memory(MemorySourceMetadata),
    #[serde(rename = "consciousness")]
    Consciousness(ConsciousnessSourceMetadata),
    #[serde(rename = "scene")]
    Scene(EmptyMetadata),
    #[serde(rename = "project")]
    Project(EmptyMetadata),
    #[serde(rename = "custom")]
    Custom(EmptyMetadata),
}

// ─── Result Types ────────────────────────────────────────────────────────────

/// Result from a single RAG source.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagSourceResult.ts")]
pub struct RagSourceResult {
    /// Which source this result came from
    pub source_type: String,

    /// Content sections (memories, context snippets, etc.)
    pub sections: Vec<RagSection>,

    /// Actual tokens used
    pub tokens_used: usize,

    /// Load time in milliseconds
    pub load_time_ms: f64,

    /// Whether this source succeeded
    pub success: bool,

    /// Error message if failed
    #[ts(optional)]
    pub error: Option<String>,

    /// Source-specific metadata (typed by source_type)
    pub metadata: RagSourceMetadata,
}

/// A section of RAG content.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagSection.ts")]
pub struct RagSection {
    /// Section type: "memory", "context", "instruction", "fact", etc.
    pub section_type: String,

    /// The actual content
    pub content: String,

    /// Relevance score (0.0 - 1.0)
    #[ts(optional)]
    pub relevance: Option<f64>,

    /// Source reference (for attribution)
    #[ts(optional)]
    pub source_ref: Option<String>,
}

/// Full RAG compose request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagComposeRequest {
    /// Persona ID for memory/persona-specific sources
    pub persona_id: String,

    /// Room/context ID
    pub room_id: String,

    /// Current message/query for semantic search
    #[serde(default)]
    pub query_text: Option<String>,

    /// All sources to load in parallel
    pub sources: Vec<RagSourceRequest>,

    /// Total token budget across all sources
    pub total_budget: usize,
}

/// Full RAG compose result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagComposeResult.ts")]
pub struct RagComposeResult {
    /// Results from each source (parallel-loaded)
    pub source_results: Vec<RagSourceResult>,

    /// Total tokens across all sources
    pub total_tokens: usize,

    /// Total compose time in milliseconds
    pub compose_time_ms: f64,

    /// Number of sources that succeeded
    pub sources_succeeded: usize,

    /// Number of sources that failed
    pub sources_failed: usize,
}

// ═══════════════════════════════════════════════════════════════════════════
// MODULE STATE
// ═══════════════════════════════════════════════════════════════════════════

/// Shared state for RAG module.
pub struct RagState {
    /// Memory manager for memory-based sources
    pub memory_manager: Arc<PersonaMemoryManager>,
}

impl RagState {
    pub fn new(memory_manager: Arc<PersonaMemoryManager>) -> Self {
        Self { memory_manager }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SOURCE LOADERS — Each source type has a dedicated loader (OOP pattern)
// ═══════════════════════════════════════════════════════════════════════════

impl RagState {
    /// Load memory source using multi-layer recall.
    fn load_memory_source(
        &self,
        persona_id: &str,
        room_id: &str,
        query_text: Option<&str>,
        params: &MemorySourceParams,
        budget_tokens: usize,
    ) -> RagSourceResult {
        let start = Instant::now();

        // Estimate max memories from budget (assuming ~80 tokens per memory)
        let max_results = (budget_tokens / 80).max(3);

        let req = crate::memory::MultiLayerRecallRequest {
            query_text: params.query_text.clone().or_else(|| query_text.map(|s| s.to_string())),
            room_id: room_id.to_string(),
            max_results,
            layers: params.layers.clone(),
        };

        match self.memory_manager.multi_layer_recall(persona_id, &req) {
            Ok(resp) => {
                let sections: Vec<RagSection> = resp.memories.iter().map(|mem| {
                    RagSection {
                        section_type: "memory".to_string(),
                        content: mem.content.clone(),
                        relevance: mem.relevance_score,
                        source_ref: Some(format!("memory:{}", mem.id)),
                    }
                }).collect();

                let tokens_used = sections.iter()
                    .map(|s| s.content.len() / 4)
                    .sum();

                let layers_str = resp.layer_timings.iter()
                    .map(|l| format!("{}({})", l.layer, l.results_found))
                    .collect::<Vec<_>>()
                    .join(", ");

                RagSourceResult {
                    source_type: "memory".to_string(),
                    sections,
                    tokens_used,
                    load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                    success: true,
                    error: None,
                    metadata: RagSourceMetadata::Memory(MemorySourceMetadata {
                        memory_count: resp.memories.len(),
                        total_candidates: resp.total_candidates,
                        layers: layers_str,
                        recall_time_ms: resp.recall_time_ms,
                    }),
                }
            }
            Err(e) => RagSourceResult {
                source_type: "memory".to_string(),
                sections: vec![],
                tokens_used: 0,
                load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                success: false,
                error: Some(e.to_string()),
                metadata: RagSourceMetadata::Memory(MemorySourceMetadata {
                    memory_count: 0,
                    total_candidates: 0,
                    layers: String::new(),
                    recall_time_ms: 0.0,
                }),
            },
        }
    }

    /// Load consciousness context (cross-context awareness, intentions, etc.)
    fn load_consciousness_source(
        &self,
        persona_id: &str,
        room_id: &str,
        query_text: Option<&str>,
        params: &ConsciousnessSourceParams,
    ) -> RagSourceResult {
        let start = Instant::now();

        let req = crate::memory::ConsciousnessContextRequest {
            room_id: room_id.to_string(),
            current_message: params.current_message.clone().or_else(|| query_text.map(|s| s.to_string())),
            skip_semantic_search: params.skip_semantic_search,
        };

        match self.memory_manager.consciousness_context(persona_id, &req) {
            Ok(resp) => {
                let sections = if let Some(prompt) = resp.formatted_prompt {
                    vec![RagSection {
                        section_type: "consciousness".to_string(),
                        content: prompt,
                        relevance: Some(1.0),
                        source_ref: Some("consciousness:context".to_string()),
                    }]
                } else {
                    vec![]
                };

                let tokens_used = sections.iter()
                    .map(|s| s.content.len() / 4)
                    .sum();

                RagSourceResult {
                    source_type: "consciousness".to_string(),
                    sections,
                    tokens_used,
                    load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                    success: true,
                    error: None,
                    metadata: RagSourceMetadata::Consciousness(ConsciousnessSourceMetadata {
                        cross_context_count: resp.cross_context_event_count,
                        intention_count: resp.active_intention_count,
                        has_peripheral_activity: resp.has_peripheral_activity,
                        temporal: ConsciousnessTemporalInfo {
                            last_active_context: resp.temporal.last_active_context,
                            time_away_ms: resp.temporal.time_away_ms,
                            was_interrupted: resp.temporal.was_interrupted,
                        },
                        build_time_ms: resp.build_time_ms,
                    }),
                }
            }
            Err(e) => RagSourceResult {
                source_type: "consciousness".to_string(),
                sections: vec![],
                tokens_used: 0,
                load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
                success: false,
                error: Some(e.to_string()),
                metadata: RagSourceMetadata::Consciousness(ConsciousnessSourceMetadata {
                    cross_context_count: 0,
                    intention_count: 0,
                    has_peripheral_activity: false,
                    temporal: ConsciousnessTemporalInfo {
                        last_active_context: None,
                        time_away_ms: 0,
                        was_interrupted: false,
                    },
                    build_time_ms: 0.0,
                }),
            },
        }
    }

    /// Load scene source (video games, VR, 3D apps)
    fn load_scene_source(
        &self,
        params: &SceneSourceParams,
        budget_tokens: usize,
    ) -> RagSourceResult {
        let start = Instant::now();

        // Scene params are pre-computed by TypeScript - we just convert to sections
        let mut content = params.description.clone();

        if let Some(objects) = &params.objects {
            if !objects.is_empty() {
                content.push_str("\n\nVisible objects: ");
                content.push_str(&objects.join(", "));
            }
        }

        if let Some(characters) = &params.characters {
            if !characters.is_empty() {
                content.push_str("\n\nCharacters present: ");
                content.push_str(&characters.join(", "));
            }
        }

        let sections = vec![RagSection {
            section_type: "scene".to_string(),
            content,
            relevance: Some(1.0),
            source_ref: Some(format!("scene:{}", params.scene_id)),
        }];

        let tokens_used = sections.iter()
            .map(|s| s.content.len() / 4)
            .sum::<usize>()
            .min(budget_tokens);

        RagSourceResult {
            source_type: "scene".to_string(),
            sections,
            tokens_used,
            load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            success: true,
            error: None,
            metadata: RagSourceMetadata::Scene(EmptyMetadata {}),
        }
    }

    /// Load project source (code/workspace context)
    fn load_project_source(
        &self,
        params: &ProjectSourceParams,
        budget_tokens: usize,
    ) -> RagSourceResult {
        let start = Instant::now();

        let mut content = format!("Project: {}", params.project_path);

        if let Some(summary) = &params.summary {
            content.push_str("\n\n");
            content.push_str(summary);
        }

        if let Some(changes) = &params.recent_changes {
            if !changes.is_empty() {
                content.push_str("\n\nRecent changes:\n");
                for change in changes {
                    content.push_str("- ");
                    content.push_str(change);
                    content.push('\n');
                }
            }
        }

        let sections = vec![RagSection {
            section_type: "project".to_string(),
            content,
            relevance: Some(0.8),
            source_ref: Some(format!("project:{}", params.project_path)),
        }];

        let tokens_used = sections.iter()
            .map(|s| s.content.len() / 4)
            .sum::<usize>()
            .min(budget_tokens);

        RagSourceResult {
            source_type: "project".to_string(),
            sections,
            tokens_used,
            load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            success: true,
            error: None,
            metadata: RagSourceMetadata::Project(EmptyMetadata {}),
        }
    }

    /// Load custom source (passthrough for extensions)
    fn load_custom_source(
        &self,
        params: &CustomSourceParams,
        budget_tokens: usize,
    ) -> RagSourceResult {
        let start = Instant::now();

        // Convert CustomSection to RagSection
        let sections: Vec<RagSection> = params.sections.iter().map(|s| {
            RagSection {
                section_type: s.section_type.clone(),
                content: s.content.clone(),
                relevance: s.relevance,
                source_ref: s.source_ref.clone(),
            }
        }).collect();

        let tokens_used = sections.iter()
            .map(|s| s.content.len() / 4)
            .sum::<usize>()
            .min(budget_tokens);

        RagSourceResult {
            source_type: "custom".to_string(),
            sections,
            tokens_used,
            load_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            success: true,
            error: None,
            metadata: RagSourceMetadata::Custom(EmptyMetadata {}),
        }
    }

    /// Load a single source based on its discriminated type (OOP pattern matching)
    fn load_source(
        &self,
        source: &RagSourceRequest,
        persona_id: &str,
        room_id: &str,
        query_text: Option<&str>,
    ) -> RagSourceResult {
        match source {
            RagSourceRequest::Memory { budget_tokens, params } => {
                self.load_memory_source(persona_id, room_id, query_text, params, *budget_tokens)
            }
            RagSourceRequest::Consciousness { budget_tokens: _, params } => {
                self.load_consciousness_source(persona_id, room_id, query_text, params)
            }
            RagSourceRequest::Scene { budget_tokens, params } => {
                self.load_scene_source(params, *budget_tokens)
            }
            RagSourceRequest::Project { budget_tokens, params } => {
                self.load_project_source(params, *budget_tokens)
            }
            RagSourceRequest::Custom { budget_tokens, params } => {
                self.load_custom_source(params, *budget_tokens)
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RAG MODULE
// ═══════════════════════════════════════════════════════════════════════════

pub struct RagModule {
    state: Arc<RagState>,
}

impl RagModule {
    pub fn new(state: Arc<RagState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for RagModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "rag",
            priority: ModulePriority::Normal,
            command_prefixes: &["rag/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "rag/compose" => {
                let _timer = TimingGuard::new("module", "rag_compose");
                let start = Instant::now();

                // Parse request
                let req: RagComposeRequest = serde_json::from_value(params)
                    .map_err(|e| format!("Invalid rag/compose request: {e}"))?;

                let persona_id = req.persona_id.clone();
                let room_id = req.room_id.clone();
                let query_text = req.query_text.clone();
                let sources = req.sources.clone();

                // Clone state for parallel access
                let state = Arc::clone(&self.state);

                // ═══════════════════════════════════════════════════════════
                // PARALLEL SOURCE LOADING WITH RAYON
                // This is the key optimization - all sources run in parallel
                // ═══════════════════════════════════════════════════════════
                let source_results: Vec<RagSourceResult> = sources
                    .par_iter()
                    .map(|source| {
                        state.load_source(
                            source,
                            &persona_id,
                            &room_id,
                            query_text.as_deref(),
                        )
                    })
                    .collect();

                // Aggregate results
                let total_tokens: usize = source_results.iter().map(|r| r.tokens_used).sum();
                let sources_succeeded = source_results.iter().filter(|r| r.success).count();
                let sources_failed = source_results.len() - sources_succeeded;
                let compose_time_ms = start.elapsed().as_secs_f64() * 1000.0;

                log_info!(
                    "module", "rag_compose",
                    "RAG compose for {}: {} sources ({} ok, {} failed), {} tokens in {:.1}ms",
                    persona_id, sources.len(), sources_succeeded, sources_failed,
                    total_tokens, compose_time_ms
                );

                let result = RagComposeResult {
                    source_results,
                    total_tokens,
                    compose_time_ms,
                    sources_succeeded,
                    sources_failed,
                };

                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            _ => Err(format!("Unknown rag command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rag_section_serialization() {
        let section = RagSection {
            section_type: "memory".to_string(),
            content: "Test content".to_string(),
            relevance: Some(0.95),
            source_ref: Some("memory:123".to_string()),
        };

        let json = serde_json::to_string(&section).unwrap();
        assert!(json.contains("memory"));
        assert!(json.contains("Test content"));
    }

    #[test]
    fn test_custom_source_passthrough() {
        // Custom sources should pass through pre-computed content
        let params = serde_json::json!({
            "content": "Player is in the forest facing a dragon",
            "relevance": 1.0,
            "source_ref": "game:scene:42"
        });

        // Would test load_custom_source here with mock state
    }
}
