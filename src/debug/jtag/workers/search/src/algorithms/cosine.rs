/// Cosine Similarity Algorithm
///
/// Vector-based similarity for semantic search using pre-computed embeddings.
/// Optimized for memory recall where vectors are already available.
///
/// Parameters:
/// - normalize: Whether to L2-normalize vectors (default: true)
/// - threshold: Minimum similarity to include in results (default: 0.0)
use super::{SearchAlgorithm, SearchInput, SearchOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct CosineAlgorithm {
    /// L2-normalize vectors before comparison
    normalize: bool,
    /// Minimum similarity threshold
    threshold: f64,
}

/// Extended input for vector-based search (passed via params in main.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchInput {
    /// Query vector (embedding)
    pub query_vector: Vec<f64>,
    /// Corpus vectors (embeddings)
    pub corpus_vectors: Vec<Vec<f64>>,
}

impl CosineAlgorithm {
    /// Factory method (OpenCV create() pattern)
    pub fn create() -> Box<dyn SearchAlgorithm> {
        Box::new(Self::default())
    }

    /// Compute cosine similarity between two vectors
    /// Uses SIMD-friendly loop that compiler can auto-vectorize
    #[inline]
    pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;

        // SIMD-friendly loop - compiler will auto-vectorize this
        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }

        let denominator = (norm_a * norm_b).sqrt();
        if denominator == 0.0 {
            0.0
        } else {
            dot / denominator
        }
    }

    /// L2-normalize a vector in-place
    fn l2_normalize(v: &mut [f64]) {
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Search using pre-computed vectors (primary use case for semantic memory)
    pub fn vector_search(&self, input: &VectorSearchInput) -> SearchOutput {
        let mut query = input.query_vector.clone();

        // Normalize query if configured
        if self.normalize {
            Self::l2_normalize(&mut query);
        }

        // Compute similarities
        let mut scores: Vec<f64> = Vec::with_capacity(input.corpus_vectors.len());

        for corpus_vec in &input.corpus_vectors {
            let mut cv = corpus_vec.clone();
            if self.normalize {
                Self::l2_normalize(&mut cv);
            }
            let sim = Self::cosine_similarity(&query, &cv);
            scores.push(if sim >= self.threshold { sim } else { 0.0 });
        }

        // Create ranked indices (sorted by score descending)
        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let ranked_indices: Vec<usize> = ranked.into_iter().map(|(i, _)| i).collect();

        SearchOutput {
            scores,
            ranked_indices,
        }
    }
}

impl Default for CosineAlgorithm {
    fn default() -> Self {
        Self {
            normalize: true,
            threshold: 0.0,
        }
    }
}

impl SearchAlgorithm for CosineAlgorithm {
    fn name(&self) -> &'static str {
        "cosine"
    }

    /// Text-based search (fallback - uses Jaccard similarity on terms)
    /// Primary use is vector_search() called directly with vectors
    fn execute(&self, input: &SearchInput) -> SearchOutput {
        // For text input, use simple term overlap as approximation
        let query_terms: std::collections::HashSet<_> = input
            .query
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect();

        let scores: Vec<f64> = input
            .corpus
            .iter()
            .map(|doc| {
                let doc_terms: std::collections::HashSet<_> = doc
                    .to_lowercase()
                    .split_whitespace()
                    .map(String::from)
                    .collect();

                if query_terms.is_empty() || doc_terms.is_empty() {
                    return 0.0;
                }

                // Jaccard similarity as approximation
                let intersection = query_terms.intersection(&doc_terms).count() as f64;
                let union = query_terms.union(&doc_terms).count() as f64;

                if union > 0.0 {
                    intersection / union
                } else {
                    0.0
                }
            })
            .collect();

        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let ranked_indices: Vec<usize> = ranked.into_iter().map(|(i, _)| i).collect();

        SearchOutput {
            scores,
            ranked_indices,
        }
    }

    fn get_param(&self, name: &str) -> Option<Value> {
        match name {
            "normalize" => Some(json!(self.normalize)),
            "threshold" => Some(json!(self.threshold)),
            _ => None,
        }
    }

    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "normalize" => {
                self.normalize = value.as_bool().ok_or("normalize must be bool")?;
                Ok(())
            }
            "threshold" => {
                self.threshold = value.as_f64().ok_or("threshold must be float")?;
                Ok(())
            }
            _ => Err(format!("Unknown parameter: {name}")),
        }
    }

    fn param_names(&self) -> Vec<&'static str> {
        vec!["normalize", "threshold"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_vectors() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((CosineAlgorithm::cosine_similarity(&v, &v) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(CosineAlgorithm::cosine_similarity(&a, &b).abs() < 1e-10);
    }

    #[test]
    fn test_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((CosineAlgorithm::cosine_similarity(&a, &b) + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_vector_search() {
        let alg = CosineAlgorithm::default();
        let input = VectorSearchInput {
            query_vector: vec![1.0, 0.0, 0.0],
            corpus_vectors: vec![
                vec![1.0, 0.0, 0.0], // identical = 1.0
                vec![0.0, 1.0, 0.0], // orthogonal = 0.0
                vec![0.7, 0.7, 0.0], // similar ≈ 0.707
            ],
        };
        let output = alg.vector_search(&input);
        assert_eq!(output.ranked_indices[0], 0); // Most similar first
        assert_eq!(output.ranked_indices[1], 2); // Second similar
    }

    #[test]
    fn test_384_dim_vectors() {
        // Test with typical embedding dimension
        let query: Vec<f64> = (0..384).map(|i| (i as f64 * 0.01).sin()).collect();
        let similar: Vec<f64> = (0..384).map(|i| (i as f64 * 0.01).sin() + 0.01).collect();
        let different: Vec<f64> = (0..384).map(|i| (i as f64 * 0.5).cos()).collect();

        let sim1 = CosineAlgorithm::cosine_similarity(&query, &similar);
        let sim2 = CosineAlgorithm::cosine_similarity(&query, &different);

        // Similar vectors should have higher similarity
        assert!(sim1 > sim2);
        assert!(sim1 > 0.99); // Very similar
    }
}
