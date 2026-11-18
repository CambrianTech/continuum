//! The model's own chat contract, in process: its jinja template rendered WITH the offered tools,
//! and its native tool-call output parsed back into structured calls. These are llama.cpp's
//! common/chat functions (behind `chat_template.cpp`), the ones llama-server's `/v1` ran, so a
//! resident on the in-process engine keeps the tool contract she had there.
//!
//! The boundary is llama.cpp's own OpenAI-compatible JSON; a failure is a typed refusal carrying
//! llama.cpp's own words, never a fallback to plain-text rendering.

use std::ffi::{c_char, CStr, CString};
use std::ptr::NonNull;

use serde::Deserialize;

use super::Model;
use crate::sys;

#[repr(C)]
struct RawTemplates {
    _private: [u8; 0],
}

extern "C" {
    fn continuum_chat_free(s: *mut c_char);
    fn continuum_chat_templates_init(model: *const sys::llama_model, err: *mut *mut c_char) -> *mut RawTemplates;
    fn continuum_chat_templates_free(t: *mut RawTemplates);
    fn continuum_chat_apply(
        t: *const RawTemplates,
        messages_json: *const c_char,
        tools_json: *const c_char,
        enable_thinking: bool,
        err: *mut *mut c_char,
    ) -> *mut c_char;
    fn continuum_reasoning_budget_init(
        model: *const sys::llama_model,
        start_tag: *const c_char,
        end_tags: *const *const c_char,
        n_end_tags: usize,
        budget: i32,
        err: *mut *mut c_char,
    ) -> *mut sys::llama_sampler;
    fn continuum_chat_parse(
        output: *const c_char,
        format: i32,
        generation_prompt: *const c_char,
        parser: *const c_char,
        err: *mut *mut c_char,
    ) -> *mut c_char;
}

/// Why the chat seam refused, in llama.cpp's own words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatRefusal {
    #[error("a chat input carries a NUL byte: {0}")]
    Nul(String),
    #[error("llama.cpp refused: {0}")]
    Native(String),
    #[error("llama.cpp answered JSON this seam cannot read: {0}")]
    Shape(String),
}

/// One grammar trigger: what, once generated, switches the lazy tool-call grammar on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GrammarTrigger {
    /// `common_grammar_trigger_type`: 0 token, 1 word, 2 pattern, 3 full pattern.
    #[serde(rename = "type")]
    pub kind: i32,
    pub value: String,
}

/// A rendered turn: the prompt the model's template produced for these messages and tools, and
/// everything its output must be read back with. Nothing here is optional: an empty grammar means
/// the template offers no constrained call, which llama.cpp states, not a default this side picks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatPrompt {
    pub prompt: String,
    /// `common_chat_format` of the template's tool-call syntax, carried to [`ChatTemplates::parse`].
    pub format: i32,
    pub generation_prompt: String,
    /// The serialized PEG parser for this template's output, carried to [`ChatTemplates::parse`].
    pub parser: String,
    pub grammar: String,
    pub grammar_lazy: bool,
    pub grammar_triggers: Vec<GrammarTrigger>,
    pub preserved_tokens: Vec<String>,
    pub additional_stops: Vec<String>,
    pub supports_thinking: bool,
    pub thinking_start_tag: String,
    pub thinking_end_tags: Vec<String>,
}

/// What a think budget bounds on this template: the tokens allowed between its thinking start tag
/// and the first end tag, which is forced once they are spent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkBound {
    pub start_tag: String,
    pub end_tags: Vec<String>,
    pub budget: u32,
}

impl ChatPrompt {
    /// The bound a `budget` puts on this template's thinking, or `None` when the template does not
    /// think (no start tag to count from, no end tag to force): a budget on it is moot.
    pub fn think_bound(&self, budget: u32) -> Option<ThinkBound> {
        let thinks = self.supports_thinking && !self.thinking_start_tag.is_empty() && !self.thinking_end_tags.is_empty();
        thinks.then(|| ThinkBound {
            start_tag: self.thinking_start_tag.clone(),
            end_tags: self.thinking_end_tags.clone(),
            budget,
        })
    }
}

/// One structured call the model made, in its own native syntax, parsed by llama.cpp.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ParsedToolCall {
    pub id: Option<String>,
    pub function: ParsedFunction,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ParsedFunction {
    pub name: String,
    /// The arguments as the model wrote them: a JSON object's text.
    pub arguments: String,
}

/// A finished generation read back with its template's own parser.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ParsedReply {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ParsedToolCall>,
}

/// A model's chat templates, built once from its GGUF.
pub struct ChatTemplates {
    raw: NonNull<RawTemplates>,
}

// SAFETY: the templates are immutable after init; llama.cpp's apply and parse read them without
// mutation, so sharing one across threads is sound.
unsafe impl Send for ChatTemplates {}
unsafe impl Sync for ChatTemplates {}

