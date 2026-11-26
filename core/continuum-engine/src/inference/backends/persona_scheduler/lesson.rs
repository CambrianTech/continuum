//! A lesson as the engine learns it: her conversation rendered with her own chat template, and the
//! token where the reply she learns begins (IN-PROCESS-LEARNING.md, L2). Rendered on the submitter's
//! thread, as a turn's prompt is: the engine thread never renders or tokenizes.
//!
//! Two renders through the same template: her context with its generation prompt (what she saw
//! before she replied), and the whole conversation without one. The reply is what the whole adds
//! past their shared tokens, so the labels cover exactly her reply and its end, never the prompt.

use continuum_persona_engine::turn::common_prefix;

use crate::inference::learning::Lesson;

/// A lesson's tokens and where its reply begins in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub tokens: Vec<i32>,
    /// The first reply token; `tokens[..reply_from]` is the context she saw.
    pub reply_from: usize,
}

impl Rendered {
    /// The rows a seat learning it is sized for: its reply, from her last context token (whose logits
    /// predict the reply's first) to its end, and the whole lesson its attention reads. The one owner
    /// of this geometry: the measure sizes a seat by it and a step checks its lesson against it.
    pub fn rows(&self) -> Option<continuum_persona_engine::residency::LessonRows> {
        let whole = self.tokens.len().checked_add(1)?;
        let rows = |count: usize| u32::try_from(count).ok().and_then(std::num::NonZeroU32::new);
        continuum_persona_engine::residency::LessonRows::new(rows(whole.saturating_sub(self.reply_from))?, rows(whole)?)
    }
}

