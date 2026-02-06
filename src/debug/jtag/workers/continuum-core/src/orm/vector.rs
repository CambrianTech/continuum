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
#[ts(export, export_to = "../../../shared/generated/orm/EmbeddingModel.ts")]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModel {
    pub name: String,
    pub dimensions: usize,
    pub provider: EmbeddingProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

/// Embedding provider
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/EmbeddingProvider.ts")]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    Fastembed,
    Ollama,
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/SimilarityMetric.ts")]
#[serde(rename_all = "lowercase")]
pub enum SimilarityMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl Default for SimilarityMetric {
    fn default() -> Self {
        SimilarityMetric::Cosine
    }
}

/// Hybrid search mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/HybridSearchMode.ts")]
#[serde(rename_all = "lowercase")]
pub enum HybridSearchMode {
    Semantic,
    Keyword,
    Hybrid,
}

impl Default for HybridSearchMode {
    fn default() -> Self {
        HybridSearchMode::Semantic
    }
}

/// Vector search query options
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorSearchOptions.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchOptions {
    pub collection: String,

    /// Query can be text (will generate embedding) OR pre-computed vector
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, unknown> | undefined")]
    pub filter: Option<Value>,

    /// Model selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<EmbeddingModel>,

    /// Pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[ts(export, export_to = "../../../shared/generated/orm/VectorSearchResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResult {
    pub id: UUID,
    #[ts(type = "Record<string, unknown>")]
    pub data: Value,
    /// Similarity score 0-1 (1 = identical)
    pub score: f32,
    /// Vector distance (lower = more similar)
    pub distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<VectorResultMetadata>,
}

/// Metadata for vector search result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorResultMetadata.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorResultMetadata {
    pub collection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_time: Option<u64>,
}

/// Full vector search response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorSearchResponse.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchResponse {
    pub results: Vec<VectorSearchResult>,
    pub total_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "Array<number> | undefined")]
    pub query_vector: Option<VectorEmbedding>,
    pub metadata: VectorResponseMetadata,
}

/// Metadata for vector search response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorResponseMetadata.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorResponseMetadata {
    pub collection: String,
    pub search_mode: HybridSearchMode,
    pub embedding_model: String,
    pub query_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,
}

/// Embedding generation request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/GenerateEmbeddingRequest.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenerateEmbeddingRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<EmbeddingModel>,
}

/// Embedding generation response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/GenerateEmbeddingResponse.ts")]
#[serde(rename_all = "camelCase")]
pub struct GenerateEmbeddingResponse {
    #[ts(type = "Array<number>")]
    pub embedding: VectorEmbedding,
    pub model: EmbeddingModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_time: Option<u64>,
}

/// Index vector request - store embedding for a record
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/IndexVectorRequest.ts")]
#[serde(rename_all = "camelCase")]
pub struct IndexVectorRequest {
    pub collection: String,
    pub id: UUID,
    #[ts(type = "Array<number>")]
    pub embedding: VectorEmbedding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IndexVectorMetadata>,
}

/// Metadata for index vector request
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/IndexVectorMetadata.ts")]
#[serde(rename_all = "camelCase")]
pub struct IndexVectorMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
}

/// Backfill vectors request - generate embeddings for existing records
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/BackfillVectorsRequest.ts")]
#[serde(rename_all = "camelCase")]
pub struct BackfillVectorsRequest {
    pub collection: String,
    /// Field to generate embeddings from (e.g., 'content')
    pub text_field: String,
    /// Only backfill matching records
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, unknown> | undefined")]
    pub filter: Option<Value>,
    /// Process N records at a time (default: 100)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<EmbeddingModel>,
}

fn default_batch_size() -> usize {
    100
}

/// Backfill vectors progress
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/BackfillVectorsProgress.ts")]
#[serde(rename_all = "camelCase")]
pub struct BackfillVectorsProgress {
    pub total: usize,
    pub processed: usize,
    pub failed: usize,
    pub elapsed_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_remaining: Option<u64>,
}

/// Vector index statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorIndexStats.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorIndexStats {
    pub collection: String,
    pub total_records: usize,
    pub records_with_vectors: usize,
    pub vector_dimensions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

/// Vector search capabilities
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/VectorSearchCapabilities.ts")]
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

/// Similarity metric implementations
pub mod similarity {
    use super::VectorEmbedding;

    /// Cosine similarity: measures angle between vectors (0-1, 1 = identical)
    pub fn cosine(a: &VectorEmbedding, b: &VectorEmbedding) -> f32 {
        assert_eq!(
            a.len(),
            b.len(),
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        );

        let len = a.len();
        let mut dot_product = 0.0f32;
        let mut norm_a = 0.0f32;
        let mut norm_b = 0.0f32;

        // Loop unrolling for SIMD-like performance
        let limit = len - (len % 4);
        let mut i = 0;

        while i < limit {
            let a0 = a[i];
            let a1 = a[i + 1];
            let a2 = a[i + 2];
            let a3 = a[i + 3];
            let b0 = b[i];
            let b1 = b[i + 1];
            let b2 = b[i + 2];
            let b3 = b[i + 3];

            dot_product += a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
            norm_a += a0 * a0 + a1 * a1 + a2 * a2 + a3 * a3;
            norm_b += b0 * b0 + b1 * b1 + b2 * b2 + b3 * b3;
            i += 4;
        }

        // Handle remaining elements
        while i < len {
            dot_product += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
            i += 1;
        }

        let denominator = norm_a.sqrt() * norm_b.sqrt();
        if denominator == 0.0 {
            0.0
        } else {
            dot_product / denominator
        }
    }

    /// Euclidean distance: straight-line distance (lower = more similar)
    pub fn euclidean(a: &VectorEmbedding, b: &VectorEmbedding) -> f32 {
        assert_eq!(
            a.len(),
            b.len(),
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        );

        let sum: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum();
        sum.sqrt()
    }

    /// Dot product: magnitude * alignment (higher = more similar)
    pub fn dot_product(a: &VectorEmbedding, b: &VectorEmbedding) -> f32 {
        assert_eq!(
            a.len(),
            b.len(),
            "Vector dimensions must match: {} vs {}",
            a.len(),
            b.len()
        );

        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    /// Convert distance to similarity score (0-1)
    pub fn distance_to_score(distance: f32, metric: super::SimilarityMetric) -> f32 {
        match metric {
            super::SimilarityMetric::Cosine => (1.0 + distance) / 2.0, // cosine is already -1 to 1
            super::SimilarityMetric::Euclidean => 1.0 / (1.0 + distance), // larger distance = lower score
            super::SimilarityMetric::DotProduct => distance.max(0.0).min(1.0), // clamp to 0-1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((similarity::cosine(&a, &b) - 1.0).abs() < 0.0001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((similarity::cosine(&a, &c) - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        assert!((similarity::euclidean(&a, &b) - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_dot_product() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert!((similarity::dot_product(&a, &b) - 32.0).abs() < 0.0001);
    }
}
