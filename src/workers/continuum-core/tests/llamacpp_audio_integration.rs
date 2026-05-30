//! End-to-end native audio integration test against real Qwen2-Audio-7B.
//!
//! Symmetric to `llamacpp_vision_integration.rs`. The vision side proved
//! the libmtmd path against Qwen2-VL-7B + image bytes 2026-04-21 ("BAD
//! MOTHER FUCKER" wallet OCR landed verbatim, confirming raw bytes
//! reached the encoder). This test does the same thing for the audio
//! modality: real Qwen2-Audio-7B GGUF + audio mmproj + a real wav,
//! exercised through `LlamaCppBackend::generate_with_audio` →
//! `MtmdContext::eval_audio`.
//!
//! Why a separate ignored integration test instead of the full stack:
//! the persona pipeline + Bevy renderer + 5-persona scheduler caused
//! enough Metal contention to wedge the entire WindowServer (mouse-
//! frozen, machine-bricked, hard reset required) when we tried the
//! full e2e path on 2026-04-22. This test isolates the question
//! "does the audio path work end-to-end through Rust?" from the
//! question "is the persona pipeline stable under concurrent
//! multi-modal load?" — the second question is real but separate.
//!
//! Marked `#[ignore]` because it requires the qwen2-audio-7b GGUF +
//! audio mmproj on disk (~5.7 GB) and pays a ~5–10s load cost. Run
//! manually:
//!
//!     cargo test --package continuum-core \
//!       --test llamacpp_audio_integration \
//!       --release -- --ignored --nocapture

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use continuum_core::inference::backends::SamplingConfig;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn qwen2_audio_paths() -> (PathBuf, PathBuf) {
    let model = env::var("QWEN2_AUDIO_7B_GGUF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(
                env::var("HOME").expect("HOME env var must be set for this integration test"),
            )
            .join("models/qwen2-audio-7b/Qwen2-Audio-7B-Instruct-Q4_K_M.gguf")
        });
    let mmproj = env::var("QWEN2_AUDIO_7B_MMPROJ")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(
                env::var("HOME").expect("HOME env var must be set for this integration test"),
            )
            .join("models/qwen2-audio-7b/mmproj-Qwen2-Audio-7B-Instruct-f16.gguf")
        });
    (model, mmproj)
}

/// Test wav loader. Prefers `TEST_AUDIO_WAV` env override; falls back
/// to `/tmp/audio-test-001.wav`. If neither is present, generates a
/// fresh wav via macOS `say` + `afconvert` (16 kHz mono PCM — the
/// canonical input format mtmd-audio expects per upstream
/// `tools/mtmd/mtmd-helper.cpp`'s miniaudio decode path). Generation
/// requires being on a Mac; on other platforms the test skips with a
/// clear message instead of hand-rolling a synthetic wav (synthetic
/// audio doesn't carry enough phonetic signal for the model to
/// transcribe meaningfully — the same lesson VAD-SYNTHETIC-AUDIO-
/// FINDINGS recorded for VAD).
fn load_or_generate_test_wav() -> Option<Vec<u8>> {
    let path = env::var("TEST_AUDIO_WAV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/audio-test-001.wav"));

    if path.exists() {
        return std::fs::read(&path).ok();
    }

    // Generate via `say` (macOS-only).
    if !cfg!(target_os = "macos") {
        eprintln!(
            "[audio-int] no test wav at {} and not on macOS — \
             set TEST_AUDIO_WAV=/path/to/16khz-mono.wav",
            path.display()
        );
        return None;
    }

    let aiff = path.with_extension("aiff");
    let say_text = "Hello, this is a test of the audio understanding model. \
                    Please describe what you hear in this clip.";
    let say_ok = Command::new("say")
        .args(["-o", aiff.to_str()?, say_text])
        .status()
        .ok()
        .map(|s| s.success())
        .unwrap_or(false);
    if !say_ok {
        eprintln!("[audio-int] `say` failed — can't generate test wav");
        return None;
    }
    let convert_ok = Command::new("afconvert")
        .args([
            "-f",
            "WAVE",
            "-d",
            "LEI16@16000",
            "-c",
            "1",
            aiff.to_str()?,
            path.to_str()?,
        ])
        .status()
        .ok()
        .map(|s| s.success())
        .unwrap_or(false);
    if !convert_ok {
        eprintln!("[audio-int] `afconvert` failed — can't convert to wav");
        return None;
    }
    let _ = std::fs::remove_file(&aiff); // clean up the intermediate

    eprintln!(
        "[audio-int] generated test wav at {} ({} bytes)",
        path.display(),
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    );
    std::fs::read(&path).ok()
}

