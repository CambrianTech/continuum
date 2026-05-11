//! No-inference token-level diagnostic for the persona prompt path.
//!
//! Loads the model's tokenizer (no KV alloc, no Metal pipeline compilation
//! beyond device init), renders a prod-shape chat prompt via the same
//! `llama_chat_apply_template` path the live system uses, tokenizes with
//! both `add_bos=true` and `add_bos=false`, and asserts on the resulting
//! token sequences.
//!
//! Why this exists: the persona render path was emitting `<|endoftext|>`
//! after one or two tokens on prod-shape input (verified 2026-04-21 via
//! the scheduler-level diagnostic that's since been removed). The
//! suspected cause was `add_bos=true` injecting the GGUF's wrong-BOS
//! token (qwen3.5-4b-code-forged declares BOS=11 = ',') at the start
//! of the rendered chatml prompt, confusing the model into immediate EOG.
//!
//! This test confirms or refutes that hypothesis WITHOUT running
//! inference, allocating KV, or risking OOM. ~50ms per run vs. minutes
//! for the full integration test.
//!
//! Run:
//!   cargo test --release --test persona_prompt_token_diagnostic -- --ignored --nocapture

use llama::{render_chat, ChatMsg, Model, ModelParams};
use std::path::PathBuf;

mod common;

fn model_path() -> std::path::PathBuf {
    common::qwen35_4b_code_gguf().expect(
        "qwen3.5-4b-code-forged GGUF not resolvable via DMR;          is Docker Desktop running with Model Runner enabled?",
    )
}

const CHATML_TEMPLATE: &str = "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\n' + message['content'] + '<|im_end|>\n' }}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\n' }}{% endif %}";

/// Token IDs that matter for the assertions below. From the GGUF metadata
/// dump in tests/qwen35_chat_pipeline_full.rs run output:
///   BOS = 11 ',' (the WRONG default — it's the comma character, not a real special token)
///   <|im_start|> = 248045
///   <|im_end|>   = 248046
///   <|endoftext|>= 248044
const BOS_COMMA_TOKEN: i32 = 11;
const IM_START_TOKEN: i32 = 248045;
const IM_END_TOKEN: i32 = 248046;
const ENDOFTEXT_TOKEN: i32 = 248044;

fn load_tokenizer_only() -> Model {
    // n_gpu_layers = 0 keeps weights on CPU only and avoids Metal pipeline
    // compilation. Tokenizer lives on the model object regardless of
    // device, so we get full tokenization without paying GPU init cost.
    let path = model_path();
    assert!(
        path.exists(),
        "Model GGUF not present at {}. \
         Pull continuum-ai/qwen3.5-4b-code-forged-gguf via DMR before running this test.",
        path.display()
    );
    Model::load(
        &path,
        ModelParams {
            n_gpu_layers: 0,
            use_mmap: true,
        },
    )
    .expect("Model::load")
}

fn render_minimal_chat() -> String {
    let messages = vec![
        ChatMsg {
            role: "system".to_string(),
            content: "You are Helper AI. Respond concisely.".to_string(),
        },
        ChatMsg {
            role: "user".to_string(),
            content: "Hi everyone.".to_string(),
        },
    ];
    render_chat(Some(CHATML_TEMPLATE), &messages, true).expect("render_chat")
}

fn dump_first_n_tokens(label: &str, model: &Model, tokens: &[i32], n: usize) {
    eprintln!(
        "[{label}] {} tokens; first {} (id, piece):",
        tokens.len(),
        n.min(tokens.len())
    );
    for (i, &tok) in tokens.iter().take(n).enumerate() {
        let piece = model.token_to_piece(tok);
        eprintln!("  [{i:>2}] id={tok:>6} piece={piece:?}");
    }
}

// ─── Test 1: Refutes the wrong-BOS hypothesis (kept as guard) ────────────

/// What this catches: a future regression where someone "fixes" the
/// scheduler by setting `add_bos=false`, breaking the (already correct)
/// behavior. llama.cpp's `llama_tokenize` is smart enough NOT to inject
/// the GGUF's declared BOS when the rendered prompt already starts with
/// a special structural token (chatml `<|im_start|>` in our case). So
/// `add_bos=true` and `add_bos=false` produce IDENTICAL output for
/// chatml-rendered prompts.
///
/// Validated 2026-04-21 (TDD/VDD): wrote this test expecting the
/// asymmetry to confirm "add_bos=true injects comma." Test FAILED
/// because BOTH variants produced position-0 = id 248045 (`<|im_start|>`).
/// Hypothesis ruled out without running inference. Test now asserts the
/// ACTUAL behavior (identical output) so any future change that breaks
/// this invariant gets caught.
///
/// The bug we were chasing is downstream of tokenization — sampler,
/// scheduler, or model behavior on the specific prompt content.
#[test]
#[ignore = "requires local GGUF; cargo test --release --test persona_prompt_token_diagnostic -- --ignored --nocapture"]
fn chatml_prompt_tokenization_is_invariant_to_add_bos_flag() {
    let model = load_tokenizer_only();
    let prompt = render_minimal_chat();

    let with_bos = model
        .tokenize(&prompt, true, true)
        .expect("tokenize add_bos=true");
    let without_bos = model
        .tokenize(&prompt, false, true)
        .expect("tokenize add_bos=false");

    dump_first_n_tokens("add_bos=true ", &model, &with_bos, 8);
    dump_first_n_tokens("add_bos=false", &model, &without_bos, 8);

    assert_eq!(
        with_bos[0], IM_START_TOKEN,
        "add_bos=true should NOT inject wrong-BOS — chatml prompt already \
         starts with <|im_start|>, llama.cpp is smart enough to skip BOS"
    );
    assert_eq!(
        without_bos[0], IM_START_TOKEN,
        "add_bos=false also produces <|im_start|> at position 0 (same prompt)"
    );
    assert_eq!(
        with_bos, without_bos,
        "for chatml-rendered prompts, add_bos flag is functionally a no-op — \
         identical token sequences. If this changes, llama.cpp behavior shifted."
    );

    // Sanity-check: the wrong-BOS comma (id=11) should NOT appear anywhere
    // in the tokenized output. If it does, llama.cpp injected it somewhere.
    assert!(
        !with_bos.contains(&BOS_COMMA_TOKEN),
        "wrong-BOS comma (id={BOS_COMMA_TOKEN}) should not appear in chatml tokenized output"
    );
}

