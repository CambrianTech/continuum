//! Real-world coding test for compacted Qwen2 32B.
//! Tests with proper chat template, system prompt, and stop conditions.

use std::path::Path;
use std::time::Instant;

fn main() {
    let home = std::env::var("HOME").unwrap_or("/Users/joel".into());
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

    // Real prompts with Qwen2 chat template
    let prompts = vec![
        (
            "flask_app",
            "<|im_start|>system\nYou are an expert Python developer. Write clean, working, production-ready code. Only output the code, no explanations.<|im_end|>\n<|im_start|>user\nCreate a Flask web app with three routes: a homepage that shows a welcome message, an /about page, and a /api/status endpoint that returns JSON with the server uptime. Include proper error handling.<|im_end|>\n<|im_start|>assistant\n"
        ),
        (
            "swift_todo",
            "<|im_start|>system\nYou are an expert Swift/iOS developer. Write clean SwiftUI code. Only output the code, no explanations.<|im_end|>\n<|im_start|>user\nCreate a SwiftUI TodoList app with: a list of todo items, ability to add new items via a text field, ability to toggle completion with a checkmark, and ability to delete items with swipe. Use @State for the data model.<|im_end|>\n<|im_start|>assistant\n"
        ),
        (
            "react_counter",
            "<|im_start|>system\nYou are an expert React/TypeScript developer. Write clean, working code. Only output the code, no explanations.<|im_end|>\n<|im_start|>user\nCreate a React component in TypeScript that implements a counter with increment, decrement, and reset buttons. Style it with inline CSS. Include a history of the last 10 values displayed as a list.<|im_end|>\n<|im_start|>assistant\n"
        ),
    ];

    for (name, prompt) in &prompts {
        eprintln!("=== {} ===", name);
        let start = Instant::now();
        match continuum_core::inference::backends::generate(
            backend.as_mut(), prompt, max_tokens, 0.1,
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
