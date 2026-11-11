//! Standalone verification of the unified bracket handling in not_speech.rs.
//! Copies just opens_with_tool_envelope + its dependencies, runs all test cases.

fn verb_ok(v: &str) -> bool {
    !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "/_-.".contains(c))
}

fn opens_with_tool_envelope(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix('[') {
        // Unified bracket handling using two complementary scans.
        // Scan A (split on `]`) handles comma-with-bracket form, stage-word+call, and bare namespaced brackets.
        // Scan B (split on `(`) handles paren-object form (JSON or kwargs) and labelled calls like [call form: code/read({…})].

        // Scan A: split on `]` — handles comma-with-bracket, stage-word+call, bare namespaced.
        if let Some((before_close, after_close)) = rest.split_once(']') {
            let trimmed_after = after_close.trim_start();

            // Comma-with-bracket form: [ns/verb] {…}
            if verb_ok(before_close) && before_close.contains('/') && trimmed_after.starts_with('{') {
                return true;
            }

            // Stage word + namespaced call: [stage] callee(obj or {obj})
            let a = after_close.trim_start();
            let call_len = a.find(['(', '{', ' ', '\n']).unwrap_or(a.len());
            if call_len > 0 && verb_ok(before_close) && verb_ok(&a[..call_len]) && matches!(a[call_len..].chars().next(), Some('(') | Some('{')) {
                return true;
            }

            // Bare namespaced: [ns/verb] with nothing object-like after ]
            if verb_ok(before_close) && before_close.contains('/') && !trimmed_after.starts_with('{') {
                return true;
            }
        }

        // Scan B: split on `(` — handles paren-object and labelled-call forms.
        if let Some((before_paren, after_paren)) = rest.split_once('(') {
            let a = after_paren.trim_start();

            let has_json = a.starts_with('{');
            let has_kwargs = a.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));

            // Direct namespaced call: [ns/verb({…})] or [ns/verb(kwargs=…)]
            if verb_ok(before_paren) && before_paren.contains('/') && (has_json || has_kwargs) {
                return true;
            }

            // Labelled call: scan tokens in label text for namespaced callee.
            for token in before_paren.split(|c: char| c.is_whitespace() || c == ':') {
                if verb_ok(token) && token.contains('/') && (has_json || has_kwargs) {
                    return true;
                }
            }
        }
    }

    // THE FOURTH DIALECT: markdown fence
    if t.starts_with("```") && has_namespaced_call_with_object(t) {
        return true;
    }

    // THE SIXTH DIALECT: observation block
    if opens_with_bare_call(t) && t.contains("\nResult:") {
        return true;
    }

    // Canonical envelope
    let compact: String = t.chars().take(24).filter(|c| !c.is_whitespace()).collect();
    if compact.starts_with("{\"tool_call\"") {
        return true;
    }

    // SEVENTH DIALECT: native name/arguments format
    is_name_arguments_envelope(t)
}

fn is_name_arguments_envelope(t: &str) -> bool {
    let body = match t.strip_prefix("```") {
        Some(fenced) => fenced.split_once('\n').map_or("", |(_, rest)| rest),
        None => t,
    };
    let compact: String = body.chars().take(48).filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("{\"name\":\"") && body.contains("\"arguments\"")
}

fn opens_with_bare_call(t: &str) -> bool {
    let Some((callee, after)) = t.split_once('(') else { return false };
    let seg = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-.".contains(c));
    let namespaced = callee.len() <= 96 && callee.contains('/') && callee.split('/').all(seg);
    let a = after.trim_start();
    let kwargs = a.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    namespaced && (a.starts_with('{') || a.starts_with(')') || kwargs)
}

fn has_namespaced_call_with_object(t: &str) -> bool {
    let bytes = t.as_bytes();
    let seg_char = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.".contains(&c);
    for (i, _) in t.match_indices('/') {
        let before = bytes[..i].iter().rev().take_while(|c| seg_char(**c)).count();
        if before == 0 { continue; }
        let after = bytes[i + 1..].iter().take_while(|c| seg_char(**c)).count();
        if after == 0 { continue; }
        let rest = &t[i + 1 + after..];
        let rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix('(') {
            if r.trim_start().starts_with('{') {
                return true;
            }
        }
    }
    false
}

// ── Test cases ────────────────────────────────────────────────────────

