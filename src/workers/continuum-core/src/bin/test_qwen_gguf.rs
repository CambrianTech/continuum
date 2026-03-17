//! Benchmark harness for compacted Qwen2 GGUF inference.
//! Measures speed, quality, and memory for quantization comparisons.
//!
//! Usage: test_qwen_gguf --metal [--model-dir PATH] [--json]
//!
//! Env: MAX_TOKENS (default 128), CANDLE_METAL_COMMAND_POOL_SIZE

use std::path::Path;
use std::time::Instant;

fn mem_mb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        extern "C" {
            fn mach_task_self() -> u32;
            fn task_info(t: u32, f: u32, o: *mut u8, c: *mut u32) -> i32;
        }
        #[repr(C)]
        struct Info { virt: u64, _r: [u64; 2], rss: u64, pad: [u64; 30] }
        let mut info: Info = unsafe { mem::zeroed() };
        let mut count = (mem::size_of::<Info>() / 4) as u32;
        if unsafe { task_info(mach_task_self(), 22, &mut info as *mut _ as *mut u8, &mut count) } == 0 {
            return info.rss as f64 / 1048576.0;
        }
    }
    0.0
}

/// Coding prompts for quality evaluation
const PROMPTS: &[(&str, &str)] = &[
    ("is_prime", "def is_prime(n):\n    "),
    ("fibonacci", "def fibonacci(n):\n    "),
    ("binary_search", "def binary_search(arr, target):\n    "),
    ("reverse_string", "def reverse_string(s):\n    "),
    ("flatten_list", "def flatten(lst):\n    "),
];

fn main() {
    let use_metal = std::env::args().any(|a| a == "--metal");
    let json_output = std::env::args().any(|a| a == "--json");
    let max_tokens: usize = std::env::var("MAX_TOKENS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(128);

    let home = std::env::var("HOME").unwrap_or("/Users/joel".into());
    let default_dir = format!("{home}/.continuum/genome/models/qwen32b-compacted-v3");
    let model_dir = std::env::args()
        .skip_while(|a| a != "--model-dir")
        .nth(1)
        .unwrap_or(default_dir);

    let device = if use_metal {
        #[cfg(feature = "metal")]
        { candle_core::Device::new_metal(0).expect("Metal") }
        #[cfg(not(feature = "metal"))]
        { panic!("no metal") }
    } else {
        candle_core::Device::Cpu
    };

    // --- Model info ---
    let gguf_path = std::fs::read_dir(&model_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "gguf").unwrap_or(false))
        .expect("no GGUF found");
    let model_size_gb = std::fs::metadata(&gguf_path)
        .map(|m| m.len() as f64 / 1073741824.0)
        .unwrap_or(0.0);
    let tokenizer_path = Path::new(&model_dir).join("tokenizer.json");
    let model_name = gguf_path.file_stem().unwrap().to_string_lossy().to_string();

    if !json_output {
        eprintln!("=== Compacted Qwen2 32B Benchmark ===");
        eprintln!("  Model: {} ({:.1} GB)", model_name, model_size_gb);
        eprintln!("  Device: {:?}", device);
        eprintln!("  Max tokens: {}", max_tokens);
    }

    // --- Load ---
    let rss_before = mem_mb();
    let load_start = Instant::now();
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("tokenizer");
    let mut backend = continuum_core::inference::backends::load_gguf_backend(
        &gguf_path, tokenizer, &model_name, &device,
    ).expect("load");
    device.synchronize().ok();
    let load_time = load_start.elapsed();
    let rss_after_load = mem_mb();

    if !json_output {
        eprintln!("  Load: {:.1?}, RSS: {:.0} MB → {:.0} MB", load_time, rss_before, rss_after_load);
        eprintln!("  Context: {}, Arch: {}\n", backend.context_length(), backend.architecture());
    }

    // --- Benchmark each prompt ---
    let mut results = Vec::new();

    for (name, prompt) in PROMPTS {
        if !json_output {
            eprint!("  [{:>15}] ", name);
        }

        let gen_start = Instant::now();
        let result = continuum_core::inference::backends::generate(
            backend.as_mut(), prompt, max_tokens, 0.1,
        );
        let gen_time = gen_start.elapsed();
        let rss_after = mem_mb();

        match result {
            Ok((output, token_count)) => {
                let tok_per_sec = if gen_time.as_secs_f64() > 0.0 {
                    token_count as f64 / gen_time.as_secs_f64()
                } else { 0.0 };

                // Quality heuristics
                let has_return = output.contains("return ");
                let has_def = output.contains("def ") || output.starts_with("\"\"\"") || output.starts_with("if ");
                let lines: Vec<&str> = output.lines().collect();
                let unique_lines: std::collections::HashSet<&str> = lines.iter().copied().collect();
                let repetition_ratio = if lines.len() > 0 {
                    1.0 - (unique_lines.len() as f64 / lines.len() as f64)
                } else { 0.0 };

                if !json_output {
                    eprintln!("{} tok, {:.1} tok/s, rep={:.0}%, RSS={:.0}MB",
                        token_count, tok_per_sec, repetition_ratio * 100.0, rss_after);
                }

                results.push(serde_json::json!({
                    "prompt_name": name,
                    "tokens": token_count,
                    "time_s": gen_time.as_secs_f64(),
                    "tok_per_sec": tok_per_sec,
                    "has_return": has_return,
                    "has_structure": has_def,
                    "repetition_ratio": repetition_ratio,
                    "output": output,
                    "rss_mb": rss_after,
                }));
            }
            Err(e) => {
                if !json_output {
                    eprintln!("FAILED: {}", e);
                }
                results.push(serde_json::json!({
                    "prompt_name": name,
                    "error": e,
                }));
            }
        }
    }

    // --- Summary ---
    let successful: Vec<_> = results.iter()
        .filter(|r| r.get("tok_per_sec").is_some())
        .collect();
    let avg_tok_s = successful.iter()
        .filter_map(|r| r["tok_per_sec"].as_f64())
        .sum::<f64>() / successful.len().max(1) as f64;
    let avg_rep = successful.iter()
        .filter_map(|r| r["repetition_ratio"].as_f64())
        .sum::<f64>() / successful.len().max(1) as f64;
    let quality_score = successful.iter()
        .filter(|r| r["has_return"].as_bool().unwrap_or(false) && r["has_structure"].as_bool().unwrap_or(false))
        .count() as f64 / PROMPTS.len() as f64;

    let summary = serde_json::json!({
        "model": model_name,
        "model_size_gb": model_size_gb,
        "load_time_s": load_time.as_secs_f64(),
        "rss_after_load_mb": rss_after_load,
        "max_tokens": max_tokens,
        "prompts_total": PROMPTS.len(),
        "prompts_succeeded": successful.len(),
        "avg_tok_per_sec": avg_tok_s,
        "avg_repetition_ratio": avg_rep,
        "quality_score": quality_score,
        "results": results,
    });

    if json_output {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    } else {
        eprintln!("\n=== Summary ===");
        eprintln!("  Model: {} ({:.1} GB)", model_name, model_size_gb);
        eprintln!("  Speed: {:.1} tok/s avg", avg_tok_s);
        eprintln!("  Quality: {:.0}% structured, {:.0}% repetition", quality_score * 100.0, avg_rep * 100.0);
        eprintln!("  Memory: {:.0} MB RSS after load", rss_after_load);
    }
}
