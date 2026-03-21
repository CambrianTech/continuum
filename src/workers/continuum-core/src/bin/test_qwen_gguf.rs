//! Real-world coding test for compacted Qwen2 32B.
//! Tests with proper chat template, system prompt, and stop conditions.

use std::path::Path;
use std::time::Instant;

fn main() {
    let home = std::env::var("HOME").unwrap_or("/tmp".into());
    let default_dir = format!("{home}/.continuum/genome/models/qwen32b-compacted-v3");
    let model_dir = std::env::args()
        .skip_while(|a| a != "--model-dir")
        .nth(1)
        .unwrap_or(default_dir);
    let max_tokens: usize = std::env::var("MAX_TOKENS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(512);

    let device = {
        #[cfg(feature = "metal")]
        { candle_core::Device::new_metal(0).expect("Metal") }
        #[cfg(not(feature = "metal"))]
        { candle_core::Device::Cpu }
    };

    let gguf_path = std::fs::read_dir(&model_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "gguf").unwrap_or(false))
        .expect("no GGUF found");
    let tokenizer_path = Path::new(&model_dir).join("tokenizer.json");

    eprintln!("Loading model...");
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("tokenizer");
    let mut backend = continuum_core::inference::backends::load_gguf_backend(
        &gguf_path, tokenizer, "qwen32b-compacted", &device,
    ).expect("load");
    device.synchronize().ok();
    eprintln!("Model loaded. Generating...\n");

    // Same prompts as RunPod PyTorch test — exact same strings for comparison
    let prompts = vec![
        (
            "flask_app",
            "<|im_start|>system\nYou are an expert Python developer. Write clean, working code. Only output the code.<|im_end|>\n<|im_start|>user\nCreate a Flask web app with a homepage, /about page, and /api/status JSON endpoint showing uptime.<|im_end|>\n<|im_start|>assistant\n"
        ),
        (
            "bare_flask",
            "from flask import Flask, jsonify\nimport time\n\napp = Flask(__name__)\nstart_time = time.time()\n\n"
        ),
    ];

    let sampling = continuum_core::inference::backends::SamplingConfig::code();
    for (name, prompt) in &prompts {
        eprintln!("=== {} ===", name);
        let start = Instant::now();
        match continuum_core::inference::backends::generate(
            backend.as_mut(), prompt, max_tokens, &sampling,
        ) {
            Ok((output, count)) => {
                let elapsed = start.elapsed();
                let tok_s = count as f64 / elapsed.as_secs_f64();

                // Trim at EOS or repetition
                let clean = trim_output(&output);

                eprintln!("{}", clean);
                eprintln!("\n--- {} tokens, {:.1} tok/s, {:.1?} ---\n", count, tok_s, elapsed);
            }
            Err(e) => eprintln!("ERROR: {}\n", e),
        }
    }
}

/// Trim output at first EOS token or obvious repetition.
fn trim_output(text: &str) -> &str {
    // Stop at <|im_end|>
    if let Some(pos) = text.find("<|im_end|>") {
        return &text[..pos];
    }
    // Stop at obvious repetition (3+ identical lines)
    let lines: Vec<&str> = text.lines().collect();
    for i in 3..lines.len() {
        if lines[i] == lines[i-1] && lines[i] == lines[i-2] {
            let byte_pos: usize = lines[..i-2].iter().map(|l| l.len() + 1).sum();
            return &text[..byte_pos.min(text.len())];
        }
    }
    text
}
