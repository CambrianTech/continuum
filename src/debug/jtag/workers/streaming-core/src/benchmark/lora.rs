//! LoRA Adapter Benchmarking
//!
//! Measures quality of LoRA fine-tuned models for genome paging:
//! - Task-specific accuracy (before/after fine-tuning)
//! - Overfitting detection (train vs validation loss)
//! - Catastrophic forgetting (base task degradation)
//! - Adapter size vs quality tradeoff
//! - Inference latency with/without adapter
//!
//! ## Integration with Existing LoRA Infrastructure
//!
//! This module builds on top of the existing LoRA implementation:
//! - `/workers/inference-grpc/src/lora.rs` - LoRA loading and weight merging
//! - `/workers/inference-grpc/src/adapter_registry.rs` - Adapter management
//!
//! Use this for QUALITY MEASUREMENT before deploying adapters to genome.

use serde::{Deserialize, Serialize};

/// LoRA adapter metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAAdapterInfo {
    /// Adapter name/ID
    pub name: String,

    /// Base model being adapted
    pub base_model: String,

    /// Target task/domain
    pub task: String,

    /// Adapter rank (1-256, typical: 8-32)
    pub rank: usize,

    /// Alpha parameter (scaling factor, typical: 16-64)
    pub alpha: f32,

    /// Training samples used
    pub training_samples: usize,

    /// Training epochs
    pub epochs: usize,

    /// Adapter file size in bytes
    pub size_bytes: usize,
}

/// Benchmark result comparing base model vs LoRA adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAComparisonResult {
    /// Test case ID
    pub test_id: String,

    /// Ground truth
    pub ground_truth: String,

    /// Base model prediction
    pub base_prediction: String,

    /// LoRA adapter prediction
    pub lora_prediction: String,

    /// Base model was correct
    pub base_correct: bool,

    /// LoRA adapter was correct
    pub lora_correct: bool,

    /// Improvement (true if LoRA fixed base model error)
    pub improvement: bool,

    /// Regression (true if LoRA broke base model correctness)
    pub regression: bool,

    /// Base model confidence
    pub base_confidence: Option<f32>,

    /// LoRA adapter confidence
    pub lora_confidence: Option<f32>,

    /// Latency increase (ms) with adapter loaded
    pub latency_overhead_ms: f64,
}

/// LoRA adapter quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAQualityMetrics {
    /// Adapter metadata
    pub adapter_info: LoRAAdapterInfo,

    /// Total test cases
    pub total: usize,

    /// Base model accuracy
    pub base_accuracy: f64,

    /// LoRA adapter accuracy
    pub lora_accuracy: f64,

    /// Accuracy improvement (lora - base)
    pub accuracy_improvement: f64,

    /// Number of improvements (LoRA fixed base error)
    pub improvements: usize,

    /// Number of regressions (LoRA broke base correctness)
    pub regressions: usize,

    /// Average latency overhead (ms) when adapter loaded
    pub avg_latency_overhead_ms: f64,

    /// Overfitting score (0.0-1.0, higher = more overfit)
    /// Calculated as (train_acc - val_acc)
    pub overfitting_score: Option<f64>,

    /// Catastrophic forgetting (base task degradation)
    pub forgetting_score: Option<f64>,
}

/// LoRA benchmark suite for genome quality measurement
pub struct LoRABenchmarkSuite {
    name: String,
    adapter_info: LoRAAdapterInfo,
    results: Vec<LoRAComparisonResult>,
}

impl LoRABenchmarkSuite {
    pub fn new(name: impl Into<String>, adapter_info: LoRAAdapterInfo) -> Self {
        Self {
            name: name.into(),
            adapter_info,
            results: Vec::new(),
        }
    }

    /// Add a comparison result
    pub fn add_result(&mut self, result: LoRAComparisonResult) {
        self.results.push(result);
    }

    /// Compute quality metrics
    pub fn compute_metrics(&self) -> LoRAQualityMetrics {
        if self.results.is_empty() {
            return LoRAQualityMetrics {
                adapter_info: self.adapter_info.clone(),
                total: 0,
                base_accuracy: 0.0,
                lora_accuracy: 0.0,
                accuracy_improvement: 0.0,
                improvements: 0,
                regressions: 0,
                avg_latency_overhead_ms: 0.0,
                overfitting_score: None,
                forgetting_score: None,
            };
        }

        let base_correct = self.results.iter().filter(|r| r.base_correct).count();
        let lora_correct = self.results.iter().filter(|r| r.lora_correct).count();
        let improvements = self.results.iter().filter(|r| r.improvement).count();
        let regressions = self.results.iter().filter(|r| r.regression).count();

        let total = self.results.len();
        let base_accuracy = base_correct as f64 / total as f64;
        let lora_accuracy = lora_correct as f64 / total as f64;
        let accuracy_improvement = lora_accuracy - base_accuracy;

        let avg_latency_overhead_ms = self
            .results
            .iter()
            .map(|r| r.latency_overhead_ms)
            .sum::<f64>()
            / total as f64;

        LoRAQualityMetrics {
            adapter_info: self.adapter_info.clone(),
            total,
            base_accuracy,
            lora_accuracy,
            accuracy_improvement,
            improvements,
            regressions,
            avg_latency_overhead_ms,
            overfitting_score: None, // Requires separate train/val datasets
            forgetting_score: None,  // Requires base task benchmark
        }
    }

