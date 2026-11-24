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

/// Why a lesson could not be rendered: it reaches the trainer as the step's refusal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LessonRefusal {
    #[error("the lesson has no reply: its last message is not hers")]
    NoReply,
    #[error("her chat template refused the lesson: {0}")]
    Template(String),
    #[error("the rendered reply adds no tokens past her context")]
    EmptyReply,
}

/// Render `lesson` with `render` (her chat template: a request in, its prompt out) and `tokenize`.
pub fn render(
    lesson: &Lesson,
    render: impl Fn(&serde_json::Value) -> Result<String, String>,
    tokenize: impl Fn(&str) -> Result<Vec<i32>, String>,
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
    let seen = render(&request(context, true)).map_err(LessonRefusal::Template)?;
    let whole = render(&request(&lesson.messages, false)).map_err(LessonRefusal::Template)?;
    let seen = tokenize(&seen).map_err(LessonRefusal::Template)?;
    let tokens = tokenize(&whole).map_err(LessonRefusal::Template)?;
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
    fn template(request: &serde_json::Value) -> Result<String, String> {
        let mut out = String::new();
        for message in request["messages"].as_array().ok_or("no messages")? {
            out.push_str(&format!("<{}>{}</{}>", message["role"].as_str().unwrap_or("?"), message["content"].as_str().unwrap_or(""), message["role"].as_str().unwrap_or("?")));
        }
        if request["add_generation_prompt"] == true {
            out.push_str("<assistant>");
        }
        Ok(out)
    }

    /// One token per character.
    fn chars(text: &str) -> Result<Vec<i32>, String> {
        Ok(text.chars().map(|c| c as i32).collect())
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
        let reply: String = rendered.tokens[rendered.reply_from..].iter().map(|&t| char::from_u32(t as u32).unwrap_or('?')).collect();
        assert_eq!(reply, "Topeka</assistant>", "the reply and its end, never her context or the generation prompt");
    }

    // what this catches: a lesson whose last message is not hers trained as if it were.
    #[test]
    fn a_lesson_that_does_not_end_in_her_reply_is_refused() {
        let lesson = Lesson { messages: vec![serde_json::json!({"role": "user", "content": "capital?"})], tools: None };
        assert_eq!(render(&lesson, template, chars), Err(LessonRefusal::NoReply));
    }
}
