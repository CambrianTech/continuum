use super::*;

impl ToolCallFormat for NarratedWriteFormat {
    fn id(&self) -> &'static str {
        "narrated-write"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
        if !text.contains("code/write") {
            return Vec::new();
        }
        let Some(content) = extract_first_code_fence(text) else {
            return Vec::new();
        };
        if content.trim().is_empty() {
            return Vec::new();
        }
        // The fence is itself the rendered `code/write(...)` call — recover it AS the
        // call (real content + path) rather than writing the envelope verbatim into
        // the file (the sample.txt corruption, #122). Only when the WHOLE naive path
        // would otherwise mis-extract `file_path` FROM the envelope text.
        if let Some(call) = recover_call_from_fence_body(&content, resolve_name) {
            return vec![call];
        }
        let Some(path) = extract_file_path(text) else {
            return Vec::new();
        };
        vec![ToolCall {
            id: format!("jip-{}", Uuid::new_v4()),
            name: "code/write".to_string(),
            input: serde_json::json!({ "file_path": path, "content": content }),
        }]
    }
}

/// A model that NARRATES a whole working session — prose steps, each followed by a
/// fenced block — instead of emitting any machine-readable call. Observed live from
/// Devstral-Small-2507 (2026-07-09, the wordstats card): "I'll create a file called
/// `wordstats.rs` with the following content: ```rust …```", "Next, I'll compile:
/// ```bash rustc wordstats.rs```", "I'll paste the output here once it's ready" —
/// parsed as Speak, NOTHING executed, and two personas then hallucinated a shared
/// workspace neither had touched. The `[Acting]` prompt block already scolds exactly
/// this in prose; telling doesn't work — the parser meets the idiom (the same
/// differentiator as `<tools>`-in-content and the `[PASS]` bracket fix).
///
/// LAST-RESORT format: walks the message's fences IN ORDER and lifts each into a
/// real call — but ONLY when the narration immediately before the fence is
/// FIRST-PERSON INTENT ("I'll run…", "Let me try…", "Next, I'll compile…"):
/// - creation framing ("create/write a file called `X`") + any fence → `code/write`
/// - shell-shaped fence (```bash/sh/zsh, or first token a known command) → `code/shell`
///
/// The safety line (each arm test-pinned): second-person / request framing near the
/// fence ("you've written…", "please provide…", "could you run…") NEVER lifts —
/// reviewing a peer's code must not execute it; bare example fences with no intent
/// framing NEVER lift. No privilege expansion: the executor ACL gates every lifted
/// call identically to a hand-written envelope.
///
/// KNOWN LIMITATION (Joel 2026-07-09, accepted as a bridge): the intent/veto
/// predicates are English string tables — locked to English, brittle to phrasing
/// (#70/#124 string-matching smell). The durable exits, in preference order:
/// (1) the LoRA loop trains the narration reflex toward the canonical envelope,
///     making this format VESTIGIAL — its live hit-rate (telemetry id
///     "narrated-script") is the signal it can be retired;
/// (2) if narrated lifting must persist, intent classification moves to the
///     embedding/classifier layer (semantic, language-neutral), not string tables.
/// Until then her hands work when she describes the work —
/// [[local-first-tool-call-robustness-is-the-differentiator]].
pub(super) struct NarratedScriptFormat;