/// Why a lesson could not be rendered: it reaches the trainer as the step's refusal.
#[derive(Debug, thiserror::Error)]
pub enum LessonRefusal {
    #[error("the lesson has no reply: its last message is not hers")]
    NoReply,
    #[error("her chat template refused the lesson: {0}")]
    Chat(#[from] llama::ChatRefusal),
    #[error("her template described its render unreadably: {0}")]
    Described(serde_json::Error),
    #[error("her template rendered no prompt")]
    NoPrompt,
    #[error("the lesson could not be rendered or tokenized: {0}")]
    Template(String),
    #[error("the rendered reply adds no tokens past her context")]
    EmptyReply,
}

/// Render `lesson` with `render` (her chat template: a request in, its prompt out) and `tokenize`.
pub fn render(
    lesson: &Lesson,
    render: impl Fn(&serde_json::Value) -> Result<String, LessonRefusal>,
    tokenize: impl Fn(&str) -> Result<Vec<i32>, LessonRefusal>,
) -> Result<Rendered, LessonRefusal> {
    let Some((reply, context)) = lesson.messages.split_last() else { return Err(LessonRefusal::NoReply) };
    if reply["role"] != "assistant" {
        return Err(LessonRefusal::NoReply);
    }
    let request = |messages: &[serde_json::Value], generation: bool| {
        let mut request = serde_json::json!({ "messages": messages, "add_generation_prompt": generation });
        if let Some(tools) = &lesson.tools {
            request["tools"] = tools.clone();
        }
        request
    };
    let seen = render(&request(context, true))?;
    let whole = render(&request(&lesson.messages, false))?;
    let seen = tokenize(&seen)?;
    let tokens = tokenize(&whole)?;
    let reply_from = common_prefix(&seen, &tokens);
    if reply_from == 0 || reply_from >= tokens.len() {
        return Err(LessonRefusal::EmptyReply);
    }
    Ok(Rendered { tokens, reply_from })
}

/// Render `lesson` within `rows` (the whole lesson's rows a seat was loaded for: her context and the
/// reply its attention reads). Whole when it fits. Otherwise its older turns are collapsed through the
/// one context codec (`continuum_context::ladder`), oldest first, each to its summary
/// (`levels::summarize`, an output's head and tail) and then to a marker naming what it stood for.
/// Her framing (a leading system message), the newest input she answered and her reply are never
/// touched, and the stored lesson is not changed: this is a rendering of it. One still past `rows`
/// with every older turn at its marker is returned as it is, and the step's geometry check answers
/// TooLong (the 5090, 2026-11-26: a code lesson measured 1.9 TB; doctrine: a training example is a
/// budgeted prompt, collapsed, never truncated and never parked).
pub fn render_within(
    lesson: &Lesson,
    rows: std::num::NonZeroU32,
    template: impl Fn(&serde_json::Value) -> Result<String, LessonRefusal>,
    tokenize: impl Fn(&str) -> Result<Vec<i32>, LessonRefusal>,
) -> Result<Rendered, LessonRefusal> {
    use continuum_context::ladder::{Leveled, Rungs};
    use continuum_context::levels::{summarize, Shape};
    let fits = |rendered: &Rendered| rendered.rows().is_some_and(|got| got.whole() <= rows);
    let mut rendered = render(lesson, &template, &tokenize)?;
    if fits(&rendered) {
        return Ok(rendered);
    }
    // Her framing, the newest input and her reply stay whole: only the turns between them collapse.
    let first = usize::from(lesson.messages.first().is_some_and(|m| m["role"] == "system"));
    let newest = lesson.messages.len().saturating_sub(2);
    // (index, content, role) of each turn that may collapse: a turn without text content stays whole
    let older: Vec<(usize, &str, &str)> = (first..newest)
        .filter_map(|i| Some((i, lesson.messages[i]["content"].as_str()?, lesson.messages[i]["role"].as_str()?)))
        .collect();
    let cost = |text: &str| tokenize(text).and_then(|tokens| u32::try_from(tokens.len()).map_err(|_| LessonRefusal::Template("a level of more tokens than a seat can count".into())));
    let mut items = Vec::with_capacity(older.len());
    let mut shown = Vec::with_capacity(older.len());
    for &(_, text, role) in &older {
        let whole = cost(text)?;
        let mut smaller = Vec::new();
        if let Some(summary) = summarize(text, Shape::Output) {
            smaller.push((cost(&summary)?, Some(summary)));
        }
        let marker = format!("[{whole} tokens of an earlier {role} message, collapsed to fit what she can learn]");
        smaller.push((cost(&marker)?, Some(marker)));
        let (rungs, tags) = Rungs::kept(whole, None, smaller);
        items.push(rungs);
        shown.push(tags);
    }
    let leveled = Leveled::new(items);
    let mut target = leveled.want();
    let at_limit = leveled.at_limit();
    loop {
        let Some(got) = rendered.rows() else { return Ok(rendered) }; // no rows a seat can hold: the step refuses it
        let over = got.whole().get().saturating_sub(rows.get());
        target = target.saturating_sub(over); // at least one: a lesson within its rows returned above
        let choice = leveled.reduce(target);
        let mut messages = lesson.messages.clone();
        for (k, &(i, _, _)) in older.iter().enumerate() {
            if let Some(Some(text)) = shown[k].get(choice.rungs[k]) {
                messages[i]["content"] = serde_json::Value::String(text.clone());
            }
        }
        rendered = render(&Lesson { messages, tools: lesson.tools.clone() }, &template, &tokenize)?;
        if fits(&rendered) || choice == at_limit {
            return Ok(rendered);
        }
        target = choice.cost;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A template that joins each message as `<role>content</role>`, and opens an assistant turn
    /// when asked for a generation prompt.
    fn template(request: &serde_json::Value) -> Result<String, LessonRefusal> {
        let Some(messages) = request["messages"].as_array() else { return Err(LessonRefusal::Template("no messages".into())) };
        let mut out = String::new();
        for message in messages {
            let (Some(role), Some(content)) = (message["role"].as_str(), message["content"].as_str()) else {
                return Err(LessonRefusal::Template("a message without a role or content".into()));
            };
            out.push_str(&format!("<{role}>{content}</{role}>"));
        }
        if request["add_generation_prompt"] == true {
            out.push_str("<assistant>");
        }
        Ok(out)
    }

    /// One token per character.
    fn chars(text: &str) -> Result<Vec<i32>, LessonRefusal> {
        Ok(text.chars().map(|c| c as i32).collect())
    }

    /// The text of `tokens`, one character per token.
    fn text(tokens: &[i32]) -> String {
        tokens.iter().filter_map(|&t| u32::try_from(t).ok().and_then(char::from_u32)).collect()
    }

    // what this catches: labels that cover her prompt (the trainer would teach her what she was told)
    // or miss her reply's start (the generation prompt's tokens counted as hers).
    #[test]
    fn the_reply_begins_where_her_context_and_its_generation_prompt_end() {
        let lesson = Lesson {
            messages: vec![serde_json::json!({"role": "user", "content": "capital?"}), serde_json::json!({"role": "assistant", "content": "Topeka"})],
            tools: None,
        };
        let rendered = render(&lesson, template, chars).expect("test: renders");
        let reply = text(&rendered.tokens[rendered.reply_from..]);
        assert_eq!(reply, "Topeka</assistant>", "the reply and its end, never her context or the generation prompt");
    }

    // what this catches: a lesson whose last message is not hers trained as if it were.
    #[test]
    fn a_lesson_that_does_not_end_in_her_reply_is_refused() {
        let lesson = Lesson { messages: vec![serde_json::json!({"role": "user", "content": "capital?"})], tools: None };
        assert!(matches!(render(&lesson, template, chars), Err(LessonRefusal::NoReply)));
    }

    fn turn(role: &str, content: &str) -> serde_json::Value {
        serde_json::json!({ "role": role, "content": content })
    }

    /// Her framing, `older` long tool outputs, the newest input and her reply.
    fn long_lesson(older: usize) -> Lesson {
        let mut messages = vec![turn("system", "be brief")];
        for n in 0..older {
            let lines: String = (0..40).map(|l| format!("output {n} line {l}\n")).collect();
            messages.push(turn("tool", &lines));
        }
        messages.push(turn("user", "capital?"));
        messages.push(turn("assistant", "Topeka"));
        Lesson { messages, tools: None }
    }

    fn rows(count: usize) -> std::num::NonZeroU32 {
        u32::try_from(count).ok().and_then(std::num::NonZeroU32::new).expect("test: a nonzero row count")
    }

    // what this catches: a lesson that fits changed anyway (a collapse that ran with nothing over,
    // altering what she learns from).
    #[test]
    fn a_lesson_that_fits_is_rendered_whole() {
        let lesson = long_lesson(2);
        let whole = render(&lesson, template, chars).expect("test: renders");
        let within = render_within(&lesson, rows(whole.tokens.len() + 1), template, chars).expect("test: renders within");
        assert_eq!(within, whole);
    }

    // what this catches (the 5090, 2026-11-26: a code lesson measured 1.9 TB): a lesson past its seat
    // truncated, its framing, newest input or reply collapsed, or its labels moved off her reply.
    // Her older turns collapse, oldest first, until it fits.
    #[test]
    fn a_lesson_past_its_rows_collapses_its_oldest_turns_and_keeps_what_she_answered() {
        let lesson = long_lesson(6);
        let whole = render(&lesson, template, chars).expect("test: renders");
        let budget = rows(whole.tokens.len() / 2);
        let within = render_within(&lesson, budget, template, chars).expect("test: renders within");
        assert!(within.rows().is_some_and(|got| got.whole() <= budget), "fits: {} tokens for {budget} rows", within.tokens.len());
        let rendered = text(&within.tokens);
        assert!(rendered.starts_with("<system>be brief</system>"), "her framing whole");
        assert!(rendered.contains("<user>capital?</user>"), "the newest input whole");
        assert_eq!(text(&within.tokens[within.reply_from..]), "Topeka</assistant>", "her reply is exactly what is trained");
        assert!(rendered.contains("output 5 line 39"), "the newest older turn whole while older ones collapse");
        assert!(!rendered.contains("output 0 line 20"), "the oldest turn collapsed first");
    }

    // what this catches: a lesson that cannot fit with every older turn at its marker looping or
    // failing the render. It returns at its least; the step's check answers TooLong.
    #[test]
    fn a_lesson_that_cannot_fit_returns_at_its_least() {
        let lesson = long_lesson(3);
        let within = render_within(&lesson, rows(8), template, chars).expect("test: renders within");
        assert!(within.rows().is_some_and(|got| got.whole().get() > 8), "still past: the caller answers TooLong");
        assert_eq!(text(&within.tokens).matches("collapsed to fit").count(), 3, "every older turn at its marker");
    }
}
