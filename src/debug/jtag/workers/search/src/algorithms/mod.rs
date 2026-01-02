/// Search Algorithm Trait and Registry
///
/// Pattern: OpenCV cv::Algorithm style
/// - Factory creation via create()
/// - Named parameters with get/set
/// - Polymorphism-based, not template-heavy
/// - Serializable (save/load params)

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub mod bow;
pub mod bm25;
pub mod cosine;

// ============================================================================
// Core Types
// ============================================================================

/// Input to any search algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub corpus: Vec<String>,
}

/// Output from any search algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOutput {
    /// Scores normalized to 0-1, parallel to corpus
    pub scores: Vec<f64>,
    /// Indices sorted by score descending
    pub ranked_indices: Vec<usize>,
}

// ============================================================================
// Algorithm Trait (OpenCV cv::Algorithm style)
// ============================================================================

/// Core trait - all search algorithms implement this
#[allow(dead_code)]
pub trait SearchAlgorithm: Send + Sync {
    /// Algorithm identifier (like cv::Algorithm::getDefaultName)
    fn name(&self) -> &'static str;

    /// Execute search, return scored results
    fn execute(&self, input: &SearchInput) -> SearchOutput;

    /// Get parameter by name
    fn get_param(&self, name: &str) -> Option<Value>;

    /// Set parameter by name
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;

    /// List available parameters
    fn param_names(&self) -> Vec<&'static str>;

    /// Check if algorithm is properly initialized
    fn is_empty(&self) -> bool {
        false
    }

    /// Clear algorithm state
    fn clear(&mut self) {}
}

// ============================================================================
// Algorithm Registry
// ============================================================================

/// Factory function type
type AlgorithmFactory = fn() -> Box<dyn SearchAlgorithm>;

/// Registry for algorithm factories (like cv::Algorithm::create pattern)
pub struct AlgorithmRegistry {
    factories: HashMap<&'static str, AlgorithmFactory>,
}

impl AlgorithmRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };

        // Register factories
        registry.register("bow", bow::BowAlgorithm::create);
        registry.register("bm25", bm25::Bm25Algorithm::create);
        registry.register("cosine", cosine::CosineAlgorithm::create);

        registry
    }

    pub fn register(&mut self, name: &'static str, factory: AlgorithmFactory) {
        println!("📝 Registered algorithm factory: {}", name);
        self.factories.insert(name, factory);
    }

    /// Create algorithm instance by name (like cv::Algorithm::create<T>)
    pub fn create(&self, name: &str) -> Option<Box<dyn SearchAlgorithm>> {
        self.factories.get(name).map(|factory| factory())
    }

    /// Create and configure in one step
    pub fn create_with_params(
        &self,
        name: &str,
        params: &HashMap<String, Value>,
    ) -> Result<Box<dyn SearchAlgorithm>, String> {
        let mut algo = self
            .create(name)
            .ok_or_else(|| format!("Unknown algorithm: {}", name))?;

        for (key, value) in params {
            algo.set_param(key, value.clone())?;
        }

        Ok(algo)
    }

    pub fn list(&self) -> Vec<&'static str> {
        self.factories.keys().copied().collect()
    }
}

impl Default for AlgorithmRegistry {
    fn default() -> Self {
        Self::new()
    }
}
