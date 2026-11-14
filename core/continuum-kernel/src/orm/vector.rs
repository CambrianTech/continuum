//! Vector Search Types and Adapter - Semantic Search for ORM
//!
//! Extends StorageAdapter with vector similarity search capabilities.
//! Uses fastembed for embedding generation (inline ONNX, ~5ms per embed).
//!
//! Key features:
//! - Cosine similarity search
//! - Hybrid search (semantic + keyword)
//! - Embedding generation via fastembed
//! - Vector indexing and backfilling

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::types::{StorageResult, UUID};

/// Vector embedding - array of f32 representing semantic meaning
pub type VectorEmbedding = Vec<f32>;

/// Embedding model configuration
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/EmbeddingModel.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModel {
    pub name: String,
    pub dimensions: usize,
    pub provider: EmbeddingProvider,
    pub max_tokens: Option<usize>,
}

/// Embedding provider
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/EmbeddingProvider.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    Fastembed,
    OpenAI,
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self {
            name: "all-minilm".to_string(),
            dimensions: 384,
            provider: EmbeddingProvider::Fastembed,
            max_tokens: Some(512),
        }
    }
}

/// Similarity metric for vector search
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/SimilarityMetric.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum SimilarityMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
}

/// Hybrid search mode
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/HybridSearchMode.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum HybridSearchMode {
    #[default]
    Semantic,
    Keyword,
    Hybrid,
}

/// Vector search query options
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorSearchOptions.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchOptions {
    pub collection: String,

    /// Query can be text (will generate embedding) OR pre-computed vector
    pub query_text: Option<String>,
    #[ts(type = "Array<number> | undefined")]
    pub query_vector: Option<VectorEmbedding>,

    /// Number of results (default: 10)
    #[serde(default = "default_k")]
    pub k: usize,

    /// Minimum similarity threshold 0-1 (default: 0.0)
    #[serde(default)]
    pub similarity_threshold: f32,

    /// Hybrid search mode
    #[serde(default)]
    pub hybrid_mode: HybridSearchMode,

    /// Weight of semantic vs keyword (0-1, default: 0.5)
    #[serde(default = "default_hybrid_ratio")]
    pub hybrid_ratio: f32,

    /// Metadata filters
    #[ts(type = "Record<string, unknown> | undefined")]
    pub filter: Option<Value>,

    /// Model selection
    pub embedding_model: Option<EmbeddingModel>,

    /// Pagination
    pub offset: Option<usize>,
    pub limit: Option<usize>,

    /// Similarity metric
    #[serde(default)]
    pub metric: SimilarityMetric,
}

fn default_k() -> usize {
    10
}

fn default_hybrid_ratio() -> f32 {
    0.5
}

impl Default for VectorSearchOptions {
    fn default() -> Self {
        Self {
            collection: String::new(),
            query_text: None,
            query_vector: None,
            k: 10,
            similarity_threshold: 0.0,
            hybrid_mode: HybridSearchMode::Semantic,
            hybrid_ratio: 0.5,
            filter: None,
            embedding_model: None,
            offset: None,
            limit: None,
            metric: SimilarityMetric::Cosine,
        }
    }
}

/// Vector search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorSearchResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResult {
    pub id: UUID,
    #[ts(type = "Record<string, unknown>")]
    pub data: Value,
    /// Similarity score 0-1 (1 = identical)
    pub score: f32,
    /// Vector distance (lower = more similar)
    pub distance: f32,
    pub metadata: Option<VectorResultMetadata>,
}

/// Metadata for vector search result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorResultMetadata.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorResultMetadata {
    pub collection: String,
    pub embedding_model: Option<String>,
    pub query_time: Option<u64>,
}

/// Full vector search response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorSearchResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResponse {
    pub results: Vec<VectorSearchResult>,
    pub total_results: usize,
    #[ts(type = "Array<number> | undefined")]
    pub query_vector: Option<VectorEmbedding>,
    pub metadata: VectorResponseMetadata,
}