impl ToolCallFormat for NarratedScriptFormat {
    fn id(&self) -> &'static str {
        "narrated-script"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
        let mut out = Vec::new();
        for fence in fenced_blocks(text) {
            let narration = trailing_narration(&text[..fence.open]);
            // Peer-addressed framing (a review, a request) vetoes ANY lift — never
            // execute quoted code on someone's behalf.
            if addressed_to_peer(&narration) {
                continue;
            }
            // The fence body is ITSELF a rendered tool call — the model illustrated
            // the CALL inside a fence rather than emitting it bare. Lift THAT, not the
            // envelope-as-content. Glass-boxed live 2026-07-09: Anwen narrated "I'll use
            // code/write to create sample.txt" then fenced `code/write({"content": …,
            // "file_path": …})`, and the whole envelope got written verbatim INTO
            // sample.txt as content. Recover the real intent instead of corrupting the
            // file. (Precise formats only — a fence of ordinary code never false-parses
            // as an envelope: it needs the `{"tool_call"|"name","arguments"}` shape or a
            // `code/write(` call form.)
            if let Some(call) = recover_call_from_fence_body(&fence.body, resolve_name) {
                out.push(call);
                continue;
            }
            if let Some(path) = extract_created_file_name(&narration) {
                if !fence.body.trim().is_empty() {
                    out.push(ToolCall {
                        id: format!("jip-{}", Uuid::new_v4()),
                        name: "code/write".to_string(),
                        input: serde_json::json!({ "file_path": path, "content": fence.body }),
                    });
                }
                continue;
            }
            // Shell execution needs EXPLICIT first-person intent ("I'll run…") — a
            // shell command is higher-stakes than writing a file she's presenting, so
            // it never lifts from a bare fence (file-authoring is gated by the
            // authoring framing inside extract_created_file_name above).
            if first_person_intent(&narration)
                && fence_is_shell(&fence)
                && !fence.body.trim().is_empty()
            {
                out.push(ToolCall {
                    id: format!("jip-{}", Uuid::new_v4()),
                    name: "code/shell".to_string(),
                    input: serde_json::json!({ "cmd": fence.body.trim() }),
                });
                continue;
            }
            // A rust-tagged fence with the same explicit first-person RUN intent and
            // no filename lifts into `code/run` — the standalone "does this little
            // program do what I think?" hand. Glass-boxed live 2026-07-10: Anwen's
            // "I'll compile and run this function" + ```rust fence fell BETWEEN
            // NarratedWriteFormat (no filename) and the shell arm (not a shell
            // fence), leaving three [unfulfilled] promises on the reverse-string
            // card. A snippet without `fn main` compile-fails HONESTLY — she sees
            // rustc's error next tick and corrects (errors as data, never silence).
            // rust/rs ONLY: code/run is the Rust organism's hand and fails loud on
            // other languages, so a ```python fence stays an [unfulfilled] promise
            // rather than a guaranteed-useless call.
            let post = leading_narration(&text[fence.end.min(text.len())..]);
            let run_signal = (first_person_intent(&narration)
                || first_person_intent(&post)
                || first_person_execution_claim(&post)
                || first_person_execution_claim(&narration))
                && !addressed_to_peer(&narration)
                && !addressed_to_peer(&post);
            if run_signal
                && matches!(fence.lang.as_str(), "rust" | "rs")
                && !fence.body.trim().is_empty()
            {
                out.push(ToolCall {
                    id: format!("jip-{}", Uuid::new_v4()),
                    name: "code/run".to_string(),
                    input: serde_json::json!({ "lang": "rust", "code": fence.body }),
                });
            }
        }
        out
    }
}