impl Drop for ChatTemplates {
    fn drop(&mut self) {
        // SAFETY: `raw` came from continuum_chat_templates_init and is freed exactly once here.
        unsafe { continuum_chat_templates_free(self.raw.as_ptr()) }
    }
}

/// Take ownership of a C string the seam allocated, or of its error.
fn owned(out: *mut c_char, err: *mut c_char) -> Result<String, ChatRefusal> {
    // SAFETY: both pointers are NUL-terminated strings from `dup` in chat_template.cpp (or null),
    // and each is released exactly once with continuum_chat_free after being copied.
    unsafe {
        if out.is_null() {
            let why = if err.is_null() {
                "no reason given".to_string()
            } else {
                let text = CStr::from_ptr(err).to_string_lossy().into_owned();
                continuum_chat_free(err);
                text
            };
            return Err(ChatRefusal::Native(why));
        }
        let text = CStr::from_ptr(out).to_string_lossy().into_owned();
        continuum_chat_free(out);
        Ok(text)
    }
}

fn c(s: &str) -> Result<CString, ChatRefusal> {
    CString::new(s).map_err(|e| ChatRefusal::Nul(e.to_string()))
}

impl ChatTemplates {
    /// The model's own templates, from its GGUF metadata.
    pub fn for_model(model: &Model) -> Result<Self, ChatRefusal> {
        let mut err: *mut c_char = std::ptr::null_mut();
        // SAFETY: the model pointer is live for this call; err receives an owned string or stays null.
        let raw = unsafe { continuum_chat_templates_init(model.as_ptr(), &mut err) };
        match NonNull::new(raw) {
            Some(raw) => Ok(Self { raw }),
            None => Err(owned(std::ptr::null_mut(), err).err().unwrap_or_else(|| ChatRefusal::Native("no templates".into()))),
        }
    }

    /// Render `messages` and `tools` (OpenAI JSON) through the model's template.
    pub fn apply(&self, messages_json: &str, tools_json: &str, enable_thinking: bool) -> Result<ChatPrompt, ChatRefusal> {
        let (messages, tools) = (c(messages_json)?, c(tools_json)?);
        let mut err: *mut c_char = std::ptr::null_mut();
        // SAFETY: every pointer is live for the call; the result is an owned string or null + err.
        let out = unsafe {
            continuum_chat_apply(self.raw.as_ptr(), messages.as_ptr(), tools.as_ptr(), enable_thinking, &mut err)
        };
        let text = owned(out, err)?;
        serde_json::from_str(&text).map_err(|e| ChatRefusal::Shape(e.to_string()))
    }

    /// Read a finished generation back with the parser `prompt` was rendered with.
    pub fn parse(&self, output: &str, prompt: &ChatPrompt) -> Result<ParsedReply, ChatRefusal> {
        let (output, generation_prompt, parser) = (c(output)?, c(&prompt.generation_prompt)?, c(&prompt.parser)?);
        let mut err: *mut c_char = std::ptr::null_mut();
        // SAFETY: every pointer is live for the call; the result is an owned string or null + err.
        let out = unsafe {
            continuum_chat_parse(output.as_ptr(), prompt.format, generation_prompt.as_ptr(), parser.as_ptr(), &mut err)
        };
        let text = owned(out, err)?;
        serde_json::from_str(&text).map_err(|e| ChatRefusal::Shape(e.to_string()))
    }
}

impl super::SamplerChainBuilder {
    /// Bound the model's thinking: llama.cpp's reasoning-budget sampler (the one llama-server ran)
    /// counts tokens after the start tag and forces the end tag once `bound.budget` is spent.
    pub fn reasoning_budget(self, model: &Model, bound: &ThinkBound) -> Result<Self, ChatRefusal> {
        let start = c(&bound.start_tag)?;
        let ends: Vec<CString> = bound.end_tags.iter().map(|tag| c(tag)).collect::<Result<_, _>>()?;
        let end_ptrs: Vec<*const c_char> = ends.iter().map(|tag| tag.as_ptr()).collect();
        let budget = i32::try_from(bound.budget).map_err(|_| ChatRefusal::Shape(format!("think budget {} exceeds i32", bound.budget)))?;
        let mut err: *mut c_char = std::ptr::null_mut();
        // SAFETY: the model and every string are live for the call; the sampler returned is owned by
        // the chain once added, and err receives an owned string or stays null.
        let smpl = unsafe {
            continuum_reasoning_budget_init(model.as_ptr(), start.as_ptr(), end_ptrs.as_ptr(), end_ptrs.len(), budget, &mut err)
        };
        if smpl.is_null() {
            return Err(owned(std::ptr::null_mut(), err).err().unwrap_or_else(|| ChatRefusal::Native("no reasoning-budget sampler".into()))); // unwrap_or_else: owned() of a null result is always an Err
        }
        Ok(self.add(smpl))
    }
}
