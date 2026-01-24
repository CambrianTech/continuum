# Benchmarking Framework

General-purpose benchmarking for ML components, critical for LoRA genome work and quality tracking.

## Purpose

Track quality metrics across:
- **LoRA adapters** (genome paging) - measure adapter effectiveness, detect overfitting
- **Audio generation** (TTS, voice cloning) - PESQ, MOS, prosody
- **Text generation** (LLMs, code completion) - perplexity, BLEU, semantic similarity
- **Image generation** (Stable Diffusion) - FID, SSIM, aesthetic score
- **Vision pipelines** (object detection) - mAP, IoU
- **VAD systems** - accuracy, precision, recall, FPR
- **Any ML component** - custom metrics, latency, quality over time

## Architecture

```
BenchmarkSuite (generic)
├── BenchmarkResult (per test case)
│   ├── ground_truth
│   ├── prediction
│   ├── is_correct (for classification)
│   ├── confidence
│   ├── latency_ms
│   └── custom_metrics (domain-specific)
└── BenchmarkStats (aggregated)
    ├── accuracy, precision, recall
    ├── latency (mean, p50, p95, p99)
    └── custom metrics (averaged)

Specialized:
├── LoRABenchmarkSuite
│   ├── LoRAAdapterInfo (metadata)
│   ├── LoRAComparisonResult (base vs adapted)
│   └── LoRAQualityMetrics (improvement, regression, overfitting)
└── GenerationBenchmarkSuite
    ├── AudioQualityMetrics (PESQ, MOS, SNR)
    ├── TextQualityMetrics (BLEU, perplexity)
    └── ImageQualityMetrics (FID, SSIM, CLIP score)
```

## Usage

### Basic Example: VAD Quality

```rust
use streaming_core::benchmark::BenchmarkSuite;

let mut suite = BenchmarkSuite::new("VAD Evaluation");

// Run tests
for (audio, ground_truth) in test_dataset {
    let start = Instant::now();
    let result = vad.detect(&audio).await?;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    let is_speech = result.confidence > 0.3;

    suite.add_result(BenchmarkResult {
        test_id: "test1".into(),
        ground_truth: json!("speech"),
        prediction: json!(if is_speech { "speech" } else { "silence" }),
        is_correct: Some(is_speech),
        confidence: Some(result.confidence),
        latency_ms,
        custom_metrics: HashMap::new(),
    });
}

// Generate report
println!("{}", suite.report());

// Export JSON for tracking
std::fs::write("results.json", suite.to_json()?)?;
```

### LoRA Adapter Benchmarking

Critical for genome paging - measure adapter quality before deploying:

```rust
use streaming_core::benchmark::lora::{LoRABenchmarkSuite, LoRAAdapterInfo};

let adapter_info = LoRAAdapterInfo {
    name: "typescript-expert",
    base_model: "llama-3.2-3b",
    task: "typescript-code-review",
    rank: 16,
    alpha: 32.0,
    training_samples: 1000,
    epochs: 3,
    size_bytes: 25_000_000,
};

let mut suite = LoRABenchmarkSuite::new("TypeScript Expert Eval", adapter_info);

// Compare base vs adapted model
for test_case in dataset {
    let base_pred = base_model.predict(&test_case.input).await?;
    let lora_pred = lora_model.predict(&test_case.input).await?;

    suite.add_result(LoRAComparisonResult {
        test_id: test_case.id,
        ground_truth: test_case.expected,
        base_prediction: base_pred,
        lora_prediction: lora_pred,
        base_correct: base_pred == test_case.expected,
        lora_correct: lora_pred == test_case.expected,
        improvement: !base_correct && lora_correct,
        regression: base_correct && !lora_correct,
        latency_overhead_ms: lora_latency - base_latency,
        ...
    });
}

let metrics = suite.compute_metrics();
println!("Accuracy improvement: {:+.2}%", metrics.accuracy_improvement * 100.0);
println!("Improvements: {} (LoRA fixed base errors)", metrics.improvements);
println!("Regressions: {} (LoRA broke base)", metrics.regressions);
```

