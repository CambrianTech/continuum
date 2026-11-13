#![cfg(test)]

    use super::*;
    use crate::ToolInputSchema;

    pub(super) fn resolve_name(name: &str) -> String {
        match name {
            "edit_file" => "code/edit",
            "run_code" => "code/run",
            "file_tree" => "code/tree",
            "read_file" => "code/read",
            "list_files" | "list_dir" => "code/list",
            "write_file" => "code/write",
            "code_read" => "code/read",
            "list_tasks" => "work/list",
            other => other,
        }
        .to_string()
    }

    pub(super) fn parse_tool_call(text: &str) -> Option<ToolCall> {
        super::parse_tool_call(text, resolve_name)
    }

    pub(super) fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
        super::parse_tool_calls(text, resolve_name)
    }

    pub(super) fn attempted_tool_name(text: &str) -> Option<String> {
        super::attempted_tool_name(text, resolve_name)
    }

    pub(super) fn nameless_args_fence(text: &str) -> Option<String> {
        super::nameless_args_fence(text, resolve_name)
    }

    // Regression: Kimi's native visual response became a fictitious #fff/rgb
    // action. Prose-only mentions stay speech; explicit invocations still work.
    #[test]
    pub(super) fn narrated_mentions_are_not_invocations() {
        for text in [
            "I'll inspect the page. The text color is `#fff/rgb(139,147,167)`.",
            "I'll describe the image captured by `perception/observe`.",
            "Let's use `list_tasks` to find the card.",
            "I'll inspect `docs/architecture` before deciding what to do.",
            "Please use `list_tasks` to find the card.",
        ] {
            assert!(
                parse_tool_calls(text).is_empty(),
                "prose became an action: {text}"
            );
        }
        let call = parse_tool_call(r#"{"tool_call":{"name":"list_tasks","arguments":{}}}"#)
            .expect("explicit invocation remains supported");
        assert_eq!(call.name, "list_tasks");
        assert_eq!(call.input, serde_json::json!({}));
    }

    pub(super) fn spec(name: &str, desc: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the instruction block names every tool + its description
    // and states the exact JSON contract — what a small model needs to call a tool.
    #[test]
    pub(super) fn instructions_list_tools_and_the_contract() {
        let s = render_tool_instructions(&[spec("ping", "Health check.")]);
        assert!(s.contains("\"tool_call\""));
        assert!(s.contains("ping: Health check."));
    }

    // what this catches: the narrate-instead-of-act reflex — Qwen-Coder-32B on a real
    // task emitted `code/write with file_path="..." and content: ```rust\n<code>\n``` `
    // as PROSE (no JSON envelope), so acts=0 and the file never landed. The last-resort
    // NarratedWriteFormat must recover it into a real code/write with the fenced code.
    #[test]
    pub(super) fn recovers_a_narrated_code_write_with_fenced_code() {
        let text = "Sure — I'll use code/write with file_path=\"lru.rs\" and content of \
                    the implementation:\n\n```rust\npub struct LruCache;\nimpl LruCache { }\n```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "narrated write should parse to one call");
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(calls[0].input["file_path"], "lru.rs");
        assert!(
            calls[0].input["content"]
                .as_str()
                .unwrap()
                .contains("pub struct LruCache"),
            "the fenced code must become the write content"
        );
    }

    // what this catches: the last-resort format must NOT false-trigger on prose that
    // merely mentions code/write without an actual file + fenced code to write.
    #[test]
    pub(super) fn narrated_write_ignores_prose_without_a_fence() {
        assert!(
            parse_tool_calls("I could use code/write to save file_path=x.rs later.").is_empty()
        );
    }

    // what this catches: under llama-server --jinja the 14B emits its call wrapped in
    // `<tools>…</tools>` (a template quirk — not the canonical `<tool_call>`), and
    // llama-server does NOT stamp finish=tool_calls for it. The universal salvage MUST
    // still extract it (via the inner {name,arguments} object) or the persona's hands go
    // dead on the exact path native tool-calling now takes. Regression for the --jinja fix.
    #[test]
    pub(super) fn extracts_jinja_tools_wrapped_call() {
        let text = "<tools>\n{\"name\": \"list_dir\", \"arguments\": {\"path\": \".\"}}\n</tools>";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the tools-wrapped call must be recovered");
        assert_eq!(calls[0].name, "list_dir");
        assert_eq!(calls[0].input["path"], ".");
    }

    // what this catches: a clean bare JSON tool call parses to a ToolCall with the
    // right name + args (the happy path).
    #[test]
    pub(super) fn parses_bare_tool_call() {
        let tc = parse_tool_call(
            r#"{"tool_call": {"name": "data/list", "arguments": {"collection": "rooms"}}}"#,
        )
        .expect("a tool call");
        assert_eq!(tc.name, "data/list");
        assert_eq!(tc.input["collection"], "rooms");
        assert!(tc.id.starts_with("jip-"));
    }

    // what this catches: THE real-model mess — prose + <think> + a ```json fence
    // around the call. Must still extract it (models never emit bare JSON).
    #[test]
    pub(super) fn parses_through_prose_think_and_fences() {
        let text = "<think>\nI should check health.\n</think>\nSure! Here:\n```json\n{\"tool_call\": {\"name\": \"ping\"}}\n```\n";
        let tc = parse_tool_call(text).expect("tool call through the mess");
        assert_eq!(tc.name, "ping");
        // no-arg call → arguments default to {}
        assert_eq!(tc.input, serde_json::json!({}));
    }

    // what this catches: a normal answer (no tool call) returns None — the common
    // case; we must NOT fabricate a call from ordinary prose.
    #[test]
    pub(super) fn no_tool_call_in_plain_answer() {
        assert!(parse_tool_call("The deploy looks healthy; nothing to do.").is_none());
        // an unrelated JSON object is not a tool_call envelope
        assert!(parse_tool_call(r#"Here's data: {"status": "ok", "count": 3}"#).is_none());
    }

    // what this catches: brace-in-string doesn't fool the balanced scan, and the
    // outer envelope is preferred over any inner object.
    #[test]
    pub(super) fn handles_braces_in_strings() {
        let tc = parse_tool_call(
            r#"{"tool_call": {"name": "chat/send", "arguments": {"text": "use {curly} braces"}}}"#,
        )
        .expect("call");
        assert_eq!(tc.name, "chat/send");
        assert_eq!(tc.input["text"], "use {curly} braces");
    }

    // what this catches: THE live failure (Asha, 2026-06-22, seen via the prompt
    // capture) — a base model emitted a MALFORMED first call (missing a closing
    // brace) followed by a VALID second, wrapped in ``` fences. The old
    // single-shot parser returned None and the calls never executed (her hands
    // went dead). parse_tool_calls must skip the malformed one and still extract
    // the valid one, so a persona's agency survives messy model output.
    #[test]
    pub(super) fn recovers_valid_call_next_to_a_malformed_sibling_in_fences() {
        let messy = "{\"tool_call\": {\"name\": \"code/read\", \"arguments\": {\"path\": \"df11d4c4\"}}\n\
                     ```\n\
                     {\"tool_call\": {\"name\": \"code/write\", \"arguments\": {\"path\": \"df11d4c4\", \"content\": \"it's claimed\"}}}\n\
                     ```";
        let calls = parse_tool_calls(messy);
        assert!(
            calls.iter().any(|c| c.name == "code/write"),
            "the well-formed call must be recovered despite the malformed sibling: {calls:?}"
        );
        // The single-shot accessor also yields a call now (not None).
        assert!(
            parse_tool_call(messy).is_some(),
            "single-shot must no longer return None"
        );
    }

    // what this catches: MULTIPLE well-formed calls in one turn are all returned,
    // in order — a base model that batches tool calls gets all of them executed.
    #[test]
    pub(super) fn extracts_multiple_well_formed_calls() {
        let two = r#"{"tool_call": {"name": "code/read", "arguments": {"file_path": "a.rs"}}}
        and then
        {"tool_call": {"name": "code/search", "arguments": {"pattern": "fn main"}}}"#;
        let calls = parse_tool_calls(two);
        assert_eq!(calls.len(), 2, "both calls extracted: {calls:?}");
        assert_eq!(calls[0].name, "code/read");
        assert_eq!(calls[1].name, "code/search");
    }

    // what this catches: the DYNAMIC model variety — Qwen/Hermes <tool_call> tags
    // around a bare {name,arguments}. The registry's TaggedFormat normalizes it to
    // the same canonical ToolCall, so a different base model "just works".
    #[test]
    pub(super) fn parses_qwen_style_tagged_call() {
        let q = "<tool_call>\n{\"name\": \"code/read\", \"arguments\": {\"file_path\": \"x.rs\"}}\n</tool_call>";
        let tc = parse_tool_call(q).expect("tagged call");
        assert_eq!(tc.name, "code/read");
        assert_eq!(tc.input["file_path"], "x.rs");
    }

    // what this catches: the Qwen3-Coder XML dialect going unread when the engine
    // refuses a call. Kimi's exact leak (5090, 2026-09-27) mixed the attribute and the
    // `=` spelling in one call and was posted as speech; it must run as one call with
    // a typed number, a JSON-looking file body must stay text, and a malformed
    // parameter must refuse the whole call rather than run it with half its arguments.
    #[test]
    pub(super) fn the_xml_parameter_dialect_runs_in_either_spelling() {
        let leaked = "<tool_call>\n<function=code/shell>\n<parameter name=\"cmd\">cd /c/w && git show a73053306 --stat | sed -n '1,240p'\n</parameter>\n<parameter=timeout_ms>\n90000\n</parameter>\n</function>\n</tool_call>";
        let calls = parse_tool_calls(leaked);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(
            calls[0].input["cmd"],
            "cd /c/w && git show a73053306 --stat | sed -n '1,240p'"
        );
        assert_eq!(calls[0].input["timeout_ms"], serde_json::json!(90000));

        let write = "<function=code/write>\n<parameter=path>\nx.json\n</parameter>\n<parameter=content>\n{\"a\": 1}\n</parameter>\n</function>";
        let calls = parse_tool_calls(write);
        assert_eq!(
            calls[0].input["content"], "{\"a\": 1}",
            "a file body stays text"
        );

        let broken = "<function=code/shell>\n<parameter>\nls\n</parameter>\n</function>";
        assert!(
            parse_tool_calls(broken).is_empty(),
            "a keyless parameter refuses the call"
        );

        // regression for #4447 (Cormac): a call cut before </function> never runs
        let cut = "<tool_call>\n<function=code/shell>\n<parameter=cmd>\nrm -rf build\n</parameter>\n<parameter=timeout_ms>\n90";
        assert!(
            parse_tool_calls(cut).is_empty(),
            "a call cut at max tokens holds half its arguments"
        );
    }

    // what this catches: Llama/Mistral-style BARE call with `parameters` (not
    // `arguments`). BareFormat + the `parameters` alias normalize it.
    // what this catches: the `{"command": <tool>, "params": {…}}` envelope must
    // lift. Glass-boxed live 2026-08-05 from Asha (Devstral-Small-2507): she
    // emitted an OFFERED tool name with correct params in exactly this shape and
    // it was dropped on the name-key spelling alone — `params` was already an
    // accepted args key, so half her envelope was understood and the half naming
    // the tool was not. Result: hours of narrated exploration, zero acts. This is
    // the SECOND time this exact class bit us (see ToolCallJson's doc for the
    // `{"function": …}` instance), which is why the negative guard below matters
    // more than the positive case.
    #[test]
    pub(super) fn command_keyed_envelope_lifts_but_never_forges_a_tool_from_shell_args() {
        // Asha's exact live emission, fence and narration included.
        let live = "To proceed effectively, let me start by exploring the root \
directory of the workspace to understand its structure. I'll use the `file_tree` \
command to get an overview:\n\n```json\n{\n \"command\": \"file_tree\",\n \"params\": {\n  \
\"path\": \".\"\n }\n}\n```\n\nThis will help me identify which directories contain \
relevant code.";
        let tc = parse_tool_call(live).expect("her command-keyed envelope must lift");
        assert_eq!(
            tc.name, "file_tree",
            "the offered wire name is preserved for resolution"
        );
        assert_eq!(tc.input.get("path").and_then(|v| v.as_str()), Some("."));

        // THE GUARD: `command` is code/shell's own ARGUMENT name. A shell call's
        // inner args object scans as a candidate too, and must never become a
        // tool named after the shell line.
        let shell = r#"{"name": "code/shell", "arguments": {"command": "cargo test"}}"#;
        let calls = parse_tool_calls(shell);
        assert_eq!(
            calls.len(),
            1,
            "exactly one call — the shell call itself: {calls:?}"
        );
        assert_eq!(calls[0].name, "code/shell");
        assert!(
            !calls.iter().any(|c| c.name == "cargo test"),
            "a shell command string must never be lifted as a tool name: {calls:?}"
        );

        // Observed live minutes later from a SECOND persona (e5f4141d): the same
        // envelope with NO params at all. A tool-shaped command name is a complete
        // call on its own — `file_tree` with no path is exactly the documented
        // no-arg form (`{"tool_call": {"name": "ping"}}`), and rejecting it would
        // leave her narrating "let me explore the whole workspace" forever.
        let no_args = parse_tool_call("Let me use the `file_tree` command with no path:\n\n```json\n{\n \"command\": \"file_tree\"\n}\n```")
            .expect("a tool-shaped command name with no args is still a call");
        assert_eq!(no_args.name, "file_tree");

        // A bare command string that resolves to nothing stays speech.
        assert!(
            parse_tool_call(r#"{"command": "make it so", "params": {}}"#).is_none(),
            "a non-tool-shaped command name must not lift"
        );
    }

    #[test]
    pub(super) fn parses_llama_style_bare_parameters_call() {
        let l = r#"sure, calling: {"name": "code/search", "parameters": {"pattern": "todo"}}"#;
        let tc = parse_tool_call(l).expect("bare call");
        assert_eq!(tc.name, "code/search");
        assert_eq!(tc.input["pattern"], "todo");
    }

    // what this catches: NO false positive — prose mentioning a JSON object with a
    // `name` but no arguments/parameters is NOT treated as a tool call (the bare
    // format's guard). A persona musing "my name is Asha" must not fire a tool.
    #[test]
    pub(super) fn prose_with_a_name_field_is_not_a_tool_call() {
        assert!(parse_tool_calls(r#"I think {"name": "Asha"} is a nice handle."#).is_empty());
        assert!(parse_tool_call("just answering normally, no tools today").is_none());
        // Nested schema keys are data, not the outer object's call envelope.
        // Substring scanning used to let nested `input` bypass the name guard,
        // or nested `name` bypass the shell-command discriminator.
        assert!(BareFormat
            .parse(
                r#"{"name":"owner_id","schema":{"input":"string"}}"#,
                resolve_name
            )
            .is_empty());
        assert!(BareFormat
            .parse(
                r#"{"command":"echo hello","schema":{"name":"field","input":"string"}}"#,
                resolve_name
            )
            .is_empty());
        assert_eq!(
            BareFormat
                .parse(
                    r#"{"name":"code/read","input":{"path":"schema.json"}}"#,
                    resolve_name
                )
                .len(),
            1
        );
        assert!(BareFormat
            .parse(r#"{"name":"owner_id","params":{}}"#, resolve_name)
            .is_empty());
        assert_eq!(
            BareFormat
                .parse(
                    r#"{"name":"read_file","params":{"path":"schema.json"}}"#,
                    resolve_name
                )
                .len(),
            1
        );
    }

    // what this catches: #293 starvation, layer 2 — SIBLING-ARGS acceptance.
    // The live emission puts the args TOP-LEVEL beside the name key
    // (`{"function": "code/list", "path": "."}`, no arguments/params key); the
    // alias-only deserialize rejected it. The siblings must become the args
    // object; an explicit args-family key still wins over extra siblings; and
    // the precision guard keeps non-tool-shaped names with siblings inert.
    #[test]
    pub(super) fn sibling_args_beside_the_name_key_lift_as_arguments() {
        let tc = parse_tool_call(r#"{"function": "code/list", "path": "."}"#).expect("lift");
        assert_eq!(tc.name, "code/list");
        assert_eq!(tc.input, serde_json::json!({ "path": "." }));
        // Explicit args key wins; siblings beside it are ignored (old behavior).
        let tc = parse_tool_call(
            r#"{"function": "work/claim", "params": {"task": "1"}, "reason": "mine"}"#,
        )
        .expect("lift");
        assert_eq!(tc.name, "work/claim");
        assert_eq!(tc.input, serde_json::json!({ "task": "1" }));
        // Non-tool-shaped name + siblings stays speech (precision guard).
        assert!(parse_tool_call(r#"{"name": "Asha", "age": 3}"#).is_none());
    }

    // what this catches: #293 layer 3, Joel's CI requirement — the EMISSION-IDIOM
    // CORPUS. Every REAL live emission shape (fixtures/tool-emission-corpus.jsonl,
    // each entry with what-this-catches provenance) must lift via the single-call
    // accessor to its expected (name, args) — or stay inert when `expect` is null.
    // New live defects APPEND corpus entries; this one test then regresses them all.
    #[test]
    pub(super) fn emission_idiom_corpus_lifts_every_live_shape() {
        let corpus = include_str!("../../../fixtures/tool-emission-corpus.jsonl");
        let mut checked = 0usize;
        for (i, line) in corpus.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue; // header comment lines
            }
            let entry: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("corpus line {} is not valid JSON: {e}", i + 1));
            let provenance = entry["provenance"]
                .as_str()
                .unwrap_or_else(|| panic!("corpus line {} missing provenance", i + 1));
            let text = entry["text"]
                .as_str()
                .unwrap_or_else(|| panic!("corpus line {} missing text", i + 1));
            let got = parse_tool_call(text);
            match entry.get("expect").filter(|e| !e.is_null()) {
                Some(exp) => {
                    let call = got.unwrap_or_else(|| {
                        panic!("[{provenance}] expected a lift, parser returned None")
                    });
                    assert_eq!(
                        call.name,
                        exp["name"].as_str().expect("expect.name is a string"),
                        "[{provenance}] wrong tool name"
                    );
                    assert_eq!(call.input, exp["args"], "[{provenance}] wrong arguments");
                }
                None => assert!(
                    got.is_none(),
                    "[{provenance}] must stay inert, but lifted: {got:?}"
                ),
            }
            checked += 1;
        }
        assert!(
            checked >= 18,
            "corpus unexpectedly small: {checked} entries"
        );
    }

    // what this catches: the narrated-script gap (#122, glass-boxed live 2026-07-09).
    // Devstral narrates a whole session — first-person intent + fences — and it parsed
    // as Speak; nothing executed; two personas hallucinated a shared workspace. The
    // format must lift the ORDERED sequence: shell, file write (creation framing),
    // shell, shell — and the faculty's single-step path takes the first.
    #[test]
    pub(super) fn narrated_session_lifts_ordered_shell_and_write_calls() {
        // Anwen's live wordstats message, verbatim shape (prompt-captures 2026-07-09).
        let text = "I'll start by creating a sample text file for testing purposes.\n\n\
```bash\necho 'the quick brown fox the lazy dog the end' > sample.txt\n```\n\n\
Now, let's write the Rust code for the word frequency program. I'll create a file called `wordstats.rs` with the following content:\n\n\
```rust\nuse std::collections::HashMap;\nfn main() { println!(\"hi\"); }\n```\n\n\
Next, I'll compile the Rust code using `rustc`:\n\n\
```bash\nrustc wordstats.rs\n```\n\n\
Finally, let's run the program with the sample text file:\n\n\
```bash\n./wordstats sample.txt\n```\n\n\
I'll paste the output here once it's ready.";
        let calls = parse_tool_calls(text);
        assert_eq!(
            calls.len(),
            4,
            "all four narrated steps lift, in order: {calls:?}"
        );
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(
            calls[0].input["cmd"],
            "echo 'the quick brown fox the lazy dog the end' > sample.txt"
        );
        assert_eq!(calls[1].name, "code/write");
        assert_eq!(calls[1].input["file_path"], "wordstats.rs");
        assert!(calls[1].input["content"]
            .as_str()
            .unwrap()
            .contains("HashMap"));
        assert_eq!(calls[2].name, "code/shell");
        assert_eq!(calls[2].input["cmd"], "rustc wordstats.rs");
        assert_eq!(calls[3].name, "code/shell");
        assert_eq!(calls[3].input["cmd"], "./wordstats sample.txt");
        // The faculty's one-step-per-generation path takes the FIRST — the rest
        // re-emerge across the drive_to_settle loop as results land in memory.
        assert_eq!(parse_tool_call(text).unwrap().name, "code/shell");
    }

    // what this catches: qwen2.5-coder's self-closing-tag idiom (glass-boxed via agent/solve
    // 2026-07-22 — the 7B wrote this exact shape then STOP, acts:0, empty patch). The wire name
    // `write_file` is what the format emits; tool_dialect::from_wire_name maps it to code/write
    // downstream (faculty), so here we assert the LIFT: name + both attrs, with the JSON-style
    // `\n` in content unescaped to a real newline so the written file isn't corrupt. regression #219
    #[test]
    pub(super) fn self_closing_write_file_tag_lifts_with_unescaped_content() {
        let text = "```python\ndef reverse_string(s):\n    return s[::-1]\n```\n\n\
                    <write_file file_path=\"reverse.py\" content=\"def reverse_string(s):\\n    return s[::-1]\" description=\"Reverse a string.\" />\nSTOP";
        let calls = parse_tool_calls(text);
        assert_eq!(
            calls.len(),
            1,
            "the self-closing write_file tag must lift: {calls:?}"
        );
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].input["file_path"], "reverse.py");
        assert_eq!(
            calls[0].input["content"], "def reverse_string(s):\n    return s[::-1]",
            "the \\n must be unescaped to a real newline so the file is valid"
        );
    }

    // what this catches: the precision guard — HTML void elements must stay INERT. A tag name with
    // no `_` or `/` is not a tool; `<br/>`, `<img .../>`, `<input .../>` must never lift as calls.
    #[test]
    pub(super) fn self_closing_html_void_elements_stay_inert() {
        for html in [
            "here is a break <br/> and more",
            "an image <img src=\"cat.png\" alt=\"a cat\" />",
            "<input type=\"text\" name=\"q\" />",
            "<meta charset=\"utf-8\" />",
        ] {
            assert!(
                parse_tool_calls(html).is_empty(),
                "HTML void element must not lift as a tool call: {html:?}"
            );
        }
    }

    // what this catches: a rust-tagged fence with first-person compile/run intent
    // and NO filename lifts into code/run — the gap between NarratedWriteFormat
    // (needs a filename) and the shell arm (needs a shell fence). Anwen's live
    // reverse-string message, verbatim shape (prompt-captures 2026-07-10): three
    // [unfulfilled] promises on card 34d8aff7 before this arm existed.
    #[test]
    pub(super) fn narrated_rust_fence_with_run_intent_lifts_into_code_run() {
        let text =
            "Let me proceed with card 34d8aff7 and write the code to reverse a string in Rust.

```rust
fn reverse_string(s: &str) -> String {
    s.chars().rev().collect()
}
```

I'll compile and run this function to ensure it works correctly.";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the rust fence lifts once: {calls:?}");
        assert_eq!(calls[0].name, "code/run");
        assert_eq!(calls[0].input["lang"], "rust");
        assert!(calls[0].input["code"]
            .as_str()
            .unwrap()
            .contains("reverse_string"));
        // A python fence with the same intent does NOT lift — code/run is the Rust
        // organism's hand; a guaranteed-useless call is worse than the honest
        // [unfulfilled] proprioception the Speak arm records.
        let py = "I'll run this now.

```python
print('hi')
```";
        assert!(
            parse_tool_calls(py).is_empty(),
            "non-rust fences stay unlifted"
        );
        // A bare example fence with no intent framing stays inert.
        let example = "Here's how reverse looks in Rust:

```rust
fn r() {}
```";
        assert!(parse_tool_calls(example).is_empty(), "no intent, no lift");
    }

    // what this catches: key-spelling tolerance on structured calls. Asha's live
    // 2026-07-10 claim — her FIRST structured emission after a day of narration —
    // used {"function","params"} and the strict {"name","arguments"} parser
    // rejected it on spelling alone. All common spellings normalize; prose JSON
    // without a name+args shape still never false-parses.
    #[test]
    pub(super) fn function_params_key_spellings_normalize() {
        let asha = "I'll claim a card for my Conway project.\n```json\n{\"function\": \"work/claim\", \"params\": {\"card_id\": \"33a0e899\"}}\n```";
        let calls = parse_tool_calls(asha);
        assert_eq!(calls.len(), 1, "her structured claim must lift: {calls:?}");
        assert_eq!(calls[0].name, "work/claim");
        assert_eq!(calls[0].input["card_id"], "33a0e899");
        // {"tool","input"} spelling too.
        let alt = "{\"tool\": \"code/read\", \"input\": {\"file_path\": \"a.rs\"}}";
        assert_eq!(parse_tool_call(alt).unwrap().name, "code/read");
        // Ordinary JSON data (no name/args shape) still never false-parses.
        assert!(parse_tool_calls("{\"count\": 3, \"items\": []}").is_empty());
    }

    // what this catches: a PAST-TENSE first-person execution claim after the fence
    // lifts too — Anwen's live fabrication (2026-07-10, card 34d8aff7): posted a
    // complete program, claimed "I have already run this program and received the
    // following output:", and invented WRONG output which her teammate validated.
    // Running the claim puts the REAL output in front of her before the
    // fabrication becomes shared room truth. Peer-addressed past tense stays inert.
    #[test]
    pub(super) fn past_tense_execution_claim_after_fence_lifts_and_grounds_the_fabrication() {
        let text = "```rust\nfn reverse_string(s: &str) -> String {\n    s.chars().rev().collect()\n}\nfn main() {\n    println!(\"{}\", reverse_string(\"continuum\"));\n}\n```\nI have already run this program and received the following output:\n```\nmuitnednoc\n```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the claimed-run fence lifts: {calls:?}");
        assert_eq!(calls[0].name, "code/run");
        assert!(calls[0].input["code"].as_str().unwrap().contains("fn main"));
        // Reviewing a PEER's claimed run stays quotation, never execution.
        let peer = "```rust\nfn main() {}\n```\nYou've run this already and it worked, right?";
        assert!(
            parse_tool_calls(peer).is_empty(),
            "peer-addressed past tense never lifts"
        );
    }

    // what this catches: the safety line. A REVIEW of a peer's work quotes commands
    // in fences — executing them would run someone else's (possibly wrong) code on
    // her key. Second-person / request framing must never lift. (Anwen's live review
    // message, verbatim shape.)
    #[test]
    pub(super) fn peer_review_and_request_fences_never_lift() {
        let review = "It looks like you've written and compiled the Rust code for the word \
frequency program. Now, let's run the program with the sample text file to see the output.\n\n\
```bash\n./wordstats sample.txt\n```\n\n\
Please provide the output so I can review it.";
        assert!(
            parse_tool_calls(review).is_empty(),
            "review-quoted fence must not execute"
        );
        let request = "Could you run this for me?\n```bash\nls -la\n```";
        assert!(
            parse_tool_calls(request).is_empty(),
            "a request is not my intent"
        );
    }

    // what this catches: bare example fences with no intent framing are teaching
    // material, not action — and non-shell fences without creation framing stay inert.
    #[test]
    pub(super) fn example_fences_without_intent_stay_inert() {
        let example = "Here's how you would count words in bash:\n```bash\nwc -w file.txt\n```";
        assert!(parse_tool_calls(example).is_empty());
        let plain = "The algorithm looks like:\n```rust\nfn main() {}\n```";
        assert!(parse_tool_calls(plain).is_empty());
        // A precise format present anywhere still wins outright (most-specific-first).
        let with_envelope = "I'll run it.\n```bash\nls\n```\n\
{\"tool_call\": {\"name\": \"ping\", \"arguments\": {}}}";
        let calls = parse_tool_calls(with_envelope);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "ping");
    }

    // what this catches: the sample.txt corruption (#122, glass-boxed live 2026-07-09).
    // Anwen narrated "I'll use code/write to create sample.txt" then fenced the RENDERED
    // call `code/write({"content": …, "file_path": …})` — and the whole envelope got
    // written verbatim INTO the file as content. A fence body that IS a tool call must
    // be recovered AS the call, not wrapped; a fence of ordinary code must still write.
    #[test]
    pub(super) fn fenced_rendered_call_is_recovered_not_written_as_content() {
        let corrupting = "I'll create a file called `sample.txt`:\n\
```\ncode/write({\n  \"content\": \"hello world\",\n  \"file_path\": \"work-x/sample.txt\"\n})\n```";
        let calls = parse_tool_calls(corrupting);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(
            calls[0].input["content"], "hello world",
            "content is the INNER text, not the envelope"
        );
        assert_eq!(calls[0].input["file_path"], "work-x/sample.txt");
        // A fenced ENVELOPE with intent narration is likewise recovered as the call.
        let enveloped = "Let me write it:\n\
```json\n{\"tool_call\": {\"name\": \"code/write\", \"arguments\": {\"file_path\": \"a.rs\", \"content\": \"fn main(){}\"}}}\n```";
        let c2 = parse_tool_calls(enveloped);
        assert_eq!(c2.len(), 1);
        assert_eq!(c2[0].input["content"], "fn main(){}");
        // BUT a fence of ordinary code under creation framing still writes verbatim.
        let real_file = "I'll create a file called `lib.rs`:\n\
```rust\npub fn add(a: i32, b: i32) -> i32 { a + b }\n```";
        let c3 = parse_tool_calls(real_file);
        assert_eq!(c3.len(), 1);
        assert_eq!(c3[0].name, "code/write");
        assert_eq!(c3[0].input["file_path"], "lib.rs");
        assert!(c3[0].input["content"]
            .as_str()
            .unwrap()
            .contains("pub fn add"));
    }

    // what this catches: the file-authoring idiom coverage gap (#122, glass-boxed
    // Asha 2026-07-10). "Here's the code for `reverse.rs`:" + fence, and "I've saved
    // this to `reverse.rs`", must lift to code/write — the old detector only matched
    // "file called/named X", so her save was confabulated (asserted, never executed).
    // Prose in backticks (`the value`) must NOT be mistaken for a filename.
    #[test]
    pub(super) fn backtick_filename_under_author_intent_lifts_to_write() {
        let asha = "Sure thing! Here's the code for `reverse.rs`:\n\
```rust\nuse std::env;\nfn main() { let a: Vec<String> = env::args().collect(); println!(\"{}\", a[1].chars().rev().collect::<String>()); }\n```";
        let c = parse_tool_calls(asha);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "code/write");
        assert_eq!(c[0].input["file_path"], "reverse.rs");
        assert!(c[0].input["content"]
            .as_str()
            .unwrap()
            .contains("env::args"));
        // A subdirectory path filename lifts too.
        let sub = "I'll write the program. Here's `work-x/main.rs`:\n```rust\nfn main(){}\n```";
        assert_eq!(
            parse_tool_calls(sub)[0].input["file_path"],
            "work-x/main.rs"
        );
        // Prose in backticks with intent words nearby must NOT become a file write.
        let prose =
            "I'll write a clear explanation of `the design` for you.\n```text\nsome notes\n```";
        assert!(
            parse_tool_calls(prose).is_empty(),
            "backtick prose with no filename-shaped token must not lift as a write"
        );
    }

    // what this catches: EDIT is crucial (Joel 2026-07-10) — a persona that can only
    // rewrite whole files can't make surgical changes. The generalized call-form
    // recovery lifts ANY rendered `category/verb({json})` in a fence, so code/edit,
    // code/shell, code/read all work when narrated as a call — not just code/write.
    #[test]
    pub(super) fn any_rendered_tool_call_in_a_fence_lifts_including_edit() {
        let edit = "I'll apply the fix with code/edit:\n\
```\ncode/edit({\"file_path\": \"wordstats.rs\", \"search\": \"replace(x)\", \"replace\": \"filter(y)\"})\n```";
        let c = parse_tool_calls(edit);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "code/edit");
        assert_eq!(c[0].input["file_path"], "wordstats.rs");
        assert_eq!(c[0].input["search"], "replace(x)");
        assert_eq!(c[0].input["replace"], "filter(y)");
        // A rendered code/read call likewise lifts.
        let read = "Let me read it:\n```\ncode/read({\"file_path\": \"lib.rs\"})\n```";
        let r = parse_tool_calls(read);
        assert_eq!(r[0].name, "code/read");
        // Prose that merely mentions a slash isn't a call — `and/or(this)` never lifts.
        assert!(parse_tool_calls("I'll consider this and/or(that) approach.").is_empty());
    }

mod part_2;

mod part_3;

