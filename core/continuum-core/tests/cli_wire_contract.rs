//! Wire-contract regression test: the `ctm generate` CLI's inline-JSON
//! params shape MUST round-trip through `TextGenerationRequest`'s
//! serde decoder.
//!
//! ## Why this lives in continuum-core's tests
//!
//! The CLI (`apps/cli`) intentionally does NOT dev-depend on
//! `continuum-core` types — it constructs JSON inline to match the
//! wire shape (see `apps/cli/src/main.rs::run_generate`). That makes
//! the wire shape a SILENT contract: drift in either direction
//! (substrate adds a required field; CLI typos a field name) is
//! invisible until the first live dispatch fails at the substrate.
//!
//! This test pins the CLI's exact JSON shape against the substrate's
//! `TextGenerationRequest` decoder. If the CLI's shape ever stops
//! decoding cleanly, this test fails at `cargo test -p continuum-core`
//! — before any operator types `ctm generate --prompt ...` and hits
//! a useless "data did not match any variant of untagged enum
//! MessageContent" error from the substrate.
//!
//! ## What the contract is
//!
//! Per `core/continuum-core/src/ai/types.rs`:
//! - `ChatMessage.content: MessageContent`
//! - `MessageContent = #[serde(untagged)] enum { Text(String), Parts(Vec<ContentPart>) }`
//! - An OBJECT shape (e.g. `{"type": "text", "text": "..."}`) matches
//!   NEITHER variant — Text wants a string, Parts wants an array.
//!
//! The CLI uses the simplest shape — plain string. The substrate's
//! own legacy `prompt`-param path produces the same shape, so it's
//! the safest wire choice (already exercised end-to-end).
//!
//! Regression caught: PR #1561 round 1 R1 review (BLOCK, 98 conf).
//! The original PR shipped `"content": {"type": "text", "text": ...}`
//! (object). Every `ctm generate` invocation would have failed at
//! the substrate. This test would have caught it pre-merge.

use continuum_core::ai::types::{MessageContent, TextGenerationRequest};

// what this catches: cf63a02e — an ordinary CLI request during a deploy used to
// launch a second (possibly stale) core. Exercise the executable, not a policy
// predicate. The child has an isolated home/endpoint and a harmless start script,
// so restoring autostart fails this test without launching a real server.
#[test]
fn ordinary_commands_never_start_a_core() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("absent.sock");
    let script = root.path().join("must-not-start.sh");
    std::fs::write(&script, "#!/bin/sh\nexit 97\n").unwrap();
    for (args, expected_exit) in [
        (vec!["ping"], 2),
        (vec!["serving/status"], 2),
        (vec!["commands/list"], 2),
        (vec!["ai/future-operation"], 2),
        (vec!["serving/status", "--help"], 2),
        // Verification never started a core: retain its transport diagnostic,
        // rather than replacing it with dispatch's generic no-core refusal.
        (vec!["deploy-verify"], 1),
        (vec!["verify"], 1),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_continuum"))
            .args(&args)
            .current_dir(root.path())
            .env("HOME", root.path())
            .env("USERPROFILE", root.path())
            .env("CONTINUUM_HOME", root.path())
            .env("CONTINUUM_CORE_SOCKET", &socket)
            .env("CONTINUUM_CORE_TCP", "0")
            .env("CONTINUUM_START_SCRIPT", &script)
            .env("CONTINUUM_FROM_SOURCE", "1")
            .env_remove("CONTINUUM_NO_AUTOSTART")
            .output()
            .expect("run the CLI under test");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "{args:?}: {stderr}"
        );
        assert!(stderr.contains("no core answering"), "{args:?}: {stderr}");
        if expected_exit == 1 {
            assert!(stderr.contains("deploy-verify:"), "{args:?}: {stderr}");
            assert!(stderr.contains("connect"), "{args:?}: {stderr}");
        }
        assert!(
            output.stdout.is_empty(),
            "refusal must not pollute JSON stdout"
        );
        assert!(
            !root.path().join("absent.sock.pid").exists(),
            "{args:?} launched a child"
        );
    }
}

/// The EXACT JSON shape `ctm generate --prompt "hello"` builds.
/// Hand-mirror what `apps/cli/src/main.rs::run_generate` constructs.
/// Do NOT consolidate this with the CLI source — the whole point is
/// that the CLI builds JSON inline (no shared type), and this test
/// pins the inline shape to the substrate's typed contract.
fn cli_generate_params(prompt: &str, model: Option<&str>) -> serde_json::Value {
    let mut params = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": prompt,
            }
        ],
    });
    if let Some(m) = model {
        params["model"] = serde_json::Value::String(m.to_string());
    }
    params
}

#[test]
fn cli_generate_params_decode_as_text_generation_request() {
    let params = cli_generate_params("hello substrate", None);
    let decoded: TextGenerationRequest =
        serde_json::from_value(params.clone()).unwrap_or_else(|e| {
            panic!(
                "CLI's `ctm generate` JSON shape failed to decode as TextGenerationRequest: {e}\n\
                 wire payload:\n{}",
                serde_json::to_string_pretty(&params).unwrap()
            );
        });
    assert_eq!(decoded.messages.len(), 1);
    assert_eq!(decoded.messages[0].role, "user");
    // The CLI sends a plain string; the substrate decodes it as
    // MessageContent::Text. Any other variant means the wire shape
    // drifted.
    match &decoded.messages[0].content {
        MessageContent::Text(s) => assert_eq!(s, "hello substrate"),
        other => panic!(
            "expected MessageContent::Text variant; CLI's plain-string \
             content shape decoded as {other:?}"
        ),
    }
}

#[test]
fn cli_generate_params_with_model_pins_model_field() {
    let params = cli_generate_params("ping", Some("qwen3.5-4b-code-forged"));
    let decoded: TextGenerationRequest = serde_json::from_value(params)
        .expect("CLI shape with --model must decode as TextGenerationRequest");
    assert_eq!(decoded.model.as_deref(), Some("qwen3.5-4b-code-forged"));
}

#[test]
fn cli_generate_object_content_shape_would_panic() {
    // Negative test — pin the wire-bug that PR #1561 round 1 R1 caught.
    // If the CLI ever reverts to the object shape, this test would
    // need to flip from "expected error" to "decodes cleanly" — which
    // means someone has to LOOK at the contract and decide.
    let bad_shape = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": { "type": "text", "text": "ping" },
            }
        ],
    });
    let decoded = serde_json::from_value::<TextGenerationRequest>(bad_shape);
    assert!(
        decoded.is_err(),
        "object-shaped content MUST fail to decode — MessageContent is \
         #[serde(untagged)] enum {{ Text(String), Parts(Vec<ContentPart>) }} \
         and an object matches neither variant. If this assertion ever fires, \
         someone added an object variant to MessageContent without updating \
         the CLI's wire shape — coordinate before merging."
    );
}