    /// Generate markdown report
    pub fn report(&self) -> String {
        let metrics = self.compute_metrics();

        let mut report = format!("# LoRA Adapter Benchmark: {}\n\n", self.name);

        report.push_str("## Adapter Info\n\n");
        report.push_str(&format!("- **Name**: {}\n", metrics.adapter_info.name));
        report.push_str(&format!("- **Base Model**: {}\n", metrics.adapter_info.base_model));
        report.push_str(&format!("- **Task**: {}\n", metrics.adapter_info.task));
        report.push_str(&format!("- **Rank**: {}\n", metrics.adapter_info.rank));
        report.push_str(&format!("- **Alpha**: {}\n", metrics.adapter_info.alpha));
        report.push_str(&format!(
            "- **Training Samples**: {}\n",
            metrics.adapter_info.training_samples
        ));
        report.push_str(&format!("- **Epochs**: {}\n", metrics.adapter_info.epochs));
        report.push_str(&format!(
            "- **Size**: {:.2} MB\n\n",
            metrics.adapter_info.size_bytes as f64 / 1_000_000.0
        ));

        report.push_str("## Quality Metrics\n\n");
        report.push_str(&format!("- **Total Tests**: {}\n", metrics.total));
        report.push_str(&format!(
            "- **Base Accuracy**: {:.2}%\n",
            metrics.base_accuracy * 100.0
        ));
        report.push_str(&format!(
            "- **LoRA Accuracy**: {:.2}%\n",
            metrics.lora_accuracy * 100.0
        ));
        report.push_str(&format!(
            "- **Improvement**: {:+.2}%\n\n",
            metrics.accuracy_improvement * 100.0
        ));

        report.push_str("## Change Analysis\n\n");
        report.push_str(&format!(
            "- **Improvements**: {} (LoRA fixed base errors)\n",
            metrics.improvements
        ));
        report.push_str(&format!(
            "- **Regressions**: {} (LoRA broke base correctness)\n\n",
            metrics.regressions
        ));

        report.push_str("## Performance\n\n");
        report.push_str(&format!(
            "- **Avg Latency Overhead**: {:.2}ms\n\n",
            metrics.avg_latency_overhead_ms
        ));

        if let Some(overfit) = metrics.overfitting_score {
            report.push_str(&format!(
                "- **Overfitting Score**: {:.2}% (train-val gap)\n",
                overfit * 100.0
            ));
        }

        if let Some(forget) = metrics.forgetting_score {
            report.push_str(&format!(
                "- **Forgetting Score**: {:.2}% (base task degradation)\n",
                forget * 100.0
            ));
        }

        report
    }

    /// Export to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let metrics = self.compute_metrics();
        serde_json::to_string_pretty(&metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lora_benchmark() {
        let adapter_info = LoRAAdapterInfo {
            name: "typescript-expert".into(),
            base_model: "llama-3.2-3b".into(),
            task: "typescript-code-review".into(),
            rank: 16,
            alpha: 32.0,
            training_samples: 1000,
            epochs: 3,
            size_bytes: 25_000_000, // 25MB
        };

        let mut suite = LoRABenchmarkSuite::new("TypeScript Expert Eval", adapter_info);

        // Add test results
        suite.add_result(LoRAComparisonResult {
            test_id: "test1".into(),
            ground_truth: "type error".into(),
            base_prediction: "syntax error".into(), // wrong
            lora_prediction: "type error".into(),    // correct
            base_correct: false,
            lora_correct: true,
            improvement: true,
            regression: false,
            base_confidence: Some(0.6),
            lora_confidence: Some(0.95),
            latency_overhead_ms: 5.2,
        });

        suite.add_result(LoRAComparisonResult {
            test_id: "test2".into(),
            ground_truth: "no error".into(),
            base_prediction: "no error".into(), // correct
            lora_prediction: "no error".into(), // correct
            base_correct: true,
            lora_correct: true,
            improvement: false,
            regression: false,
            base_confidence: Some(0.85),
            lora_confidence: Some(0.90),
            latency_overhead_ms: 4.8,
        });

        let metrics = suite.compute_metrics();
        assert_eq!(metrics.total, 2);
        assert_eq!(metrics.base_accuracy, 0.5); // 1/2 correct
        assert_eq!(metrics.lora_accuracy, 1.0); // 2/2 correct
        assert_eq!(metrics.improvements, 1);
        assert_eq!(metrics.regressions, 0);

        println!("{}", suite.report());
    }
}
