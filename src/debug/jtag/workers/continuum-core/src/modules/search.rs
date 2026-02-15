//! SearchModule — Absorbs the standalone search worker into the unified runtime.
//!
//! Provides search algorithms (BoW, BM25, Cosine) with OpenCV-style interface:
//! - Factory creation via algorithm registry
//! - Named parameters with get/set
//! - Polymorphism-based, not template-heavy
//!
//! Commands:
//! - search/execute: Run text search algorithm
//! - search/vector: Run vector similarity search
//! - search/list: List available algorithms
//! - search/params: Get algorithm parameters
//!
//! Migration from: workers/search (258 lines main.rs + algorithms)

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use ts_rs::TS;

// ============================================================================
// Types
// ============================================================================

/// Input to any search algorithm
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/search/SearchInput.ts")]
pub struct SearchInput {
    pub query: String,
    pub corpus: Vec<String>,
}

/// Output from any search algorithm
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/search/SearchOutput.ts")]
pub struct SearchOutput {
    /// Scores normalized to 0-1, parallel to corpus
    pub scores: Vec<f64>,
    /// Indices sorted by score descending
    pub ranked_indices: Vec<usize>,
}

/// Input for vector-based search
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/search/VectorSearchInput.ts")]
#[serde(rename_all = "camelCase")]
pub struct VectorSearchInput {
    pub query_vector: Vec<f64>,
    pub corpus_vectors: Vec<Vec<f64>>,
    #[serde(default = "default_true")]
    pub normalize: bool,
    #[serde(default)]
    pub threshold: f64,
}

fn default_true() -> bool {
    true
}

// ============================================================================
// Algorithm Trait (OpenCV cv::Algorithm style)
// ============================================================================

trait SearchAlgorithm: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, input: &SearchInput) -> SearchOutput;
    fn get_param(&self, name: &str) -> Option<Value>;
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;
    fn param_names(&self) -> Vec<&'static str>;
}

type AlgorithmFactory = fn() -> Box<dyn SearchAlgorithm>;

struct AlgorithmRegistry {
    factories: HashMap<&'static str, AlgorithmFactory>,
}

impl AlgorithmRegistry {
    fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        registry.factories.insert("bow", BowAlgorithm::create);
        registry.factories.insert("bm25", Bm25Algorithm::create);
        registry.factories.insert("cosine", CosineAlgorithm::create);
        registry
    }

    fn create(&self, name: &str) -> Option<Box<dyn SearchAlgorithm>> {
        self.factories.get(name).map(|factory| factory())
    }

    fn create_with_params(
        &self,
        name: &str,
        params: &HashMap<String, Value>,
    ) -> Result<Box<dyn SearchAlgorithm>, String> {
        let mut algo = self
            .create(name)
            .ok_or_else(|| format!("Unknown algorithm: {name}"))?;
        for (key, value) in params {
            algo.set_param(key, value.clone())?;
        }
        Ok(algo)
    }

    fn list(&self) -> Vec<&'static str> {
        self.factories.keys().copied().collect()
    }
}

// ============================================================================
// Bag of Words Algorithm
// ============================================================================

struct BowAlgorithm {
    case_insensitive: bool,
    stopwords: HashSet<String>,
    min_term_length: usize,
}

impl BowAlgorithm {
    fn create() -> Box<dyn SearchAlgorithm> {
        Box::new(Self::default())
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let text = if self.case_insensitive {
            text.to_lowercase()
        } else {
            text.to_string()
        };
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= self.min_term_length)
            .filter(|s| !self.stopwords.contains(*s))
            .map(String::from)
            .collect()
    }

    fn score_document(&self, query_terms: &HashSet<String>, doc: &str) -> f64 {
        let doc_terms: HashSet<String> = self.tokenize(doc).into_iter().collect();
        if doc_terms.is_empty() || query_terms.is_empty() {
            return 0.0;
        }
        let intersection = query_terms.intersection(&doc_terms).count();
        let union = query_terms.union(&doc_terms).count();
        intersection as f64 / union as f64
    }
}

