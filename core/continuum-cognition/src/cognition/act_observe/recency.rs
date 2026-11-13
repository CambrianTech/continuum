//! Recency-channel rendering: turn one executed act (its args + result) into
//! bounded, legible working-memory text. Extracted verbatim from `act_observe`
//! (pure code-motion, #386 decomposition) — one responsibility, one file.
//!
//! The bounds are NEVER constants: they derive from the persona's live served
//! window via `ContextBudget`. See `cognition/context_budget.rs`.

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// Decode a tool result for HER WINDOW (card 0a4c0648, Joel's encoding catch 2026-08-08).
///
/// The executor hands back each command's response as serde-serialized JSON, and this
/// renderer used to embed that string VERBATIM. Net effect: a `code/read` of a Python
/// file reached working memory as one enormous line of `\n` literals and `\"` quotes.
/// Indentation was byte-preserved and structurally illegible — and block indentation is
/// exactly what her next `code/edit` is parsed and graded on. Glass-boxed on Benchy's
/// requests-2148 solve: right file, right line, right fix, replacement block mis-indented
/// against code she had only ever seen escaped; the parse gate refused; she abandoned.
///
/// The decode rule, ONE seam for every tool: if the result parses as a JSON object,
/// render each top-level field as `key: value` where STRING values print RAW — real
/// newlines, real columns, code as code. Multi-line strings open on their own line so
/// column 0 of the payload is column 0 in her window. Null fields are dropped (an
/// `error: null` teaches nothing). Non-string values (numbers, bools, nested structure)
/// stay compact JSON — they were never the legibility problem. A bare JSON string
/// unwraps; anything that isn't JSON was never escaped and passes through untouched.
pub(super) fn humanize_result_content(raw: &str) -> String {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    match parsed {
        // A bare JSON string is an ENCODING LAYER, not content — live-verified on
        // Benchy's r2 run: the executor hands back `"{\"error\":null,...}"` (the
        // command's JSON wrapped in a JSON string), so unwrapping once still left
        // raw JSON in her window. Recurse: each pass strips one string layer;
        // a plain-text payload stops the recursion at the parse guard above.
        serde_json::Value::String(s) => humanize_result_content(&s),
        serde_json::Value::Object(map) => {
            let mut out = String::new();
            for (key, value) in &map {
                if value.is_null() {
                    continue;
                }
                match value {
                    serde_json::Value::String(s) if s.contains('\n') => {
                        // Own line so the payload's indentation columns are hers too.
                        out.push_str(key);
                        out.push_str(":\n");
                        out.push_str(s);
                    }
                    serde_json::Value::String(s) => {
                        out.push_str(key);
                        out.push_str(": ");
                        out.push_str(s);
                    }
                    other => {
                        out.push_str(key);
                        out.push_str(": ");
                        out.push_str(&serde_json::to_string(other).unwrap_or_default());
                    }
                }
                out.push('\n');
            }
            if out.is_empty() {
                raw.to_string()
            } else {
                out
            }
        }
        // Arrays / numbers / bools: compact JSON was already legible.
        _ => raw.to_string(),
    }
}

/// How her tool call's arguments appear in its result's header: a SHAPE rule, never a size (one
/// budget: no reader decides its own length). A single-line value (a path, an id, a flag) names
/// WHICH call this was and is shown whole. A multi-line value (a file's content, a patch) is a body
/// she wrote in the call itself, already in her view as her own tool call, so the header names it by
/// its shape and hash instead of repeating it: repeating it put a whole-file paste ahead of the
/// result she had to read (sympy-21379: all of basic.py ahead of the `EDIT REFUSED` diagnostic).
pub(super) fn render_call_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.contains('\n') => format!("{k}: {}", body_reference(s)),
                serde_json::Value::String(s) => format!("{k}={s}"),
                other => format!("{k}={other}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::String(s) if s.contains('\n') => body_reference(s),
        other => other.to_string(),
    }
}

/// A body named by its shape and a hash, so two different bodies never read the same. The hash is
/// a label in a human-readable receipt, never a cache key or an admission signal.
fn body_reference(body: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    format!("<{} lines, {} chars, #{:x}: in your call>", body.lines().count(), body.chars().count(), h.finish())
}

