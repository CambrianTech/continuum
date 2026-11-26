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
}