/// A fenced block whose CONTENT is a rendered tool call (the model illustrated the
/// call inside a fence). Recover the actual call via the precise formats + the
/// `code/write("json")` prose call-form. Returns `None` for ordinary code/text — so
/// a fence of real source is never mistaken for a call.
pub(super) fn recover_call_from_fence_body(body: &str, resolve_name: ResolveName) -> Option<ToolCall> {
    // Precise object formats first (envelope / tagged / bare) — the machine-readable
    // shapes; ordinary code lacks the `{"tool_call"…}` / `{"name","arguments"}` shape.
    if let Some(c) = EnvelopeFormat
        .parse(body, resolve_name)
        .into_iter()
        .chain(TaggedFormat.parse(body, resolve_name))
        .chain(BareFormat.parse(body, resolve_name))
        .next()
    {
        return Some(c);
    }
    // The `<tool/name>({ … })` call-FORM: a JSON object wrapped in a
    // `category/verb( … )` invocation the model rendered as text. Recover it as a
    // call to THAT tool with the inner object as arguments — so code/write,
    // code/edit, code/shell, code/read, etc. ALL lift when narrated this way, not
    // just code/write (edit is crucial — a persona that can only rewrite whole
    // files can't do surgical changes). The name must look like a real tool path
    // (`a/b`, lowercase + slash) so prose like `foo(x)` never false-parses.
    let trimmed = body.trim();
    if let Some(open) = trimmed.find('(') {
        let name = trimmed[..open].trim();
        let looks_like_tool = name.contains('/')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '/' || c == '_' || c == '-');
        if looks_like_tool {
            let after = &trimmed[open + 1..];
            let obj_end = after.rfind(')').unwrap_or(after.len());
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(after[..obj_end].trim()) {
                if v.is_object() {
                    return Some(ToolCall {
                        id: format!("jip-{}", Uuid::new_v4()),
                        name: name.to_string(),
                        input: v,
                    });
                }
            }
        }
    }
    None
}

/// Does this text NARRATE an action the speaker intends to take — a fence preceded
/// by first-person intent, not addressed to a peer — regardless of whether any
/// format could LIFT it? The act→observe Speak arm uses this as the backstop: a
/// Speak that narrates action which nothing executed gets an `[unfulfilled]`
/// proprioception line, so next tick she perceives her own unkept promise instead
/// of believing the work happened (the shared-hallucinated-workspace failure,
/// 2026-07-09). Broader than the lift conditions on purpose — e.g. a ```python
/// fence with "I'll run this" is narrated action we can't lift into `code/shell`.
pub fn narrates_fenced_action(text: &str) -> bool {
    fenced_blocks(text).iter().any(|f| {
        let narration = trailing_narration(&text[..f.open]);
        first_person_intent(&narration) && !addressed_to_peer(&narration)
    })
}

/// Does this text end its intent in a theatrical STAGE DIRECTION — a line that is
/// exactly a bracketed present-participle phrase, `[writing test files]` /
/// `[creating test files]`? The fence-less sibling of [`narrates_fenced_action`]:
/// glass-boxed live 2026-07-10, Atlas looped "I'll create three test files …
/// [writing test files]" for over an hour — no fence, so no format could lift it
/// and the fenced backstop never fired; the intent stayed unsatisfied and he
/// re-declared it every turn. The bracket-gerund shape is pure geometry: it never
/// collides with the substrate's own bracket tags (`[t=…]`, `[recall]`,
/// `[action #n]`, `[unfulfilled]` — none open with a gerund).
/// Does this text contain any fenced block at all? Cheap public predicate for
/// the unverified-artifact backstop (#134/Joel 2026-07-11: under verification
/// pressure a persona upgraded from stage directions to CONFABULATED file
/// contents — plausible fenced "artifacts" no tool ever produced. Prose can't
/// distinguish a draft from a claim; the turn-level evidence can).
pub fn has_fenced_block(text: &str) -> bool {
    !fenced_blocks(text).is_empty()
}