/// Collapse tool ARGS for the recall channel: a large string value (e.g. a whole file
/// passed in `content`) becomes `<key>: N chars` — re-showing that file verbatim on every
/// future turn is the dead weight that taxes context (measured: it drowned an unfamiliar
/// 8B). Small args pass through compact.
fn summarize_args_for_recall(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) if s.chars().count() > 80 => {
                    format!("{k}: {} chars", s.chars().count())
                }
                other => format!(
                    "{k}={}",
                    truncate_chars(other.to_string().trim_matches('"'), 60)
                ),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_chars(&other.to_string(), 120),
    }
}

/// Render ONE completed act as a desktop-style COLLAPSED reference for the RECALL channel —
/// the PX side of the universal handle/expand primitive ([[handles-events-expansion-one-
/// universal-primitive]]): a small result stays inline (like a short link); a big one is a
/// one-line summary the mind expands on demand (the full body already carries its own
/// `tool/output` handle when the executor spilled it — the same handle a positron thumbnail
/// would open); an ERROR is always shown in full (highlighted — never hide what broke). The
/// RECENCY channel (working memory) still holds the whole latest result; this only slims
/// what recall re-injects turn after turn.
pub(super) fn render_act_for_recall(
    name: &str,
    args: &serde_json::Value,
    intent: &str,
    verdict: crate::sdk_codegen::ActVerdict,
    body: &str,
) -> String {
    const RECALL_INLINE_MAX: usize = 280;
    let args_summary = summarize_args_for_recall(args);
    let is_err = verdict.failed();
    // THE COLLAPSED LINE MUST NOT SAY "ok" FOR WORK THAT DID NOT FINISH (Astra on
    // #4352). This channel is what the Episodic engram re-injects turn after turn —
    // it is the mind reading its own past — and a long-bodied act that was still
    // RUNNING, or whose result could not be decoded, took the else-branch below and
    // was collapsed to "ok — {first line}". The short-body branch is unaffected
    // because it quotes the body verbatim and asserts nothing.
    let label = match verdict {
        v if v.running() => "running —",
        crate::sdk_codegen::ActVerdict::Undecodable => "unreadable —",
        _ => "ok —",
    };
    let outcome = if is_err {
        format!("FAILED:\n{}", truncate_chars(body.trim(), 800))
    } else if body.trim().chars().count() <= RECALL_INLINE_MAX {
        body.trim().to_string()
    } else {
        format!(
            "{label} {}",
            truncate_chars(body.trim().lines().next().unwrap_or(""), 140)
        )
    };
    let mark = if is_err {
        "⚠ "
    } else if verdict.running() {
        "⋯ "
    } else {
        ""
    };
    // Omit "because …" when there's no real stated reason — an empty intent must
    // not render an imitable receipt template (#158).
    let because = if intent.trim().is_empty() {
        String::new()
    } else {
        format!(" because {}", intent.trim())
    };
    // No first-person "I ran" opener (#158): measured 2026-07-13 that base models
    // copy the receipt verbatim to OPEN a room message ("I ran X → ok — {…}") — the
    // line-anchored stop can't catch a position-0 opener, but a bare `name(args)`
    // memory entry doesn't read as speech, so it's not reproduced as one.
    format!("{mark}{name}({args_summary}){because} → {outcome}\n\n")
}

#[cfg(test)]
mod tests {
    use crate::sdk_codegen::{ActVerdict, ToolVerdict};
    /// The two pre-#4352 states this file used to take as a bare bool.
    const OK_V: ActVerdict = ActVerdict::Declared(ToolVerdict::Succeeded);
    const FAIL_V: ActVerdict = ActVerdict::Declared(ToolVerdict::Failed);

    use super::*;

