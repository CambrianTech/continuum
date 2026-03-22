//! Diagnostic: dump per-token logits during prefill to compare Candle vs PyTorch.
//!
//! Outputs JSON lines: {"pos": N, "top5": [[token_id, logit], ...], "eos_logit": F}
//! Run the same prompt through PyTorch on RunPod with matching diagnostic to find divergence.

use std::path::Path;
use std::time::Instant;

fn main() {
    let home = std::env::var("HOME").unwrap_or("/tmp".into());
    let default_dir = format!("{home}/.continuum/genome/models/qwen14b-compacted-v1");
    let model_dir = std::env::args()
        .skip_while(|a| a != "--model-dir")
        .nth(1)
        .unwrap_or(default_dir);

    let device = {
        #[cfg(feature = "metal")]
        { candle_core::Device::new_metal(0).expect("Metal") }
        #[cfg(not(feature = "metal"))]
        { candle_core::Device::Cpu }
    };

    // Find GGUF + tokenizer
    let gguf_path = std::fs::read_dir(&model_dir)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map(|e| e == "gguf").unwrap_or(false))
        .expect("no GGUF found");
    let tokenizer_path = Path::new(&model_dir).join("tokenizer.json");

    eprintln!("Loading model from {:?}...", gguf_path);
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).expect("tokenizer");
    let mut backend = continuum_core::inference::backends::load_gguf_backend(
        &gguf_path, tokenizer.clone(), "qwen14b-diag", &device,
    ).expect("load");
    device.synchronize().ok();
    eprintln!("Model loaded.");

    // EXACT same system prompt from QAT v2 training
    let sys = "<|im_start|>system\nYou are a coding agent working in a project directory. You have these tools:\n\n- code/write: Create a NEW file. Params: {filePath: string, content: string}\n- code/read: Read an existing file. Params: {filePath: string}\n- code/edit: Modify an existing file. Params: {filePath: string, oldString: string, newString: string}\n- code/shell/execute: Run a shell command. Params: {command: string}\n- code/tree: List directory structure. Params: {path: string}\n- code/search: Search for text in files. Params: {query: string, path: string}\n\nUse <tool_use> XML format to call tools. Always use code/write for NEW files, code/edit for MODIFYING existing files, code/read before editing.<|im_end|>";

    let prompt = format!(
        "{sys}\n<|im_start|>user\nCreate hello.py with a Flask hello world app<|im_end|>\n<|im_start|>assistant\n"
    );

    let encoding = tokenizer.encode(prompt.as_str(), false).expect("encode");
    let tokens = encoding.get_ids();
    eprintln!("Prompt: {} tokens", tokens.len());

    // Clear cache
    backend.clear_cache().expect("clear");

    // Prefill token by token, logging top-5 logits at key positions
    let start = Instant::now();
    let check_positions: Vec<usize> = {
        let mut v: Vec<usize> = (0..5).collect();  // first 5
        v.extend((tokens.len().saturating_sub(5))..tokens.len()); // last 5
        // Also every 50th
        for i in (50..tokens.len()).step_by(50) {
            v.push(i);
        }
        v.sort();
        v.dedup();
        v
    };

    eprintln!("Checking positions: {:?}", check_positions);
    eprintln!("--- BEGIN DIAGNOSTICS ---");

    let mut last_prefill_logits: Option<candle_core::Tensor> = None;

    for (pos, &token) in tokens.iter().enumerate() {
        let input = candle_core::Tensor::new(&[token], &device)
            .expect("tensor")
            .unsqueeze(0)
            .expect("unsqueeze");

        let logits = backend.forward(&input, pos).expect("forward");
        last_prefill_logits = Some(logits.clone());

        if check_positions.contains(&pos) {
            // Extract logits for this position
            let logits_1d = logits.squeeze(0).expect("squeeze batch");
            let logits_1d = if logits_1d.dims().len() > 1 {
                let seq_len = logits_1d.dim(0).unwrap();
                logits_1d.get(seq_len - 1).expect("last seq pos")
            } else {
                logits_1d
            };

            let logits_vec: Vec<f32> = logits_1d.to_vec1().expect("to_vec1");

            // Top 5 tokens by logit value
            let mut indexed: Vec<(usize, f32)> = logits_vec.iter().cloned().enumerate().collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let top5: Vec<(u32, f32)> = indexed.iter().take(5).map(|&(i, v)| (i as u32, v)).collect();

            // Decode current token and top predictions
            let current_decoded = tokenizer.decode(&[token], false).unwrap_or_default();
            let top_decoded: Vec<String> = top5.iter()
                .map(|(tid, logit)| {
                    let d = tokenizer.decode(&[*tid], false).unwrap_or("?".into());
                    format!("{}:{:.2}:{}", tid, logit, d.replace('\n', "\\n").replace('"', "'"))
                })
                .collect();

            // Special tokens
            let eos_logit = logits_vec.get(151645).copied().unwrap_or(f32::NAN);  // <|im_end|>
            let eot_logit = logits_vec.get(151643).copied().unwrap_or(f32::NAN);  // <|endoftext|>

            eprintln!(
                "pos={:>4} token={:>6}({:>15}) | top5=[{}] | eos={:.2} eot={:.2}",
                pos, token, &current_decoded[..current_decoded.len().min(15)],
                top_decoded.join(", "),
                eos_logit, eot_logit,
            );
        }

        if (pos + 1) % 64 == 0 {
            device.synchronize().ok();
        }
    }

    let prefill_time = start.elapsed();
    eprintln!("--- PREFILL DONE in {:.1}s ---", prefill_time.as_secs_f64());

    // Generate 20 tokens.
    //
    // IMPORTANT: gen[0] is sampled from the prefill logits (already computed above).
    // We do NOT call forward() again for gen[0] — that would duplicate the last
    // prefill step and corrupt the KV cache.  Generation starts at pos=prompt_len.
    eprintln!("--- GENERATION ---");
    let prompt_len = tokens.len();
    let mut all_tokens: Vec<u32> = tokens.to_vec();

    // Helper: greedy argmax with special token suppression
    let greedy_suppress = |logits_vec: &Vec<f32>| -> (u32, f32) {
        let mut best_id = 0u32;
        let mut best_val = f32::NEG_INFINITY;
        for (idx, &val) in logits_vec.iter().enumerate() {
            if idx == 151643 || idx == 151644 { continue; } // suppress <|endoftext|>, <|im_start|>
            if val > best_val {
                best_val = val;
                best_id = idx as u32;
            }
        }
        (best_id, best_val)
    };

    // gen[0]: sample from prefill logits (no extra forward call).
    let last_logits = last_prefill_logits.expect("prefill produced no logits");
    {
        let logits_1d_ref = if last_logits.dims().len() > 1 {
            let sl = last_logits.dim(0).unwrap();
            last_logits.get(sl - 1).expect("last seq pos for gen[0]")
        } else {
            last_logits.clone()
        };
        let logits_vec: Vec<f32> = logits_1d_ref.to_vec1().expect("vec");
        let (best_id, best_val) = greedy_suppress(&logits_vec);
        let eos_logit = logits_vec.get(151645).copied().unwrap_or(f32::NAN);
        let decoded = tokenizer.decode(&[best_id], false).unwrap_or("?".into());

        eprintln!(
            "gen[{:>2}] pos={:>4} token={:>6}({:>15}) logit={:.2} eos={:.2}  [from prefill]",
            0, prompt_len - 1, best_id, &decoded[..decoded.len().min(15)], best_val, eos_logit
        );

        if best_id == 151645 {
            eprintln!("  → EOS hit");
        } else {
            all_tokens.push(best_id);
        }
    }

    // gen[1..20]: forward(gen[i-1], pos=prompt_len + i - 1)
    for i in 1..20 {
        if all_tokens.len() <= prompt_len {
            break; // EOS was hit on gen[0]
        }
        let token = *all_tokens.last().unwrap();
        let pos = all_tokens.len() - 1; // = prompt_len + i - 1
        let input = candle_core::Tensor::new(&[token], &device)
            .expect("tensor")
            .unsqueeze(0)
            .expect("unsqueeze");

        let logits = backend.forward(&input, pos).expect("forward");
        let logits_1d = logits.squeeze(0).expect("squeeze");
        let logits_1d = if logits_1d.dims().len() > 1 {
            let sl = logits_1d.dim(0).unwrap();
            logits_1d.get(sl - 1).expect("last")
        } else {
            logits_1d
        };
        let logits_vec: Vec<f32> = logits_1d.to_vec1().expect("vec");
        let (best_id, best_val) = greedy_suppress(&logits_vec);
        let eos_logit = logits_vec.get(151645).copied().unwrap_or(f32::NAN);
        let decoded = tokenizer.decode(&[best_id], false).unwrap_or("?".into());

        eprintln!(
            "gen[{:>2}] pos={:>4} token={:>6}({:>15}) logit={:.2} eos={:.2}",
            i, pos, best_id, &decoded[..decoded.len().min(15)], best_val, eos_logit
        );

        if best_id == 151645 {
            eprintln!("  → EOS hit");
            break;
        }
        all_tokens.push(best_id);
    }

    // Print generated text
    let gen_tokens = &all_tokens[prompt_len..];
    let generated = tokenizer.decode(gen_tokens, false).unwrap_or_default();
    eprintln!("\n--- GENERATED TEXT ---\n{}", generated);
}
