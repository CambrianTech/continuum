//! Test Qwen3.5 safetensors inference through Candle.
//!
//! Usage: cargo run --bin test_qwen35 --features metal -- --model-dir /path/to/model
//! Or on BigMama: cargo run --bin test_qwen35 --features cuda -- --model-dir ~/sentinel-ai/output/forged/qwen3.5-4b-code-128k/model

use std::path::Path;
use std::time::Instant;

use candle_core::{DType, Device};
use candle_nn::VarBuilder;

fn main() {
    let default_dir = format!(
        "{}/sentinel-ai/output/forged/qwen3.5-4b-code-128k/model",
        std::env::var("HOME").unwrap_or("/tmp".into())
    );
    let model_dir = std::env::args()
        .skip_while(|a| a != "--model-dir")
        .nth(1)
        .unwrap_or(default_dir);
    let max_tokens: usize = std::env::var("MAX_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);

    eprintln!("=== Qwen3.5 Candle Inference Test ===");
    eprintln!("Model: {model_dir}");

    // Device selection
    let device = {
        #[cfg(feature = "metal")]
        {
            eprintln!("Device: Metal");
            candle_core::Device::new_metal(0).expect("Metal device")
        }
        #[cfg(feature = "cuda")]
        {
            eprintln!("Device: CUDA");
            candle_core::Device::new_cuda(0).expect("CUDA device")
        }
        #[cfg(not(any(feature = "metal", feature = "cuda")))]
        {
            eprintln!("Device: CPU");
            candle_core::Device::Cpu
        }
    };

    // Load config
    let config_path = Path::new(&model_dir).join("config.json");
    let config_str = std::fs::read_to_string(&config_path).expect("read config.json");
    let config_json: serde_json::Value = serde_json::from_str(&config_str).expect("parse config");
    let config =
        continuum_core::inference::vendored::qwen35::Qwen35Config::from_json(&config_json)
            .expect("parse Qwen3.5 config");
    eprintln!("Config: {} layers, {} hidden, {} heads",
        config.num_hidden_layers, config.hidden_size, config.num_attention_heads);
    eprintln!("Layer types: {} linear, {} full",
        config.layer_types.iter().filter(|t| matches!(t, continuum_core::inference::vendored::qwen35::LayerType::LinearAttention)).count(),
        config.layer_types.iter().filter(|t| matches!(t, continuum_core::inference::vendored::qwen35::LayerType::FullAttention)).count(),
    );

    // Load tokenizer
    let tokenizer_path = Path::new(&model_dir).join("tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("load tokenizer");
    eprintln!("Tokenizer loaded: {} vocab", tokenizer.get_vocab_size(false));

    // Load weights
    eprintln!("Loading weights...");
    let start = Instant::now();
    let safetensors: Vec<std::path::PathBuf> = std::fs::read_dir(&model_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "safetensors").unwrap_or(false))
        .collect();

    if safetensors.is_empty() {
        eprintln!("ERROR: No safetensors files found in {model_dir}");
        std::process::exit(1);
    }

    let dtype = DType::F16; // Match the forged model's dtype
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&safetensors, dtype, &device)
            .expect("load safetensors")
    };
    eprintln!("Weights loaded in {:.1}s", start.elapsed().as_secs_f64());

    // Build model
    eprintln!("Building model...");
    let model = continuum_core::inference::vendored::qwen35::Qwen35::load(vb, &config)
        .expect("build Qwen3.5 model");
    device.synchronize().ok();
    eprintln!("Model built. Context length: {}", model.context_length);

    // Create backend
    let eos_ids = vec![config_json.get("text_config")
        .or(Some(&config_json))
        .and_then(|tc| tc.get("eos_token_id"))
        .and_then(|v| v.as_u64())
        .unwrap_or(248044) as u32];

    let mut backend = continuum_core::inference::backends::qwen35_safetensors::Qwen35SafetensorsBackend::new(
        model, tokenizer, device.clone(), dtype,
        "qwen3.5-4b-code-128k-forged".to_string(),
        eos_ids,
        safetensors,
    );

    // Test prompts
    let prompts = [
        "def fibonacci(n):\n    ",
        "def merge_sort(arr):\n    ",
        "# Binary search implementation\ndef binary_search(arr, target):\n    ",
    ];

    for prompt in &prompts {
        eprintln!("\n--- Prompt: {:?} ---", &prompt[..prompt.len().min(40)]);
        let start = Instant::now();
        match continuum_core::inference::backends::generate(
            &mut backend,
            prompt,
            max_tokens,
            &continuum_core::inference::backends::SamplingConfig::code(),
        ) {
            Ok((text, n_tokens)) => {
                let elapsed = start.elapsed();
                let tok_s = n_tokens as f64 / elapsed.as_secs_f64();
                eprintln!("Generated {} tokens in {:.1}s ({:.1} tok/s)", n_tokens, elapsed.as_secs_f64(), tok_s);
                println!("{prompt}{text}");
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
            }
        }
    }

    eprintln!("\n=== Done ===");
}