    /// what this catches: card 0a4c0648 — the receipt renderer embedding the executor's
    /// serde-serialized JSON VERBATIM, so a code/read result reached her window as one
    /// line of `\n` literals with structurally illegible indentation. Benchy diagnosed
    /// requests-2148 correctly and still lost the solve to a mis-indented edit against
    /// code she had only ever seen escaped. String payloads must render as vertical
    /// text with real columns; non-JSON results must pass through byte-identical.
    #[test]
    fn tool_result_json_renders_as_vertical_text_not_escape_soup() {
        // The exact shape the executor returns for a code/read: JSON object whose
        // `content` field is the file text with real newlines *inside the JSON string*.
        let read_result = serde_json::json!({
            "content": "def iter_content(self):\n    while True:\n        chunk = \"x\"\n",
            "file_path": "requests/models.py",
            "error": null,
            "lines": 3
        })
        .to_string();
        assert!(
            read_result.contains("\\n"),
            "precondition: the wire form really is escape-soup"
        );

        let rendered = humanize_result_content(&read_result);
        assert!(
            rendered.contains("    while True:\n        chunk = \"x\""),
            "file content must come out as REAL indented lines, got: {rendered}"
        );
        assert!(
            !rendered.contains("\\n"),
            "no escape literals may survive into her window: {rendered}"
        );
        assert!(
            !rendered.contains("error"),
            "null fields are dropped — `error: null` teaches nothing"
        );
        // Multi-line strings open on their own line so payload column 0 is window column 0.
        assert!(
            rendered.contains("content:\ndef iter_content"),
            "{rendered}"
        );
        assert!(
            rendered.contains("file_path: requests/models.py"),
            "{rendered}"
        );
        assert!(rendered.contains("lines: 3"), "{rendered}");

        // Non-JSON results (plain shell output) pass through untouched.
        let plain = "FAILED tests/test_lowlevel.py - 3 passed, 1 failed";
        assert_eq!(humanize_result_content(plain), plain);

        // A bare JSON string unwraps to its text.
        assert_eq!(humanize_result_content("\"line1\\nline2\""), "line1\nline2");

        // The DOUBLE-ENCODED shape, live-verified on Benchy's r2 run: the executor
        // wraps the command's JSON doc in a JSON string, so one unwrap still left
        // `{"error":null,...}` in her window. Every string layer must strip.
        let double = serde_json::Value::String(read_result.clone()).to_string();
        let rendered2 = humanize_result_content(&double);
        assert_eq!(
            rendered2, rendered,
            "a double-encoded result must humanize identically to the single-encoded one"
        );
        assert!(
            !rendered2.contains("\"error\":null"),
            "the inner JSON layer must not survive raw: {rendered2}"
        );
    }

    // what this catches: recall collapse (the PX/handle primitive, RAG side). A code/write
    // carries a WHOLE FILE in `content` — on recall that must become `content: N chars`, not
    // the file re-shown every future turn (the measured context tax). A small success stays
    // inline; an ERROR is always shown, highlighted. The recency channel keeps the full trace;
    // this only guards what recall re-injects.
    #[test]
    fn recall_collapses_big_args_and_highlights_errors() {
        let big = "fn main(){}\n".repeat(200); // a whole "file"
        let args = serde_json::json!({ "file_path": "x.rs", "content": big });
        let ref_ok =
            render_act_for_recall("code/write", &args, "acting", OK_V, "{\"success\":true}");
        assert!(
            ref_ok.contains("content: "),
            "big content arg must collapse to a size"
        );
        assert!(ref_ok.contains("chars"), "collapsed arg names its size");
        assert!(
            !ref_ok.contains("fn main(){}\nfn main(){}"),
            "the file must NOT be re-shown verbatim"
        );

        // small success → inline
        let small = render_act_for_recall(
            "code/read",
            &serde_json::json!({"file_path":"a"}),
            "acting",
            OK_V,
            "hello",
        );
        assert!(small.contains("→ hello"), "small result stays inline");

        // error → highlighted + shown
        let err = render_act_for_recall(
            "code/shell",
            &serde_json::json!({"cmd":"x"}),
            "acting",
            FAIL_V,
            "error: no such file",
        );
        assert!(err.starts_with("⚠"), "errors are highlighted");
        assert!(
            err.contains("FAILED") && err.contains("no such file"),
            "errors are shown, never hidden"
        );
    }

    // what this catches: #158 — an EMPTY intent (no `<think>` reasoning) renders NO
    // "because …" clause, so the receipt carries nothing template-shaped for a base
    // model to imitate. The old fabricated default ("{name} is acting on the current
    // situation") was the identity-bleed mimicry fuel. A real intent still renders.
    #[test]
    fn empty_intent_renders_no_because_clause() {
        let args = serde_json::json!({"file_path": "a"});
        let empty = render_act_for_recall("code/read", &args, "", OK_V, "hi");
        assert!(!empty.contains("because"), "no fabricated reason: {empty}");
        assert!(
            empty.contains("code/read("),
            "the act is still recorded by name(args)"
        );
        assert!(
            !empty.contains("I ran"),
            "no imitable 'I ran' opener (#158): {empty}"
        );
        let real = render_act_for_recall("code/read", &args, "checking the header", OK_V, "hi");
        assert!(
            real.contains("because checking the header"),
            "a real intent still shows"
        );
    }

