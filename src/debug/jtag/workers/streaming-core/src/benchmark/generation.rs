//! Generation Quality Benchmarking
//!
//! Measures quality for ANY generation task:
//! - Text generation (LLMs, code completion)
//! - Audio generation (TTS, voice cloning, music)
//! - Image generation (Stable Diffusion, LoRA layers)
//! - Video generation
//!
//! Metrics:
//! - Subjective: Human ratings, preference tests
//! - Objective: Perplexity, BLEU, PESQ, SSIM, FID
//! - Diversity: N-gram uniqueness, spectral diversity
//! - Consistency: Multi-run variance

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Human rating for subjective quality (1-5 scale)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HumanRating {
    /// Overall quality (1-5)
    pub quality: u8,

    /// Naturalness/realism (1-5)
    pub naturalness: u8,

    /// Coherence/consistency (1-5)
    pub coherence: u8,

    /// Prompt adherence (1-5)
    pub prompt_adherence: u8,
}

/// Generation benchmark result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    /// Test case ID
    pub test_id: String,

    /// Input prompt/condition
    pub prompt: String,

    /// Ground truth (if available, e.g., reference audio/text)
    pub ground_truth: Option<serde_json::Value>,

    /// Generated output
    pub generated: serde_json::Value,

    /// Human rating (if available)
    pub human_rating: Option<HumanRating>,

    /// Objective metrics
    pub metrics: HashMap<String, f64>,

    /// Generation latency (ms)
    pub latency_ms: f64,

    /// Model/adapter used
    pub model: String,
}

/// Audio generation quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioQualityMetrics {
    /// PESQ score (1.0-4.5, higher = better)
    /// Perceptual Evaluation of Speech Quality
    pub pesq: Option<f64>,

    /// MOS (Mean Opinion Score, 1.0-5.0)
    pub mos: Option<f64>,

    /// SNR (Signal-to-Noise Ratio, dB)
    pub snr_db: Option<f64>,

    /// Spectral distortion
    pub spectral_distortion: Option<f64>,

    /// Prosody naturalness (0.0-1.0)
    pub prosody_score: Option<f64>,

    /// Voice similarity (for voice cloning, 0.0-1.0)
    pub voice_similarity: Option<f64>,
}

/// Text generation quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextQualityMetrics {
    /// Perplexity (lower = better)
    pub perplexity: Option<f64>,

    /// BLEU score (0.0-1.0, higher = better)
    pub bleu: Option<f64>,

    /// ROUGE score (0.0-1.0)
    pub rouge: Option<f64>,

    /// Exact match accuracy (0.0-1.0)
    pub exact_match: Option<f64>,

    /// Semantic similarity (cosine, 0.0-1.0)
    pub semantic_similarity: Option<f64>,

    /// Diversity (unique n-grams ratio)
    pub diversity: Option<f64>,
}

/// Image generation quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageQualityMetrics {
    /// FID (Fréchet Inception Distance, lower = better)
    pub fid: Option<f64>,

    /// IS (Inception Score, higher = better)
    pub inception_score: Option<f64>,

    /// SSIM (Structural Similarity, 0.0-1.0)
    pub ssim: Option<f64>,

    /// PSNR (Peak Signal-to-Noise Ratio, dB)
    pub psnr_db: Option<f64>,

    /// CLIP score (prompt-image alignment, 0.0-1.0)
    pub clip_score: Option<f64>,

    /// Aesthetic score (0.0-10.0)
    pub aesthetic_score: Option<f64>,
}

/// Generation benchmark suite
pub struct GenerationBenchmarkSuite {
    name: String,
    domain: String, // "text", "audio", "image", "video"
    results: Vec<GenerationResult>,
}

