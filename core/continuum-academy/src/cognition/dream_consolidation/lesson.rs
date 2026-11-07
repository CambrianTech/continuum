//! What one distillation writes, as a type: the instruction the model reads and the JSON schema the
//! engine enforces are both rendered from [`DistilledLesson`], so the two cannot disagree.
//!
//! Before this, the shape was prose in each lens's prompt ("one or two plain sentences", a required
//! final `SUPERSEDES:` line) and a tolerant line parser read it back. Nothing bounded the answer:
//! with `max_tokens: None` one historian pass asked the engine for 180,382 tokens (Aris on the M5).
//! The engine now holds the answer to this shape: on our fork the template wraps the schema in its
//! reasoning rule, so it constrains the answer and never the think (llama.cpp#95/#96 probes).

use uuid::Uuid;

use crate::ai::types::{JsonSchemaFormat, ResponseFormat};
use crate::cognition::engram::Engram;

/// One distillation's answer.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct DistilledLesson {
    /// The lesson, as one or two plain sentences.
    #[schemars(length(min = 1, max = 2))]
    sentences: Vec<String>,
    /// The numbers of the prior beliefs listed that are now outdated, wrong, or replaced by better
    /// understanding; empty when every one still holds or none is listed.
    supersedes: Vec<u32>,
}

/// A distillation that did not decode as a [`DistilledLesson`].
#[derive(Debug, thiserror::Error)]
#[error("the distillation is not the lesson's shape: {0}")]
pub struct UnreadableLesson(#[from] serde_json::Error);

/// A decoded lesson: its text and the prior beliefs it supersedes, by id.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Lesson {
    pub content: String,
    pub supersedes: Vec<Uuid>,
    /// Numbers she named that no listed belief carries (her slip, never applied).
    pub unlisted: Vec<u32>,
}

impl DistilledLesson {
    /// The format the request asks the engine to hold the answer to.
    pub(super) fn format() -> ResponseFormat {
        ResponseFormat::JsonSchema {
            json_schema: JsonSchemaFormat { name: "dream_lesson".into(), schema: crate::sdk_codegen::input_schema::<Self>() },
        }
    }

    /// What the model reads about the shape, the same fields the schema holds it to. Its
    /// supersession phrase is the one the Speak gate recognizes when a citizen speaks it
    /// (`framing_echo::CONSOLIDATOR_SUPERSEDES_INSTRUCTION`).
    pub(super) fn instruction() -> String {
        format!(
            "Answer as JSON. \"sentences\": the lesson as one or two plain sentences (no numbering, no preamble, no quotes). \
             \"supersedes\": {}, wrong, or replaced by better understanding, as their numbers ([] when every one still \
             holds or none is listed).",
            crate::cognition::framing_echo::CONSOLIDATOR_SUPERSEDES_INSTRUCTION
        )
    }

    /// Read the engine's answer, mapping each superseded number (1-based, as listed) onto the
    /// prior belief it names.
    pub(super) fn read(raw: &str, prior_beliefs: &[Engram]) -> Result<Lesson, UnreadableLesson> {
        let lesson: Self = serde_json::from_str(raw)?; // boundary: the engine's schema-held answer
        let content = lesson.sentences.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        let (mut supersedes, mut unlisted) = (Vec::new(), Vec::new());
        for number in lesson.supersedes {
            match (number as usize).checked_sub(1).and_then(|i| prior_beliefs.get(i)) {
                Some(belief) => supersedes.push(belief.id),
                None => unlisted.push(number),
            }
        }
        Ok(Lesson { content, supersedes, unlisted })
    }
}