    // what this catches: the recency-channel result bound (#165) — a huge raw-JSON
    // result (a 5000-entry code/list, a multi-match code/search) is cut CLEANLY at
    // the source with a teaching marker, never dumped whole (flood) and never left
    // for the downstream budget to cut mid-JSON (the garbled/nested line_content a
    // persona then loops on). A small result passes through untouched.
    // what this catches: the RESULT buried under her own ARGS. The recall path has collapsed
    // whole-file args forever; the recency path echoed `serde_json::to_string(&call.input)`
    // raw. Live on sympy-21379 — her one `code/edit` passed all of basic.py as `content`, so
    // that paste went into working memory AHEAD of the `EDIT REFUSED` result carrying the
    // diagnostic, on a 16k lane. She never landed an edit. The thing she must READ is the
    // result; the args she wrote herself one generation ago.
    #[test]
    fn a_whole_file_arg_is_not_echoed_back_ahead_of_the_result() {
        let whole_file = "x = 1\n".repeat(4000); // ~24k chars, a real source file
        let args = serde_json::json!({ "file_path": "sympy/core/basic.py", "content": whole_file });
        let rendered = render_call_args(&args);
        assert!(rendered.contains("file_path=sympy/core/basic.py"), "the identifier is shown whole: {rendered}");
        assert!(rendered.contains("4000 lines, 24000 chars"), "the body is named by its shape: {rendered}");
        assert!(!rendered.contains("x = 1"), "the body she wrote is never repeated: {rendered}");

        // The rule is shape, never size: a short multi-line edit is a body too (it is in her
        // call), and a long single-line value is an identifier, shown whole.
        let small = serde_json::json!({ "file_path": "a.py", "new_content": "def f():\n    return refine_arg(x)\n" });
        assert!(render_call_args(&small).contains("new_content: <2 lines"));
        let long_id = "p".repeat(5_000);
        assert!(render_call_args(&serde_json::json!({ "query": long_id })).contains(&long_id));
    }

    /// what this catches (Astra on #4352): the COLLAPSE branch saying "ok" for work
    /// that did not finish. Only a LONG body takes that branch — a short one quotes
    /// itself verbatim and asserts nothing — so the regression needs a body over
    /// RECALL_INLINE_MAX. This channel is what the Episodic engram re-injects on
    /// later turns, which makes it the mind reading its own past: a running shell
    /// remembered as "ok" is worse than the room glyph being wrong.
    #[test]
    fn a_long_bodied_act_that_did_not_finish_is_never_collapsed_to_ok() {
        let args = serde_json::json!({"command": "cargo test"});
        let long = "x".repeat(400);

        let running = render_act_for_recall(
            "code/shell",
            &args,
            "building",
            ActVerdict::Declared(ToolVerdict::Running),
            &long,
        );
        assert!(
            !running.contains("ok —"),
            "a running act must not be remembered as ok: {running}"
        );
        assert!(running.contains("running —"), "{running}");
        assert!(running.starts_with("⋯"), "and it carries its own mark: {running}");

        let unreadable =
            render_act_for_recall("code/run", &args, "running", ActVerdict::Undecodable, &long);
        assert!(
            !unreadable.contains("ok —"),
            "a result nobody could decode must not be remembered as ok: {unreadable}"
        );
        assert!(unreadable.contains("unreadable —"), "{unreadable}");

        // The unchanged paths stay unchanged.
        let ok = render_act_for_recall("code/read", &args, "reading", OK_V, &long);
        assert!(ok.contains("ok —"), "{ok}");
        let unprojected =
            render_act_for_recall("code/read", &args, "reading", ActVerdict::Unprojected, &long);
        assert!(
            unprojected.contains("ok —"),
            "a command that never opted in is byte-for-byte as before: {unprojected}"
        );
    }
}
