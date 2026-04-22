//! Standalone inference test — same binary, same model, same prompt.
//! Run on both Metal (local) and CPU (local or RunPod) for A/B comparison.
//!
//! Usage:
//!   inference_test [--cpu]
//!
//! Loads the compacted 14B GGUF, runs the EXACT QAT training prompt,
//! prints raw token output. No server, no daemon, no ToolGroupRegistry.

use std::path::PathBuf;
use std::time::Instant;

use candle_core::Device;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let force_cpu = args.iter().any(|a| a == "--cpu");

    let device = if force_cpu {
        eprintln!("Device: CPU (forced)");
        Device::Cpu
    } else {
        #[cfg(feature = "metal")]
        {
            if let Ok(d) = Device::new_metal(0) {
                eprintln!("Device: Metal");
                d
            } else {
                eprintln!("Device: CPU (Metal unavailable)");
                Device::Cpu
            }
        }
        #[cfg(not(feature = "metal"))]
        {
            #[cfg(feature = "cuda")]
            {
                if let Ok(d) = Device::new_cuda(0) {
                    eprintln!("Device: CUDA");
                    d
                } else {
                    eprintln!("Device: CPU");
                    Device::Cpu
                }
            }
            #[cfg(not(feature = "cuda"))]
            {
                eprintln!("Device: CPU");
                Device::Cpu
            }
        }
    };

    // Find model — same search as candle_adapter
    let model_dir = find_model_dir().expect("Model not found");
    eprintln!("Model dir: {:?}", model_dir);

    let gguf_path = find_gguf(&model_dir).expect("No GGUF in model dir");
    eprintln!("GGUF: {:?}", gguf_path);

    let tokenizer_path = model_dir.join("tokenizer.json");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("tokenizer");

    // Load model
    let load_start = Instant::now();
    let mut backend = continuum_core::inference::backends::load_gguf_backend(
        &gguf_path,
        tokenizer.clone(),
        "qwen14b-test",
        &device,
    )
    .expect("load model");
    device.synchronize().ok();
    eprintln!(
        "Model loaded in {:.1}s (ctx={})",
        load_start.elapsed().as_secs_f32(),
        backend.context_length()
    );

    // Read prompt from PROMPT env var, or PROMPT_FILE, or use default
    let prompt = if let Ok(p) = std::env::var("PROMPT") {
        format!("<|im_start|>user\n{p}<|im_end|>\n<|im_start|>assistant\n")
    } else if let Ok(f) = std::env::var("PROMPT_FILE") {
        std::fs::read_to_string(&f).expect(&format!("Failed to read {f}"))
    } else {
        let system = "You are a coding agent. Use <tool_use> XML to call tools.";
        let user_msg = "Create hello.py with a Flask hello world app";
        format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user_msg}<|im_end|>\n<|im_start|>assistant\n")
    };

    eprintln!("\n=== Prompt ({} chars) ===", prompt.len());

    // Minimal test: prefill only, dump top-10 logits. No full generation.
    let max_tokens = std::env::var("MAX_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let sampling = continuum_core::inference::backends::SamplingConfig::code();
    eprintln!("Sampling: {:?}", sampling);

    let (output, token_count) = continuum_core::inference::backends::generate(
        backend.as_mut(),
        &prompt,
        max_tokens,
        &sampling,
    )
    .expect("generate");

    eprintln!("\n=== Output ({} tokens) ===", token_count);
    println!("{}", output);
    eprintln!("\n=== Done ===");
}

fn find_model_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let internal = PathBuf::from(&home).join(".continuum/genome/models/qwen14b-compacted-v1");
    if internal.exists() {
        return Some(internal);
    }
    let external = std::env::var("CONTINUUM_STORAGE_PATH")
        .ok()
        .map(|p| PathBuf::from(p).join("genome/models/qwen14b-compacted-v1"));
    external.filter(|p| p.exists())
}

fn find_gguf(dir: &PathBuf) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("gguf"))
}