fn main() {
    let mut passed = 0u32;
    let mut failed: Vec<(&str, bool, Option<&str>)> = Vec::new();

    macro_rules! check {
        ($t:expr, $expected:expr $(,)?) => {{
            let got = opens_with_tool_envelope($t);
            if got == $expected {
                passed += 1;
            } else {
                failed.push(($t, $expected, Some(got)));
            }
        }};
    }

    // ── a_bracketed_call_with_parens_is_a_tool_envelope ────────────────

    check!("[code/read({\"file_path\":\"src/main.rs\"})]\nResult:\n```python\n\n# THESIS: a distributed proof is an ACTIVITY\n```", true);
    check!("[work/list({})]", true);
    check!("[execute] code/read(code=\n# THESIS…", true);
    check!("[executing] work/list{\"state\":\"open\"}", true);
    check!("[answer] the code/read verb is the one I would use", false);
    check!("[call form: code/read({\"file_path\":\"src/main.rs\"})]", true);
    check!("[the call form is code/read and it takes (a path)]", false);
    check!("[code/shell(cmd=code/read({\"file_path\":\"src/main.rs\"}) \n, lang=rust))]", true);
    check!("I ran [code/read({\"file_path\":\"x\"})] and it was fine.", false);
    check!("[note(this)] is a phrase, not a verb", false);
    check!("[code/read(path)] returned nothing", false);

    // ── the_occurrence_header_at_the_lead_is_not_speech ────────────────
    check!("[occurred 2026-09-17T01:15:37.456Z] ```python\n# THESIS\n```", false); // occurrence_header, not tool_envelope — but we only test opens_with_tool_envelope
    check!("[occupied] the lane is mine for this act", false);

    // ── the_space_separated_bracket_dialect_is_also_not_speech ─────────
    check!(r#"[code/read] {"file_path":"src/main.rs"}"#, true);

    // ── a_bare_namespaced_verb_with_no_object_is_also_not_speech ───────
    check!("[code/run]", true);
    check!("[work/list]", true);
    check!("[command/run]", true);
    check!("[code/run]\n[invalid] command_run: unsupported lang", true);

    // ── a_command_called_inside_a_fence_is_not_speech_but_a_real_snippet_is ──
    let observed = "```python\n# Mark this room's activity concluded (o); \
                    code/shell({\"cmd\":\"# Mark o\", \"timeoutSecs\": null})\n```";
    check!(observed, true);

    for speech in [
        "```rust\nfn main() {\n    let a = 5;\n}\n```",
        "```python\nfrom itertools import product\n```",
        "```python\n# Mark this room's activity concluded in 420cfe24.\n```",
    ] {
        check!(speech, false);
    }

    // Explaining the call is not making it.
    check!("To run it you'd write code/shell({\"cmd\":\"ls\"}) — note the braces.", false);

    // ── an_echoed_observation_block_is_not_speech_but_explaining_a_call_is ──
    let kimi = "code/shell(cmd=cd /c/Users/x/workspace && ls state) because Let me organize.\n\nResult:\nexecution_id: a4e208b2";
    check!(kimi, true);

    check!("code/read({\"file_path\":\"src/main.rs\"})\nResult:\nfn main() {}", true);
    check!("collaboration/chat/send(room=general, message=hi)\nResult:\nshortId: abc", true);
    check!("genome/training-trigger/submit({\"x\":1})\nResult:\nok", true);
    check!("work/list()\nResult:\n3 cards", true);

    // Explaining a call is speech.
    check!("code/shell(cmd=ls state) is how you would list it; no need to run it twice.", false);
    check!("I ran it.\nResult: the tests pass now, 12 of 12.", false);

    // ── ordinary_speech_is_never_silenced ──────────────────────────────
    for ok in [
        "I'm on card 4e4949ec — fixing the Django ModelAdmin check.",
        "[note] I read the file and the fix is committed at HEAD.",
        "Looking at code/read now to confirm the seam.",
        "{ this is just a brace }",
        "",
    ] {
        check!(ok, false);
    }

    // ── canonical envelope ────────────────────────────────────────────
    let observed = r#"[code/read,{"file_path":"src/main.rs"}] — exact args for any tool: commands/help(name)"#;
    check!(observed, true);

    check!(r#"{"tool_call": {"name": "code/read", "arguments": {}}}"#, true);

    // ── native name/arguments call ────────────────────────────────────
    for obs in [
        r#"{"name": "error", "arguments": {"message": "forbidden: no policy grants access to URI: code/shell"}}"#,
        "```json\n{\n  \"name\": \"git_pull\",\n  \"arguments\": {\n    \"branch\": null,\n    \"repo\": \"origin/main\"\n  }\n}\n```",
        r#"{ "name": "error", "arguments": { "code": "{\"staged\":true}", "lang": null, "timeoutSecs": null } }"#,
    ] {
        check!(obs, true);
    }

    // Prose quoting the envelope stays speech.
    check!(r#"My reply came out as {"name": "error", "arguments": {}} — the lifter refused it."#, false);

    // ── Results ───────────────────────────────────────────────────────
    println!("Passed: {}/{}", passed, passed + failed.len() as u32);
    if !failed.is_empty() {
        println!("\nFAILED cases ({}):", failed.len());
        for (t, expected, got) in &failed {
            let short = t.lines().next().unwrap_or(t).get(..60).unwrap_or(t);
            println!("  Input: {}", short);
            println!("    Expected: {}, Got: {}\n", expected, got.unwrap_or(false));
        }
        std::process::exit(1);
    } else {
        println!("All cases passed ✓");
    }
}
