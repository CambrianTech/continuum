//! Test Qwen2 32B compacted GGUF inference.
//! Usage: test_qwen_gguf --metal [--stage N] [--model-dir PATH]

use std::path::Path;

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

fn r(label: &str) { eprintln!("  [{:>40}] RSS = {:.0} MB", label, mem_mb()); }

fn main() {
    let use_metal = std::env::args().any(|a| a == "--metal");
    let stage: usize = std::env::args()
        .skip_while(|a| a != "--stage")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2); // Default: full forward

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

    eprintln!("=== Qwen2 32B Compacted GGUF Test (stage {}) ===", stage);
    eprintln!("  Model: {}", model_dir);
    r("start");

    let gguf_path = std::fs::read_dir(&model_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "gguf").unwrap_or(false))
        .expect("no GGUF found");
    let tokenizer_path = Path::new(&model_dir).join("tokenizer.json");

    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("tokenizer");
    let mut backend = continuum_core::inference::backends::load_gguf_backend(
        &gguf_path, tokenizer, "qwen32b-compacted", &device,
    ).expect("load");
    device.synchronize().ok();
    eprintln!("  arch={}, ctx={}", backend.architecture(), backend.context_length());
    r("loaded + sync");

    if stage == 0 { return; }

    // Stage 1+: generate
    let prompt = "def is_prime(n):\n    ";
    let max_tokens = std::env::var("MAX_TOKENS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(32);

    eprintln!("\n  Generating from: {:?} (max {} tokens)", prompt, max_tokens);
    let start = std::time::Instant::now();
    match continuum_core::inference::backends::generate(backend.as_mut(), prompt, max_tokens, 0.1) {
        Ok((output, count)) => {
            let elapsed = start.elapsed();
            r("generation done");
            eprintln!("\n--- {} tokens in {:.1?} ({:.1} tok/s) ---", count, elapsed,
                count as f64 / elapsed.as_secs_f64());
            eprintln!("{}", output);
            eprintln!("---");
        }
        Err(e) => {
            r("generation FAILED");
            eprintln!("  Error: {}", e);
        }
    }
}
