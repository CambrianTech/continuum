#![cfg(test)]
use super::*;

    // what this catches: Atlas's EXACT live script idiom (glass-boxed
    // 2026-07-22 twice — subtract narrated it; on multiply the whole script
    // got WRITTEN INTO mathlib.py as content, the correct fix trapped in a
    // string variable). An assignment-bound call (`result = code/read({...})`)
    // is real intent behind python fiction: the binding strips, the call
    // lifts. Sibling lines hold the guards: the `code/write` whose args
    // reference an UNQUOTED variable (`new_content`) refuses (invalid JSON —
    // she must read for real first), `print(...)` and `new_content = """..."""`
    // stay speech. And because CliFlagFormat precedes NarratedWrite, the fence
    // is a SCRIPT — parse_tool_calls yields the calls, never a file-write of
    // the script text.
    #[test]
    pub(super) fn assignment_bound_calls_lift_from_live_script_emission() {
        let live = r#"Let's fix the bug in `mathlib.py`:

```python
# Read the existing content of mathlib.py
result = code/read({"file_path": "mathlib.py"})
print(result["content"])

# Fix the bug in the multiply function
new_content = """
def multiply(a, b):
    return a * b
"""
code/write({"content": new_content, "file_path": "mathlib.py"})

# Confirm the fix by running the code
result = code/shell({"cmd": "python mathlib.py"})
print(result["stdout"])
```

Please run this script."#;
        let calls = parse_tool_calls(live);
        assert_eq!(
            calls.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["code/read", "code/shell"],
            "read + shell lift; the variable-arg write refuses: {calls:?}"
        );
        assert_eq!(calls[0].input["file_path"], "mathlib.py");
        assert_eq!(calls[1].input["cmd"], "python mathlib.py");
    }

    // what this catches: the assignment strip is NARROW — ordinary python
    // bindings whose callee resolves in NO registry stay speech, comparisons
    // are not bindings, and a dotted/complex lhs is left alone.
    #[test]
    pub(super) fn assignment_strip_stays_narrow() {
        for speech in [
            "x = compute({\"y\": 1})",
            "if x == is_even({\"n\": 2}): pass",
            "self.result = code_read({\"file_path\": \"x\"})",
            "new_content = \"\"\"\ndef f():\n    pass\n\"\"\"",
        ] {
            assert!(
                parse_tool_calls(speech).is_empty(),
                "must stay speech: {speech}"
            );
        }
    }

    // what this catches: a call written as text leaving with her words into a room line, where a
    // roommate copies it as her own act (the IntelMac, 2026-11-22: a 1.5B resident replayed a peer's
    // posted tool/output call with the peer's output id). The fenced call and a bare call object go;
    // her words, a fence holding plain code, and braces that are not a call stay.
    #[test]
    fn her_words_keep_everything_but_the_calls_written_in_them() {
        let fenced = "Let me reopen that output.\n\n```json\n{\"name\": \"tool/output\", \"arguments\": {\"outputId\": \"c29a5e36f8a7e3cf\"}}\n```\n";
        assert_eq!(super::without_calls(fenced, resolve_name), "Let me reopen that output.");
        let bare = "Reading it now {\"name\": \"code_read\", \"arguments\": {\"file_path\": \"a.rs\"}} then I edit.";
        assert_eq!(super::without_calls(bare, resolve_name), "Reading it now  then I edit.");
        let code = "The fix is:\n```rust\nfn f() { let x = {1}; }\n```";
        assert_eq!(super::without_calls(code, resolve_name), code);
        assert_eq!(super::without_calls("```json\n{\"name\": \"tool/output\", \"arguments\": {}}\n```", resolve_name), "");
    }