impl Default for BowAlgorithm {
    fn default() -> Self {
        let stopwords: HashSet<String> = [
            "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "must",
            "shall", "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on", "with",
            "at", "by", "from", "as", "into", "through", "during", "before", "after", "above",
            "below", "between", "under", "again", "further", "then", "once", "here", "there",
            "when", "where", "why", "how", "all", "each", "few", "more", "most", "other", "some",
            "such", "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "just",
            "and", "but", "if", "or", "because", "until", "while", "this", "that", "these",
            "those", "it", "its",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            case_insensitive: true,
            stopwords,
            min_term_length: 2,
        }
    }
}

impl SearchAlgorithm for BowAlgorithm {
    fn name(&self) -> &'static str { "bow" }

    fn execute(&self, input: &SearchInput) -> SearchOutput {
        let query_terms: HashSet<String> = self.tokenize(&input.query).into_iter().collect();
        let scores: Vec<f64> = input.corpus.iter()
            .map(|doc| self.score_document(&query_terms, doc))
            .collect();
        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        SearchOutput {
            scores,
            ranked_indices: ranked.into_iter().map(|(i, _)| i).collect(),
        }
    }

    fn get_param(&self, name: &str) -> Option<Value> {
        match name {
            "case_insensitive" => Some(json!(self.case_insensitive)),
            "min_term_length" => Some(json!(self.min_term_length)),
            _ => None,
        }
    }

    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "case_insensitive" => {
                self.case_insensitive = value.as_bool().ok_or("case_insensitive must be bool")?;
                Ok(())
            }
            "min_term_length" => {
                self.min_term_length = value.as_u64().ok_or("min_term_length must be uint")? as usize;
                Ok(())
            }
            _ => Err(format!("Unknown parameter: {name}")),
        }
    }

    fn param_names(&self) -> Vec<&'static str> {
        vec!["case_insensitive", "min_term_length"]
    }
}

// ============================================================================
// BM25 Algorithm
// ============================================================================

struct Bm25Algorithm {
    k1: f64,
    b: f64,
    case_insensitive: bool,
    min_term_length: usize,
}

impl Bm25Algorithm {
    fn create() -> Box<dyn SearchAlgorithm> {
        Box::new(Self::default())
    }

    fn tokenize(&self, text: &str) -> Vec<String> {
        let text = if self.case_insensitive { text.to_lowercase() } else { text.to_string() };
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= self.min_term_length)
            .map(String::from)
            .collect()
    }

    fn term_frequencies(&self, doc: &str) -> HashMap<String, usize> {
        let mut tf: HashMap<String, usize> = HashMap::new();
        for term in self.tokenize(doc) {
            *tf.entry(term).or_insert(0) += 1;
        }
        tf
    }

    fn idf(&self, term: &str, doc_term_freqs: &[HashMap<String, usize>], n: usize) -> f64 {
        let docs_containing = doc_term_freqs.iter().filter(|tf| tf.contains_key(term)).count();
        if docs_containing == 0 { return 0.0; }
        let n_f = n as f64;
        let df = docs_containing as f64;
        ((n_f - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    fn score_document(
        &self,
        query_terms: &[String],
        doc_tf: &HashMap<String, usize>,
        doc_len: usize,
        avg_doc_len: f64,
        idf_cache: &HashMap<String, f64>,
    ) -> f64 {
        let mut score = 0.0;
        for term in query_terms {
            let idf = idf_cache.get(term).copied().unwrap_or(0.0);
            let tf = *doc_tf.get(term).unwrap_or(&0) as f64;
            if tf > 0.0 {
                let numerator = tf * (self.k1 + 1.0);
                let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len as f64 / avg_doc_len));
                score += idf * (numerator / denominator);
            }
        }
        score
    }

    fn normalize_scores(scores: &mut [f64]) {
        let max = scores.iter().cloned().fold(0.0_f64, f64::max);
        if max > 0.0 {
            for score in scores.iter_mut() { *score /= max; }
        }
    }
}

impl Default for Bm25Algorithm {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75, case_insensitive: true, min_term_length: 2 }
    }
}

impl SearchAlgorithm for Bm25Algorithm {
    fn name(&self) -> &'static str { "bm25" }