impl GenerationBenchmarkSuite {
    pub fn new(name: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            domain: domain.into(),
            results: Vec::new(),
        }
    }

    /// Add generation result
    pub fn add_result(&mut self, result: GenerationResult) {
        self.results.push(result);
    }

    /// Compute aggregate metrics
    pub fn compute_metrics(&self) -> HashMap<String, f64> {
        if self.results.is_empty() {
            return HashMap::new();
        }

        let mut aggregated = HashMap::new();

        // Collect all metric keys
        let all_keys: Vec<String> = self
            .results
            .iter()
            .flat_map(|r| r.metrics.keys().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Average each metric
        for key in all_keys {
            let values: Vec<f64> = self
                .results
                .iter()
                .filter_map(|r| r.metrics.get(&key).copied())
                .collect();

            if !values.is_empty() {
                aggregated.insert(key, values.iter().sum::<f64>() / values.len() as f64);
            }
        }

        // Aggregate human ratings if present
        let human_ratings: Vec<&HumanRating> =
            self.results.iter().filter_map(|r| r.human_rating.as_ref()).collect();

        if !human_ratings.is_empty() {
            let avg_quality = human_ratings.iter().map(|r| r.quality as f64).sum::<f64>()
                / human_ratings.len() as f64;
            let avg_naturalness = human_ratings.iter().map(|r| r.naturalness as f64).sum::<f64>()
                / human_ratings.len() as f64;
            let avg_coherence = human_ratings.iter().map(|r| r.coherence as f64).sum::<f64>()
                / human_ratings.len() as f64;
            let avg_adherence = human_ratings
                .iter()
                .map(|r| r.prompt_adherence as f64)
                .sum::<f64>()
                / human_ratings.len() as f64;

            aggregated.insert("human_quality".into(), avg_quality);
            aggregated.insert("human_naturalness".into(), avg_naturalness);
            aggregated.insert("human_coherence".into(), avg_coherence);
            aggregated.insert("human_prompt_adherence".into(), avg_adherence);
        }

        // Average latency
        let avg_latency =
            self.results.iter().map(|r| r.latency_ms).sum::<f64>() / self.results.len() as f64;
        aggregated.insert("avg_latency_ms".into(), avg_latency);

        aggregated
    }

    /// Generate markdown report
    pub fn report(&self) -> String {
        let metrics = self.compute_metrics();

        let mut report = format!("# Generation Benchmark: {} ({})\n\n", self.name, self.domain);

        report.push_str(&format!("**Total Samples**: {}\n\n", self.results.len()));

        report.push_str("## Quality Metrics\n\n");

        // Human ratings
        if metrics.contains_key("human_quality") {
            report.push_str("### Human Ratings (1-5 scale)\n\n");
            if let Some(quality) = metrics.get("human_quality") {
                report.push_str(&format!("- **Quality**: {:.2}/5\n", quality));
            }
            if let Some(naturalness) = metrics.get("human_naturalness") {
                report.push_str(&format!("- **Naturalness**: {:.2}/5\n", naturalness));
            }
            if let Some(coherence) = metrics.get("human_coherence") {
                report.push_str(&format!("- **Coherence**: {:.2}/5\n", coherence));
            }
            if let Some(adherence) = metrics.get("human_prompt_adherence") {
                report.push_str(&format!("- **Prompt Adherence**: {:.2}/5\n\n", adherence));
            }
        }

        // Objective metrics
        report.push_str("### Objective Metrics\n\n");
        for (key, value) in &metrics {
            if !key.starts_with("human_") && key != "avg_latency_ms" {
                report.push_str(&format!("- **{}**: {:.4}\n", key, value));
            }
        }
        report.push_str("\n");

        // Performance
        if let Some(latency) = metrics.get("avg_latency_ms") {
            report.push_str("## Performance\n\n");
            report.push_str(&format!("- **Avg Latency**: {:.2}ms\n\n", latency));
        }

        report
    }

    /// Export to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_generation_benchmark() {
        let mut suite = GenerationBenchmarkSuite::new("Kokoro TTS", "audio");

        let mut metrics = HashMap::new();
        metrics.insert("pesq".into(), 4.2);
        metrics.insert("mos".into(), 4.5);
        metrics.insert("snr_db".into(), 35.0);

        suite.add_result(GenerationResult {
            test_id: "test1".into(),
            prompt: "Hello, how are you?".into(),
            ground_truth: None,
            generated: serde_json::json!({"audio_samples": 24000}),
            human_rating: Some(HumanRating {
                quality: 5,
                naturalness: 5,
                coherence: 5,
                prompt_adherence: 5,
            }),
            metrics,
            latency_ms: 250.0,
            model: "kokoro-v1".into(),
        });

        let agg = suite.compute_metrics();
        assert!(agg.contains_key("pesq"));
        assert!(agg.contains_key("human_quality"));

        println!("{}", suite.report());
    }
}