/// Does this text CLAIM a past tool execution in the first person — "I ran
/// `commands/list` and got…", "The tool returned this poem"? The PAST-TENSE
/// sibling of [`narrates_fenced_action`] (#144): glass-boxed live 2026-07-11/12
/// when a persona presented self-authored poems as the output of a `gpt-4` run
/// that never happened (log-verified: zero ai/inference/generate invocations),
/// and a peer adopted the fabricated result as room truth. The caller gates on
/// working memory: a claim WITH matching `[action #n]` receipts is honest
/// reporting; a claim with ZERO acts this concern is confabulated execution.
/// Precision-first: quoted lines, substrate tags, second-person coaching, and
/// hypothetical framing never fire; first-person forms additionally require a
/// tool-shaped token (backticked name or slash-path) so "I ran fast" is inert.
pub fn claims_past_tool_run(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        // Quoted/relayed lines and substrate bracket-tags are never her claim.
        if l.starts_with('>') || l.starts_with('[') {
            return false;
        }
        let lower = l.to_lowercase();
        // Hypothetical / instructional framing is not a claim of execution.
        if lower.contains("if i ran")
            || lower.contains("if you run")
            || lower.contains("would return")
            || lower.contains("here's how")
            || lower.contains("you can use")
        {
            return false;
        }
        const RESULT_CLAIMS: &[&str] = &[
            "the tool returned",
            "the command returned",
            "it returned this",
            "and got this list",
            "and got this result",
            "here's the output i got",
            // Presenting file/command contents as an accomplished fact —
            // Casper live 2026-07-12: "I have initialized a new Rust project
            // ... Here are the contents of the `Cargo.toml` file:" + a fully
            // FABRICATED toml (the real crate was two days old and different).
            "here are the contents of",
        ];
        if RESULT_CLAIMS.iter().any(|p| lower.contains(p)) {
            return true;
        }
        const FIRST_PERSON_PAST: &[&str] = &[
            "i ran ",
            "i executed ",
            "i've run ",
            "i have run ",
            "i've executed ",
            "i just ran ",
            "i called ",
            // The creation/initialization family (same live incident): claims
            // of having scaffolded/created something. The tool-shaped-token
            // gate below keeps benign retrospectives ("I created the plan")
            // inert — they carry no backticked/slash token.
            "i have initialized ",
            "i initialized ",
            "i've initialized ",
            "i have created ",
            "i've created ",
            "i have set up ",
        ];
        let claims = FIRST_PERSON_PAST.iter().any(|p| {
            lower.starts_with(p)
                || lower.contains(&format!(". {p}"))
                || lower.contains(&format!("! {p}"))
        });
        if !claims {
            return false;
        }
        // Require a tool-shaped token: backticked name or a slash-path word.
        l.contains('`')
            || l.split_whitespace()
                .any(|w| w.contains('/') && w.len() > 3 && !w.starts_with("http"))
    })
}

pub fn narrates_stage_direction(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        let Some(inner) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
            return false;
        };
        // Stage directions are short standalone phrases, not prose in brackets.
        if inner.len() > 60 || inner.contains('[') || inner.contains(']') {
            return false;
        }
        let Some(head) = inner.split_whitespace().next() else {
            return false;
        };
        head.len() >= 4 && head.ends_with("ing") && head.chars().all(|c| c.is_ascii_alphabetic())
    })
}

/// One ```…``` fenced block: byte offset of the opening fence, the language token
/// (lowercased, may be empty), and the body with the language line stripped.
pub(super) struct FencedBlock {
    pub(super) open: usize,
    /// Byte offset just past the CLOSING fence — where any trailing narration
    /// ("I have already run this…") begins.
    pub(super) end: usize,
    pub(super) lang: String,
    pub(super) body: String,
}

/// All fenced blocks in `text`, in order. An unterminated final fence is ignored
/// (half-written code is not an executable intent).
pub(super) fn fenced_blocks(text: &str) -> Vec<FencedBlock> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut base = 0usize;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let (lang, body_start) = match after.find('\n') {
            Some(nl) => (after[..nl].trim().to_lowercase(), nl + 1),
            None => break,
        };
        let body_zone = &after[body_start..];
        let Some(close) = body_zone.find("```") else {
            break;
        };
        let consumed = open + 3 + body_start + close + 3;
        out.push(FencedBlock {
            open: base + open,
            end: base + consumed,
            lang,
            body: body_zone[..close].to_string(),
        });
        base += consumed;
        rest = &rest[consumed..];
    }
    out
}