    fn execute(&self, input: &SearchInput) -> SearchOutput {
        let n = input.corpus.len();
        if n == 0 {
            return SearchOutput { scores: vec![], ranked_indices: vec![] };
        }

        let doc_term_freqs: Vec<HashMap<String, usize>> = input.corpus.iter()
            .map(|doc| self.term_frequencies(doc))
            .collect();
        let doc_lens: Vec<usize> = input.corpus.iter().map(|d| self.tokenize(d).len()).collect();
        let avg_doc_len = doc_lens.iter().sum::<usize>() as f64 / n as f64;
        let query_terms = self.tokenize(&input.query);

        let mut idf_cache: HashMap<String, f64> = HashMap::new();
        for term in &query_terms {
            if !idf_cache.contains_key(term) {
                idf_cache.insert(term.clone(), self.idf(term, &doc_term_freqs, n));
            }
        }

        let mut scores: Vec<f64> = doc_term_freqs.iter().zip(doc_lens.iter())
            .map(|(tf, &len)| self.score_document(&query_terms, tf, len, avg_doc_len, &idf_cache))
            .collect();
        Self::normalize_scores(&mut scores);

        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        SearchOutput {
            scores,
            ranked_indices: ranked.into_iter().map(|(i, _)| i).collect(),
        }
    }

    fn get_param(&self, name: &str) -> Option<Value> {
        match name {
            "k1" => Some(json!(self.k1)),
            "b" => Some(json!(self.b)),
            "case_insensitive" => Some(json!(self.case_insensitive)),
            _ => None,
        }
    }

    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "k1" => { self.k1 = value.as_f64().ok_or("k1 must be float")?; Ok(()) }
            "b" => { self.b = value.as_f64().ok_or("b must be float")?; Ok(()) }
            "case_insensitive" => { self.case_insensitive = value.as_bool().ok_or("case_insensitive must be bool")?; Ok(()) }
            _ => Err(format!("Unknown parameter: {name}")),
        }
    }

    fn param_names(&self) -> Vec<&'static str> {
        vec!["k1", "b", "case_insensitive"]
    }
}

// ============================================================================
// Cosine Similarity Algorithm
// ============================================================================

struct CosineAlgorithm {
    normalize: bool,
    threshold: f64,
}

impl CosineAlgorithm {
    fn create() -> Box<dyn SearchAlgorithm> {
        Box::new(Self::default())
    }

    #[inline]
    fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() || a.is_empty() { return 0.0; }
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;
        for i in 0..a.len() {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }
        let denominator = (norm_a * norm_b).sqrt();
        if denominator == 0.0 { 0.0 } else { dot / denominator }
    }

    fn l2_normalize(v: &mut [f64]) {
        let norm: f64 = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() { *x /= norm; }
        }
    }

    fn vector_search(&self, input: &VectorSearchInput) -> SearchOutput {
        let mut query = input.query_vector.clone();
        if self.normalize { Self::l2_normalize(&mut query); }

        let mut scores: Vec<f64> = Vec::with_capacity(input.corpus_vectors.len());
        for corpus_vec in &input.corpus_vectors {
            let mut cv = corpus_vec.clone();
            if self.normalize { Self::l2_normalize(&mut cv); }
            let sim = Self::cosine_similarity(&query, &cv);
            scores.push(if sim >= self.threshold { sim } else { 0.0 });
        }

        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        SearchOutput {
            scores,
            ranked_indices: ranked.into_iter().map(|(i, _)| i).collect(),
        }
    }
}

impl Default for CosineAlgorithm {
    fn default() -> Self {
        Self { normalize: true, threshold: 0.0 }
    }
}

impl SearchAlgorithm for CosineAlgorithm {
    fn name(&self) -> &'static str { "cosine" }

    fn execute(&self, input: &SearchInput) -> SearchOutput {
        let query_terms: HashSet<_> = input.query.to_lowercase().split_whitespace().map(String::from).collect();
        let scores: Vec<f64> = input.corpus.iter().map(|doc| {
            let doc_terms: HashSet<_> = doc.to_lowercase().split_whitespace().map(String::from).collect();
            if query_terms.is_empty() || doc_terms.is_empty() { return 0.0; }
            let intersection = query_terms.intersection(&doc_terms).count() as f64;
            let union = query_terms.union(&doc_terms).count() as f64;
            if union > 0.0 { intersection / union } else { 0.0 }
        }).collect();

        let mut ranked: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        SearchOutput {
            scores,
            ranked_indices: ranked.into_iter().map(|(i, _)| i).collect(),
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
            "normalize" => { self.normalize = value.as_bool().ok_or("normalize must be bool")?; Ok(()) }
            "threshold" => { self.threshold = value.as_f64().ok_or("threshold must be float")?; Ok(()) }
            _ => Err(format!("Unknown parameter: {name}")),
        }
    }