/// Metadata for vector search response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorResponseMetadata.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorResponseMetadata {
    pub collection: String,
    pub search_mode: HybridSearchMode,
    pub embedding_model: String,
    pub query_time: u64,
    pub cache_hit: Option<bool>,
}

/// Embedding generation request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/GenerateEmbeddingRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateEmbeddingRequest {
    pub text: String,
    pub model: Option<EmbeddingModel>,
}

/// Embedding generation response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/GenerateEmbeddingResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateEmbeddingResponse {
    #[ts(type = "Array<number>")]
    pub embedding: VectorEmbedding,
    pub model: EmbeddingModel,
    pub token_count: Option<usize>,
    pub generation_time: Option<u64>,
}

/// Index vector request - store embedding for a record
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/IndexVectorRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct IndexVectorRequest {
    pub collection: String,
    pub id: UUID,
    #[ts(type = "Array<number>")]
    pub embedding: VectorEmbedding,
    pub metadata: Option<IndexVectorMetadata>,
}

/// Metadata for index vector request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/IndexVectorMetadata.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct IndexVectorMetadata {
    pub embedding_model: Option<String>,
    pub generated_at: Option<String>,
}

/// Backfill vectors request - generate embeddings for existing records
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/BackfillVectorsRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct BackfillVectorsRequest {
    pub collection: String,
    /// Field to generate embeddings from (e.g., 'content')
    pub text_field: String,
    /// Only backfill matching records
    #[ts(type = "Record<string, unknown> | undefined")]
    pub filter: Option<Value>,
    /// Process N records at a time (default: 100)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    pub model: Option<EmbeddingModel>,
}

fn default_batch_size() -> usize {
    100
}

/// Backfill vectors progress
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/BackfillVectorsProgress.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct BackfillVectorsProgress {
    pub total: usize,
    pub processed: usize,
    pub failed: usize,
    pub elapsed_time: u64,
    pub estimated_remaining: Option<u64>,
}

/// Vector index statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorIndexStats.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorIndexStats {
    pub collection: String,
    pub total_records: usize,
    pub records_with_vectors: usize,
    pub vector_dimensions: usize,
    pub embedding_model: Option<String>,
    pub index_size: Option<usize>,
    pub last_updated: Option<String>,
}

/// Vector search capabilities
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/VectorSearchCapabilities.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchCapabilities {
    pub supports_vector_search: bool,
    pub supports_hybrid_search: bool,
    pub supports_embedding_generation: bool,
    pub max_vector_dimensions: usize,
    pub supported_similarity_metrics: Vec<SimilarityMetric>,
    pub embedding_providers: Vec<EmbeddingProvider>,
}

/// Vector Search Adapter Trait
///
/// Adapters that support vector search implement this trait.
/// Uses fastembed for embedding generation.
#[async_trait]
pub trait VectorSearchAdapter: Send + Sync {
    /// Perform vector similarity search
    async fn vector_search(
        &self,
        options: VectorSearchOptions,
    ) -> StorageResult<VectorSearchResponse>;

    /// Generate embedding for text using fastembed
    async fn generate_embedding(
        &self,
        request: GenerateEmbeddingRequest,
    ) -> StorageResult<GenerateEmbeddingResponse>;

    /// Index vector for a record
    async fn index_vector(&self, request: IndexVectorRequest) -> StorageResult<bool>;

    /// Backfill embeddings for existing records
    async fn backfill_vectors(
        &self,
        request: BackfillVectorsRequest,
    ) -> StorageResult<BackfillVectorsProgress>;

    /// Get vector index statistics
    async fn get_vector_index_stats(&self, collection: &str) -> StorageResult<VectorIndexStats>;

    /// Get vector search capabilities
    fn get_vector_search_capabilities(&self) -> VectorSearchCapabilities;
}