/// What this catches: native audio through `LlamaCppBackend::generate_with_audio`
/// failing to produce a coherent transcription / description of the wav.
/// If this passes, the chain (audio mmproj load → mtmd_helper_eval_chunks
/// with audio bitmap → sampler loop) works end-to-end against a real
/// model. If it fails, the printed output (under `--nocapture`) shows
/// the model's actual response — we look for any signal word from the
/// source utterance ("hello", "test", "audio", "model", "describe",
/// "hear", "clip") rather than pinning an exact string because audio-LLM
/// transcription phrasing varies (some models paraphrase, some quote,
/// some describe the speaker characteristics).
///
/// Sampling uses greedy + low temp like the vision integration test,
/// for the same reason: deterministic output makes the assertion
/// stable across runs.
#[test]
#[ignore = "requires real Qwen2-Audio-7B GGUF + audio mmproj + 5-10s; run manually with --ignored --nocapture"]
fn qwen2_audio_describes_clip_via_rust_pipeline() {
    let (model_path, mmproj_path) = qwen2_audio_paths();
    if !model_path.exists() {
        eprintln!(
            "[audio-int] skipping — Qwen2-Audio-7B GGUF not at {}. \
             Set QWEN2_AUDIO_7B_GGUF or download via \
             `hf download mradermacher/Qwen2-Audio-7B-Instruct-GGUF \
              Qwen2-Audio-7B-Instruct.Q4_K_M.gguf \
              Qwen2-Audio-7B-Instruct.mmproj-f16.gguf \
              --local-dir ~/models/qwen2-audio-7b` (then rename `.` -> `-` \
             to match the path convention).",
            model_path.display()
        );
        return;
    }
    if !mmproj_path.exists() {
        eprintln!(
            "[audio-int] skipping — audio mmproj not at {}. \
             The audio-capable model needs the projector file alongside the main GGUF. \
             Note: bartowski/second-state/gaianet ship weights only — only the mradermacher \
             repo has the audio mmproj at the time of writing.",
            mmproj_path.display()
        );
        return;
    }

    let load_start = Instant::now();
    let config = LlamaCppConfig {
        model_path: model_path.clone(),
        mmproj_path: Some(mmproj_path.clone()),
        context_length: None, // = derive from GGUF (32768 for qwen2-audio-7b)
        n_batch: 2048,
        n_gpu_layers: -1,
        n_seq_max: 1,
        ..Default::default()
    };
    let backend =
        LlamaCppBackend::load(config).expect("backend loads with audio-capable Qwen2-Audio");
    eprintln!(
        "[audio-int] backend loaded in {}ms",
        load_start.elapsed().as_millis()
    );

    let Some(audio) = load_or_generate_test_wav() else {
        eprintln!(
            "[audio-int] skipping — no test wav available. \
             Set TEST_AUDIO_WAV=/path/to/16khz-mono.wav to use a custom clip."
        );
        return;
    };
    eprintln!("[audio-int] audio is {} bytes", audio.len());

    // Apply the model's embedded chat template via llama::render_chat,
    // same machinery the vision test uses. Marker stays the model's
    // declared media marker (`<__media__>` by default); mtmd
    // distinguishes image vs audio at eval-time via the kind enum,
    // not via a different marker token.
    let user_content = format!(
        "{}Transcribe this audio clip and tell me what was said.",
        llama::MtmdContext::default_marker()
    );
    let messages = vec![llama::ChatMsg {
        role: "user".to_string(),
        content: user_content,
    }];
    let template = backend.model_chat_template();
    let prompt = llama::render_chat(template.as_deref(), &messages, true)
        .expect("render_chat with model's embedded template");
    eprintln!("[audio-int] rendered prompt: {prompt:?}");

    let gen_start = Instant::now();
    let mut sampling = SamplingConfig::chat();
    sampling.temperature = 0.0; // greedy
    sampling.top_k = 0;
    sampling.top_p = 1.0;
    sampling.repeat_penalty = 1.0;
    let (text, tokens) = backend
        .generate_with_audio(
            &prompt,
            &audio,
            120, // max_tokens — keep test cheap
            sampling,
            &[], // no extra stops; rely on EOS
        )
        .expect("generate_with_audio runs against the loaded backend");
    let gen_ms = gen_start.elapsed().as_millis();

    eprintln!(
        "[audio-int] generated {} tokens in {}ms — model said: {:?}",
        tokens, gen_ms, text
    );

    // Assertion: the response should contain at least one signal word
    // from the source utterance. We're not pinning an exact string —
    // qwen2-audio is allowed to paraphrase, describe the voice, or
    // transcribe verbatim. What it must NOT do is hallucinate something
    // disconnected (e.g. "I see a cat", "the user is silent") which
    // would mean the audio bytes never made it to the encoder.
    let lower = text.to_lowercase();
    let signal_words = [
        "hello",
        "test",
        "audio",
        "model",
        "describe",
        "hear",
        "clip",
        "understanding",
    ];
    let hits: Vec<&str> = signal_words
        .iter()
        .copied()
        .filter(|w| lower.contains(w))
        .collect();

    assert!(
        !hits.is_empty(),
        "model output contained no signal word from the source clip — \
         either the audio path is broken or the encoder produced garbage. \
         Output was: {text:?}"
    );
    eprintln!("[audio-int] OK — output matched signal words: {hits:?}");
}