    fn param_names(&self) -> Vec<&'static str> {
        vec!["normalize", "threshold"]
    }
}

// ============================================================================
// SearchModule — ServiceModule Implementation
// ============================================================================

pub struct SearchModule {
    registry: AlgorithmRegistry,
}

impl SearchModule {
    pub fn new() -> Self {
        Self {
            registry: AlgorithmRegistry::new(),
        }
    }

    fn handle_execute(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let algorithm = p.str_or("algorithm", "bm25");
        let query = p.str("query")?;
        let corpus: Vec<String> = p.array("corpus")?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        let algo_params: HashMap<String, Value> = p.value("params")
            .and_then(|v| v.as_object())
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let algo = if algo_params.is_empty() {
            self.registry.create(algorithm).ok_or_else(|| format!("Unknown algorithm: {algorithm}"))?
        } else {
            self.registry.create_with_params(algorithm, &algo_params)?
        };

        let input = SearchInput { query: query.to_string(), corpus };
        let output = algo.execute(&input);

        Ok(CommandResult::Json(json!({
            "algorithm": algorithm,
            "scores": output.scores,
            "rankedIndices": output.ranked_indices
        })))
    }

    fn handle_vector(&self, params: Value) -> Result<CommandResult, String> {
        let input: VectorSearchInput = serde_json::from_value(params)
            .map_err(|e| format!("Invalid vector search params: {e}"))?;

        let mut algo = CosineAlgorithm::default();
        algo.normalize = input.normalize;
        algo.threshold = input.threshold;

        let output = algo.vector_search(&input);

        Ok(CommandResult::Json(json!({
            "algorithm": "cosine",
            "scores": output.scores,
            "rankedIndices": output.ranked_indices
        })))
    }

    fn handle_list(&self) -> Result<CommandResult, String> {
        Ok(CommandResult::Json(json!({
            "algorithms": self.registry.list()
        })))
    }

    fn handle_params(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let algorithm = p.str("algorithm")?;
        let algo = self.registry.create(algorithm).ok_or_else(|| format!("Unknown algorithm: {algorithm}"))?;

        // Build params with current values using get_param()
        let param_values: serde_json::Map<String, Value> = algo.param_names()
            .iter()
            .filter_map(|name| {
                algo.get_param(name).map(|value| (name.to_string(), value))
            })
            .collect();

        Ok(CommandResult::Json(json!({
            "algorithm": algo.name(),
            "params": algo.param_names(),
            "values": param_values
        })))
    }
}

impl Default for SearchModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for SearchModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "search",
            priority: ModulePriority::Normal,
            command_prefixes: &["search/"],
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
            "search/execute" => self.handle_execute(params),
            "search/vector" => self.handle_vector(params),
            "search/list" => self.handle_list(),
            "search/params" => self.handle_params(params),
            _ => Err(format!("Unknown search command: {command}")),
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
    async fn test_search_list() {
        let module = SearchModule::new();
        let result = module.handle_command("search/list", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let algos = json["algorithms"].as_array().unwrap();
            assert!(algos.len() >= 3); // bow, bm25, cosine
        }
    }

    #[tokio::test]
    async fn test_search_execute() {
        let module = SearchModule::new();
        let params = json!({
            "algorithm": "bm25",
            "query": "genome register",
            "corpus": [
                "Use genome/paging-register with personaId",
                "The weather is nice today",
                "Register genome adapters for personas"
            ]
        });
        let result = module.handle_command("search/execute", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["algorithm"], "bm25");
            let scores = json["scores"].as_array().unwrap();
            assert_eq!(scores.len(), 3);
            // Docs with query terms should score higher
            assert!(scores[0].as_f64().unwrap() > scores[1].as_f64().unwrap());
        }
    }

    #[tokio::test]
    async fn test_vector_search() {
        let module = SearchModule::new();
        let params = json!({
            "queryVector": [1.0, 0.0, 0.0],
            "corpusVectors": [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.7, 0.7, 0.0]
            ],
            "normalize": true,
            "threshold": 0.0
        });
        let result = module.handle_command("search/vector", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let ranked = json["rankedIndices"].as_array().unwrap();
            assert_eq!(ranked[0], 0); // Most similar (identical) first
        }
    }
}
