//! Benchmarking Framework
//!
//! General-purpose benchmarking for:
//! - LoRA adapter quality (genome paging)
//! - Vision pipelines (object detection, segmentation)
//! - Text generation (perplexity, coherence)
//! - Audio generation (TTS, voice cloning)
//! - RAG/search (precision, recall)
//! - Any ML component that needs quality measurement

pub mod generation; // Generation quality (text, audio, image, video)
pub mod lora; // LoRA adapter benchmarking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Benchmark result for a single test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Test case identifier
    pub test_id: String,

    /// Ground truth label/value
    pub ground_truth: serde_json::Value,

    /// Model prediction/output
    pub prediction: serde_json::Value,

    /// Is prediction correct? (for classification)
    pub is_correct: Option<bool>,

    /// Confidence score (0.0-1.0)
    pub confidence: Option<f32>,

    /// Latency in milliseconds
    pub latency_ms: f64,

    /// Custom metrics (e.g., BLEU score, perplexity, IoU)
    pub custom_metrics: HashMap<String, f64>,
}

/// Aggregated benchmark statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkStats {
    /// Total test cases
    pub total: usize,

    /// Accuracy (for classification tasks)
    pub accuracy: Option<f64>,

    /// Average confidence
    pub avg_confidence: Option<f64>,

    /// Latency statistics
    pub latency: LatencyStats,

    /// Custom aggregated metrics
    pub custom_metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    pub mean_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

/// Benchmark suite for testing ML components
pub struct BenchmarkSuite {
    name: String,
    results: Vec<BenchmarkResult>,
}

impl BenchmarkSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            results: Vec::new(),
        }
    }

    /// Add a benchmark result
    pub fn add_result(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    /// Run a single test case with timing
    pub async fn run_test<F, Fut, T>(
        &mut self,
        test_id: impl Into<String>,
        ground_truth: serde_json::Value,
        test_fn: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
        T: Into<serde_json::Value>,
    {
        let start = Instant::now();
        let prediction = test_fn().await?;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        let prediction_value = prediction.into();
        let is_correct = if ground_truth == prediction_value {
            Some(true)
        } else {
            Some(false)
        };

        self.results.push(BenchmarkResult {
            test_id: test_id.into(),
            ground_truth,
            prediction: prediction_value,
            is_correct,
            confidence: None,
            latency_ms,
            custom_metrics: HashMap::new(),
        });

        Ok(())
    }

    /// Compute aggregate statistics
    pub fn compute_stats(&self) -> BenchmarkStats {
        if self.results.is_empty() {
            return BenchmarkStats {
                total: 0,
                accuracy: None,
                avg_confidence: None,
                latency: LatencyStats {
                    mean_ms: 0.0,
                    min_ms: 0.0,
                    max_ms: 0.0,
                    p50_ms: 0.0,
                    p95_ms: 0.0,
                    p99_ms: 0.0,
                },
                custom_metrics: HashMap::new(),
            };
        }

        // Accuracy
        let correct_count = self
            .results
            .iter()
            .filter(|r| r.is_correct.unwrap_or(false))
            .count();
        let accuracy = Some(correct_count as f64 / self.results.len() as f64);

        // Average confidence
        let confidences: Vec<f32> = self
            .results
            .iter()
            .filter_map(|r| r.confidence)
            .collect();
        let avg_confidence = if !confidences.is_empty() {
            Some(confidences.iter().sum::<f32>() / confidences.len() as f32)
        } else {
            None
        };

        // Latency statistics
        let mut latencies: Vec<f64> = self.results.iter().map(|r| r.latency_ms).collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let latency = LatencyStats {
            mean_ms: latencies.iter().sum::<f64>() / latencies.len() as f64,
            min_ms: *latencies.first().unwrap(),
            max_ms: *latencies.last().unwrap(),
            p50_ms: percentile(&latencies, 0.50),
            p95_ms: percentile(&latencies, 0.95),
            p99_ms: percentile(&latencies, 0.99),
        };

        // Custom metrics (average across all results)
        let mut custom_metrics = HashMap::new();
        let all_keys: Vec<String> = self
            .results
            .iter()
            .flat_map(|r| r.custom_metrics.keys().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for key in all_keys {
            let values: Vec<f64> = self
                .results
                .iter()
                .filter_map(|r| r.custom_metrics.get(&key).copied())
                .collect();

            if !values.is_empty() {
                custom_metrics.insert(key, values.iter().sum::<f64>() / values.len() as f64);
            }
        }

        BenchmarkStats {
            total: self.results.len(),
            accuracy,
            avg_confidence: avg_confidence.map(|c| c as f64),
            latency,
            custom_metrics,
        }
    }

    /// Generate markdown report
    pub fn report(&self) -> String {
        let stats = self.compute_stats();

        let mut report = format!("# Benchmark Report: {}\n\n", self.name);

        report.push_str(&format!("**Total Tests**: {}\n\n", stats.total));

        if let Some(acc) = stats.accuracy {
            report.push_str(&format!("**Accuracy**: {:.2}%\n\n", acc * 100.0));
        }

        if let Some(conf) = stats.avg_confidence {
            report.push_str(&format!("**Avg Confidence**: {:.3}\n\n", conf));
        }

        report.push_str("## Latency\n\n");
        report.push_str(&format!("- Mean: {:.2}ms\n", stats.latency.mean_ms));
        report.push_str(&format!("- Min: {:.2}ms\n", stats.latency.min_ms));
        report.push_str(&format!("- Max: {:.2}ms\n", stats.latency.max_ms));
        report.push_str(&format!("- P50: {:.2}ms\n", stats.latency.p50_ms));
        report.push_str(&format!("- P95: {:.2}ms\n", stats.latency.p95_ms));
        report.push_str(&format!("- P99: {:.2}ms\n\n", stats.latency.p99_ms));

        if !stats.custom_metrics.is_empty() {
            report.push_str("## Custom Metrics\n\n");
            for (key, value) in &stats.custom_metrics {
                report.push_str(&format!("- {}: {:.4}\n", key, value));
            }
            report.push_str("\n");
        }

        report
    }

    /// Export results to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.results)
    }

    /// Import results from JSON
    pub fn from_json(name: impl Into<String>, json: &str) -> Result<Self, serde_json::Error> {
        let results: Vec<BenchmarkResult> = serde_json::from_str(json)?;
        Ok(Self {
            name: name.into(),
            results,
        })
    }
}

/// Calculate percentile from sorted array
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_benchmark_suite() {
        let mut suite = BenchmarkSuite::new("Test Suite");

        // Add some test results
        suite.add_result(BenchmarkResult {
            test_id: "test1".into(),
            ground_truth: serde_json::json!("cat"),
            prediction: serde_json::json!("cat"),
            is_correct: Some(true),
            confidence: Some(0.95),
            latency_ms: 10.5,
            custom_metrics: HashMap::new(),
        });

        suite.add_result(BenchmarkResult {
            test_id: "test2".into(),
            ground_truth: serde_json::json!("dog"),
            prediction: serde_json::json!("cat"),
            is_correct: Some(false),
            confidence: Some(0.70),
            latency_ms: 12.3,
            custom_metrics: HashMap::new(),
        });

        let stats = suite.compute_stats();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.accuracy, Some(0.5));

        println!("{}", suite.report());
    }
}