// ─── Test 2: Verify special tokens render correctly ──────────────────────

/// What this catches: chat-template boundary tokens (`<|im_start|>`,
/// `<|im_end|>`) MUST tokenize to their actual special token IDs
/// (248045, 248046), NOT to character-level pieces. If `special=false`
/// in the tokenize call, these become individual character tokens and
/// the model never sees the structural boundaries it was trained on,
/// producing garbage.
///
/// Validated 2026-04-21: with `special=true`, `<|im_start|>` appears
/// as a single token id 248045. With `special=false`, the same string
/// becomes ~9 character-level tokens (`<`, `|`, `i`, `m`, `_`, `s`, ...).
/// This test catches anyone "fixing" a tokenization bug by setting
/// `special=false` — which would silently break chat-template rendering.
#[test]
#[ignore = "requires local GGUF; cargo test --release --test persona_prompt_token_diagnostic -- --ignored --nocapture"]
fn special_tokens_render_as_single_ids_when_special_flag_true() {
    let model = load_tokenizer_only();
    let prompt = render_minimal_chat();

    let with_special = model
        .tokenize(&prompt, false, true)
        .expect("tokenize special=true");
    let without_special = model
        .tokenize(&prompt, false, false)
        .expect("tokenize special=false");

    dump_first_n_tokens("special=true ", &model, &with_special, 8);
    dump_first_n_tokens("special=false", &model, &without_special, 12);

    // With special=true, position 0 is the chatml im_start token (one
    // single id). Without special, it's a sequence of char tokens.
    assert_eq!(
        with_special[0], IM_START_TOKEN,
        "special=true should tokenize <|im_start|> as the single special token id"
    );
    assert_ne!(
        without_special[0], IM_START_TOKEN,
        "special=false should NOT recognize the special token; first byte is what shows up"
    );

    // special=false produces strictly more tokens (because each special
    // string fragments into multiple character tokens).
    assert!(
        without_special.len() > with_special.len(),
        "special=false should produce more tokens than special=true (chars > single special)"
    );
}

// ─── Test 3: Render shape proof — what exactly is the model receiving ────

/// What this catches: ensures the chatml template renders a multi-message
/// chat with the expected structural shape. Specifically, position 0 should
/// be `<|im_start|>`, the system role + content should follow, then
/// `<|im_end|>`, then another `<|im_start|>` for user, etc. Any drift in
/// the template (or in our llama_chat_apply_template wrapper) shows up as
/// the wrong special token in the wrong position.
///
/// Validated 2026-04-21: the chatml template produces exactly:
///   [<|im_start|>, "system", \n, ..., <|im_end|>, \n, <|im_start|>, ...]
/// with the special tokens at the structural positions. Regression in
/// either the template string or the C++ template renderer would change
/// this layout.
#[test]
#[ignore = "requires local GGUF; cargo test --release --test persona_prompt_token_diagnostic -- --ignored --nocapture"]
fn chatml_template_emits_im_start_im_end_at_structural_boundaries() {
    let model = load_tokenizer_only();
    let prompt = render_minimal_chat();
    eprintln!("[chatml render] prompt:\n{prompt}\n---END---");

    let tokens = model.tokenize(&prompt, false, true).expect("tokenize");
    dump_first_n_tokens("chatml shape", &model, &tokens, 30);

    // Count occurrences — minimal chat is system + user + assistant
    // generation prompt = 3 <|im_start|> + 2 <|im_end|>.
    let im_start_count = tokens.iter().filter(|&&t| t == IM_START_TOKEN).count();
    let im_end_count = tokens.iter().filter(|&&t| t == IM_END_TOKEN).count();

    assert_eq!(
        im_start_count, 3,
        "minimal chat (system + user + assistant prompt) should have exactly 3 <|im_start|> tokens; got {im_start_count}"
    );
    assert_eq!(
        im_end_count, 2,
        "minimal chat (system + user) should have exactly 2 <|im_end|> tokens (assistant turn isn't closed); got {im_end_count}"
    );

    // No <|endoftext|> (token 248044) should appear in our prompt — that's
    // an EOG token, the model is supposed to OUTPUT it, not see it in input.
    let endoftext_count = tokens.iter().filter(|&&t| t == ENDOFTEXT_TOKEN).count();
    assert_eq!(
        endoftext_count, 0,
        "<|endoftext|> should NEVER appear in input tokens; got {endoftext_count}"
    );
}
