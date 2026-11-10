// Standalone test for not_speech bracket dialects
// Extracted from core/continuum-prompt/src/not_speech.rs to avoid llama/metal build issues

fn verb_ok(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "/_-.".contains(c))
}

fn opens_with_tool_envelope(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix('[') {
        // Three bracket dialects reach the room, and each was found the hard way.
        // THE SECOND: while the first version of this gate was compiling, a citizen
        // posted `[code/read] {"file_path":"src/main.rs"}` — verb closed by `]`, object
        // after a space — which the comma form below does not match.
        if let Some((verb, after)) = rest.split_once(',') {
            if verb_ok(verb) && after.trim_start().starts_with('{') {
                return true;
            }
        }
        if let Some((verb, after)) = rest.split_once(']') {
            if verb_ok(verb) && after.trim_start().starts_with('{') {
                return true;
            }
            // `[execute] code/read(code=…` — stage word + call inside bracket
            let a = after.trim_start();
            let call_len = a.find(['(', '{', ' ', '\n']).unwrap_or(a.len());
            let callee = &a[..call_len];
            if verb_ok(verb)
                && verb_ok(callee)
                && callee.contains('/')
                && matches!(a[call_len..].chars().next(), Some('(') | Some('{'))
            {
                return true;
            }
            // THE THIRD DIALECT: bare namespaced [ns/verb] with no object
            if verb_ok(verb) && verb.contains('/') {
                return true;
            }
        }
        // THE FIFTH DIALECT (Saoirse): `[code/read({"file_path":"src/main.rs"})]`
        if let Some((verb, after)) = rest.split_once('(') {
            let a = after.trim_start();
            let kwargs = a
                .split_once('=')
                .is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            if verb_ok(verb) && verb.contains('/') && (a.starts_with('{') || kwargs) {
                return true;
            }
            // THE LABELLED CALL: `[call form: code/read({"file_path":…})]`
            let callee = verb.rsplit([' ', ':']).next().unwrap_or(""); 
            if callee != verb && verb_ok(callee) && callee.contains('/') && (a.starts_with('{') || kwargs) {
                return true;
            }
        }
    }
    false
}

fn main() {
    // Test 1: comma form — should be caught
    let t1 = "[code/read,{\"key\":\"val\"}]";
    println!("comma form: {} (expected: true)", opens_with_tool_envelope(t1));

    // Test 2: bracket-close + object — should be caught  
    let t2 = "[code/read] {\"file_path\":\"src/main.rs\"}";
    println!("bracket-close+obj: {} (expected: true)", opens_with_tool_envelope(t2));

    // Test 3: bare namespaced in brackets — should be caught
    let t3 = "[code/run]\n";
    println!("bare namespaced: {} (expected: true)", opens_with_tool_envelope(t3));

    // Test 4: FIFTH DIALECT — the problematic one
    let t4 = "[code/read({\"file_path\":\"src/main.rs\"})]";
    println!("fifth dialect: {} (expected: true, CURRENTLY: {})", 
             opens_with_tool_envelope(t4), opens_with_tool_envelope(t4));

    // Test 5: labelled call — should be caught
    let t5 = "[call form: code/read({\"file_path\":\"src/main.rs\"})]";
    println!("labelled call: {} (expected: true)", opens_with_tool_envelope(t5));

    // Test 6: stage word + call inside bracket — should be caught
    let t6 = "[execute] code/read(cmd=\"ls\")";
    println!("stage+call: {} (expected: true)", opens_with_tool_envelope(t6));

    // Test 7: non-namespaced verb in brackets — should NOT be caught
    let t7 = "[answer] this is speech";
    println!("non-ns verb: {} (expected: false)", opens_with_tool_envelope(t7));

    // Test 8: KWARGS form — should be caught
    let t8 = "[code/shell(cmd=ls, lang=rust)]";
    println!("kwargs: {} (expected: true)", opens_with_tool_envelope(t8));
}