/// The narration IMMEDIATELY before a fence: the text after the previous fence (or
/// message start), bounded to the last few lines so early-message framing can't
/// leak intent onto a distant fence.
pub(super) fn trailing_narration(before: &str) -> String {
    let after_prev_fence = match before.rfind("```") {
        Some(idx) => &before[idx + 3..],
        None => before,
    };
    let lines: Vec<&str> = after_prev_fence.lines().rev().take(4).collect();
    lines
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

/// The narration IMMEDIATELY after a fence: the first few lines before the next
/// fence (or message end). Where a model states what it claims it DID with the
/// code it just presented ("I have already run this program and received: …").
pub(super) fn leading_narration(after: &str) -> String {
    let before_next_fence = match after.find("```") {
        Some(idx) => &after[..idx],
        None => after,
    };
    before_next_fence
        .lines()
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

/// Does this narration CLAIM the speaker already executed the thing? Past-tense
/// first-person execution ("I have already run…", "I ran it and got…"). Glass-
/// boxed live 2026-07-10 (card 34d8aff7): Anwen posted a complete program, wrote
/// "I have already run this program and received the following output:", and
/// fabricated the output — WRONG output ("muitnednoc" for reverse("continuum"),
/// letters not even in the input) — which her teammate then validated as correct.
/// Lifting the claim RUNS the code, so the real output reaches her before the
/// fabrication reaches the room: the substrate makes her words true or shows her
/// they weren't. Same veto as intent: peer-addressed narration never lifts.
pub(super) fn first_person_execution_claim(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "i have already run",
        "i have run ",
        "i've run ",
        "i've already run",
        "i ran ",
        "i have executed",
        "i've executed",
        "i executed ",
        "i compiled and ran",
        "i have compiled and run",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Is this narration the SPEAKER's own stated intent to do the thing? Markers are
/// first-person-future doing ("I'll…", "let me…", "next, i…"). This is the positive
/// gate; [`addressed_to_peer`] is the veto.
pub(super) fn first_person_intent(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "i'll ",
        "i will ",
        "i'm going to",
        "i am going to",
        "let me ",
        "let's ",
        "next, i",
        "now, i",
        "first, i",
        "then, i",
        "finally, ",
        "now, let",
        "next, let",
        "finally, let",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Is this narration aimed at SOMEONE ELSE doing/having done the thing? A request
/// or a review of a peer's work must never execute — the fence there is quotation.
pub(super) fn addressed_to_peer(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "you've ",
        "you have ",
        "you wrote",
        "you ran",
        "your code",
        "please ",
        "could you",
        "would you",
        "can you ",
        "here's how you",
        "here is how you",
        "you would",
        "you can ",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Extract the target file name from file-authoring narration. Two shapes:
///   1. explicit "create/write a file called|named `X`"
///   2. any save/write/create intent + a BACKTICK-QUOTED filename-with-extension
///      anywhere in the narration (`reverse.rs`, `lib.rs`), e.g. "Here's the code for
///      `reverse.rs`:", "I've saved this to `reverse.rs`", "the `wordstats.rs` program".
/// Shape 2 was added after glass-boxing Asha 2026-07-10: she narrated "Here's the code
/// for `reverse.rs`" + a fence and asserted "I've saved this to reverse.rs" — but the
/// old detector only matched "file called/named", so nothing lifted and the save was
/// confabulated. Meeting the idiom ([[local-first-tool-call-robustness-is-the-differentiator]]).
/// None → not a file authoring (the shell/other paths handle it).
pub(super) fn extract_created_file_name(narration: &str) -> Option<String> {
    const AUTHOR_INTENT: &[&str] = &[
        "create",
        "write",
        "save",
        "here's the",
        "here is the",
        "code for",
        "program",
    ];
    if !AUTHOR_INTENT.iter().any(|k| narration.contains(k)) {
        return None;
    }
    // Shape 1: "file called|named `X`".
    if let Some(idx) = narration
        .find("file called")
        .map(|i| i + "file called".len())
        .or_else(|| narration.find("file named").map(|i| i + "file named".len()))
    {
        let after = narration[idx..].trim_start();
        let name: String = after
            .trim_start_matches(['`', '"', '\''])
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '`' | '"' | '\'' | ',' | ':' | ';'))
            .collect();
        if !name.is_empty() && name.contains('.') {
            return Some(name);
        }
    }
    // Shape 2: the first backtick-quoted `filename.ext` token in the narration. A
    // filename = has a dot with a short alphanumeric extension and no path separator
    // spaces, so prose in backticks (`the value`) is not mistaken for a file.
    for seg in narration.split('`').skip(1).step_by(2) {
        let tok = seg.trim();
        if let Some((stem, ext)) = tok.rsplit_once('.') {
            let ext_ok =
                !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric());
            let stem_ok = !stem.is_empty()
                && !tok.contains(' ')
                && tok
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
            if ext_ok && stem_ok {
                return Some(tok.to_string());
            }
        }
    }
    None
}

/// Is this fence a runnable shell block? Either shell-tagged, or untagged with a
/// first token that is unmistakably a command (a `./binary` or a well-known CLI).
pub(super) fn fence_is_shell(fence: &FencedBlock) -> bool {
    if matches!(
        fence.lang.as_str(),
        "bash" | "sh" | "shell" | "zsh" | "console" | "terminal"
    ) {
        return true;
    }
    if !fence.lang.is_empty() {
        return false;
    }
    let first = fence
        .body
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| l.trim().split_whitespace().next())
        .unwrap_or("");
    first.starts_with("./")
        || matches!(
            first,
            "rustc"
                | "cargo"
                | "echo"
                | "ls"
                | "cat"
                | "python"
                | "python3"
                | "pip"
                | "git"
                | "make"
                | "mkdir"
                | "touch"
                | "grep"
                | "find"
                | "curl"
                | "sha256sum"
                | "shasum"
                | "npm"
                | "node"
        )
}

/// Pull `file_path`'s value out of prose: `file_path="x"`, `file_path=x`,
/// `file_path: x`, `file_path x`. Value ends at a quote, whitespace, or comma.
pub(super) fn extract_file_path(text: &str) -> Option<String> {
    let idx = text.find("file_path")?;
    let after = text[idx + "file_path".len()..].trim_start_matches([' ', '=', ':', '"', '\'']);
    let end = after
        .find(|c: char| c == '"' || c == '\'' || c.is_whitespace() || c == ',')
        .unwrap_or(after.len());
    let path = after[..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// The body of the FIRST ```…``` fenced block, with the optional language line stripped.
pub(super) fn extract_first_code_fence(text: &str) -> Option<String> {
    let open = text.find("```")?;
    let after = &text[open + 3..];
    // Skip the language token on the opening fence line (```rust\n → drop "rust\n").
    let body = match after.find('\n') {
        Some(nl) => &after[nl + 1..],
        None => after,
    };
    let close = body.find("```")?;
    Some(body[..close].to_string())
}

/// Scan `text` for balanced `{...}` objects (ignoring braces inside JSON strings),
/// applying `f` to each; collect the `Some` results, skipping past a matched object
/// so its insides aren't re-scanned. A malformed (unbalanced) object is skipped and
/// scanning resumes after it. Shared by the object-based formats.
pub(super) fn scan_objects<F>(text: &str, f: F) -> Vec<ToolCall>
where
    F: Fn(&str) -> Option<ToolCall>,
{
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = matching_brace_end(bytes, i) {
                if let Some(call) = f(&text[i..=end]) {
                    out.push(call);
                    i = end + 1; // consume this object; don't re-scan its insides
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Index of the `}` matching the `{` at `start`, respecting JSON string literals
/// and escapes. `None` if unbalanced (truncated output).
pub(super) fn matching_brace_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}