### Audio Generation Quality

```rust
use streaming_core::benchmark::generation::{GenerationBenchmarkSuite, AudioQualityMetrics};

let mut suite = GenerationBenchmarkSuite::new("Kokoro TTS", "audio");

for prompt in test_prompts {
    let start = Instant::now();
    let audio = tts.synthesize(prompt).await?;
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    let mut metrics = HashMap::new();
    metrics.insert("pesq", calculate_pesq(&audio, &reference));
    metrics.insert("mos", human_rating.mos);

    suite.add_result(GenerationResult {
        test_id: "prompt1",
        prompt: prompt.to_string(),
        generated: json!({"audio_samples": audio.len()}),
        human_rating: Some(HumanRating { quality: 5, ... }),
        metrics,
        latency_ms,
        model: "kokoro-v1",
        ...
    });
}

println!("{}", suite.report());
```

## Genome Integration

**Critical for LoRA paging** - before evicting/loading adapters:

1. **Quality gate**: Only evict adapters above quality threshold
2. **Overfit detection**: Compare train vs validation metrics
3. **Forgetting check**: Ensure base tasks don't degrade
4. **Performance tracking**: Monitor latency overhead

```rust
// Before evicting adapter from genome
let metrics = benchmark_adapter(&adapter).await;

if metrics.accuracy_improvement < 0.05 {
    // Adapter provides <5% improvement, safe to evict
    genome.evict_adapter(&adapter.name);
} else if metrics.regressions > 0 {
    // Adapter breaks base model, don't use
    genome.mark_broken(&adapter.name);
} else if metrics.overfitting_score > 0.2 {
    // Adapter overfit (20% train-val gap), retrain with more data
    genome.schedule_retrain(&adapter.name);
}
```

## Tracking Over Time

Export JSON after each benchmark run:

```rust
let timestamp = chrono::Utc::now().to_rfc3339();
let filename = format!("benchmarks/{}_{}_{}.json",
    suite_name, model_name, timestamp);
std::fs::write(filename, suite.to_json()?)?;
```

Then compare across versions:

```bash
# Plot accuracy over time
cat benchmarks/vad_*.json | jq '.[] | .accuracy' | plot

# Compare LoRA adapter versions
diff benchmarks/typescript-expert_v1.json benchmarks/typescript-expert_v2.json
```

## Metrics Reference

### Classification

- **Accuracy**: (TP + TN) / Total
- **Precision**: TP / (TP + FP) - how many detections are correct
- **Recall**: TP / (TP + FN) - how many positives are detected
- **F1**: 2 × (Precision × Recall) / (Precision + Recall)
- **FPR**: FP / (FP + TN) - false alarm rate
- **FNR**: FN / (TP + FN) - miss rate

### Latency

- **Mean**: Average latency
- **P50**: Median latency
- **P95**: 95th percentile (worst 5% cases)
- **P99**: 99th percentile (worst 1% cases)

### Generation Quality

- **PESQ**: Perceptual speech quality (1.0-4.5)
- **MOS**: Mean opinion score (1.0-5.0)
- **BLEU**: Text similarity (0.0-1.0)
- **FID**: Image distribution distance (lower = better)
- **CLIP**: Prompt-image alignment (0.0-1.0)

## Best Practices

1. **Separate train/val/test**: Never benchmark on training data
2. **Stratified sampling**: Ensure diverse test cases (easy/hard, common/rare)
3. **Multiple runs**: Average over 3-5 runs to reduce variance
4. **Version control**: Tag benchmarks with model/adapter version
5. **Human validation**: Combine objective metrics with human ratings
6. **Track over time**: Export JSON after every run for trend analysis

## Future Enhancements

- [ ] A/B testing framework (compare two models head-to-head)
- [ ] Preference-based ranking (GPT-4 as judge)
- [ ] Ablation testing (measure impact of each component)
- [ ] Continuous monitoring (track metrics in production)
- [ ] Auto-regression detection (alert on quality drops)
