//! The model's own chat handling, through `chat_boundary.cpp` (llama.cpp common/chat): render a
//! conversation through the GGUF's template with its tools and reasoning, and parse what the model
//! writes into content, reasoning and tool calls. This is what llama-server does for an OpenAI
//! request, in-process, so an in-process mind keeps native tool calls (her hands) and reasoning.
//!
//! A [`ChatRender`] is a native handle: it keeps the parser state, so each streamed update parses
//! with it directly and the prompt and sampler inputs cross once ([`ChatRender::describe`]). The
//! request and the parsed message cross as JSON in the OpenAI shape the adapters already speak; the
//! typed forms live with the adapter. A C++ exception is caught on the native side and comes back
//! as a [`ChatRefusal`], never an unwind across Rust.

#[cfg(test)]
mod tests;
mod turn_sampler;

pub use turn_sampler::{SamplingKnobs, TurnSampler};

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::NonNull;

use super::Model;
use crate::sys;

extern "C" {
    fn continuum_chat_init(model: *const sys::llama_model, override_template: *const c_char, reply: *mut Reply) -> c_int;
    fn continuum_chat_free(chat: *mut c_void);
    fn continuum_chat_string_free(text: *mut c_char);
    fn continuum_chat_render_new(chat: *const c_void, request: *const c_char, reasoning: c_int, reply: *mut Reply) -> c_int;
    fn continuum_chat_render_free(render: *mut c_void);
    fn continuum_chat_render_describe(render: *const c_void, reply: *mut Reply) -> c_int;
    fn continuum_chat_render_parse(render: *const c_void, said: *const Utterance, reply: *mut Reply) -> c_int;
    #[cfg(test)]
    fn continuum_chat_render_content_only(reasoning: c_int, reply: *mut Reply) -> c_int;
}

/// What every native call hands back: its result (a handle or an owned string) or its error text.
#[repr(C)]
struct Reply {
    out: *mut c_void,
    error: [c_char; 512],
}

impl Reply {
    fn new() -> Self {
        Self { out: std::ptr::null_mut(), error: [0; 512] }
    }

    /// The handle or string the call returned, or the refusal it reported.
    fn taken(self, step: &'static str, status: c_int) -> Result<NonNull<c_void>, ChatRefusal> {
        if status != 0 {
            return Err(self.refusal(step));
        }
        NonNull::new(self.out).ok_or_else(|| ChatRefusal { step, message: "the native side returned nothing".into() })
    }

    /// The refusal a failed call reported.
    fn refusal(&self, step: &'static str) -> ChatRefusal {
        // The native guard writes with snprintf into a nonempty, zero-initialized buffer.
        let message = unsafe { CStr::from_ptr(self.error.as_ptr()) }.to_string_lossy().into_owned();
        ChatRefusal { step, message }
    }
}

/// What the model has written so far, by length, and whether the stream continues.
#[repr(C)]
struct Utterance {
    text: *const c_char,
    length: usize,
    partial: c_int,
}

/// The chat handling refused: which step, and what the native side said.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("chat {step} refused: {message}")]
pub struct ChatRefusal {
    pub step: &'static str,
    pub message: String,
}

/// How the model's reasoning comes back from a parse. Typed at the boundary, so an unknown format is
/// unrepresentable rather than defaulted (Codex on #6142).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningFormat {
    /// Reasoning stays in the content.
    None,
    /// The template's own reasoning split, into `reasoning_content`.
    Auto,
    /// Thinking-tag contents into `reasoning_content`, in streamed updates too.
    Deepseek,
    /// Thinking-tag contents into `reasoning_content`, left inline while streaming.
    DeepseekLegacy,
}

impl ReasoningFormat {
    fn code(self) -> c_int {
        match self {
            Self::None => 0,
            Self::Auto => 1,
            Self::Deepseek => 2,
            Self::DeepseekLegacy => 3,
        }
    }
}

/// A model's chat templates, loaded once and used for every render.
pub struct ChatTemplates {
    ptr: NonNull<c_void>,
}

// Owned by this handle alone and never aliased: moving it to another thread is sound. Rendering is
// not claimed to be safe from two threads at once (no `Sync`).
unsafe impl Send for ChatTemplates {}

impl ChatTemplates {
    /// The templates of `model`, or `override_template` in place of the GGUF's own.
    pub fn new(model: &Model, override_template: Option<&str>) -> Result<Self, ChatRefusal> {
        let override_template = override_template.map(|text| c_text("init", text)).transpose()?;
        let template = override_template.as_ref().map_or(std::ptr::null(), |text| text.as_ptr());
        let mut reply = Reply::new();
        let status = unsafe { continuum_chat_init(model.as_ptr(), template, &mut reply) };
        Ok(Self { ptr: reply.taken("init", status)? })
    }

    /// Render an OpenAI-shaped request (`messages`, `tools`, `tool_choice`, `json_schema`,
    /// `grammar`, `enable_thinking`, `add_generation_prompt`, `parallel_tool_calls`), parsing its
    /// reasoning as `reasoning`.
    pub fn render(&self, request: &str, reasoning: ReasoningFormat) -> Result<ChatRender, ChatRefusal> {
        let request = c_text("render", request)?;
        let mut reply = Reply::new();
        let status = unsafe { continuum_chat_render_new(self.ptr.as_ptr(), request.as_ptr(), reasoning.code(), &mut reply) };
        Ok(ChatRender { ptr: reply.taken("render", status)? })
    }
}

impl Drop for ChatTemplates {
    fn drop(&mut self) {
        unsafe { continuum_chat_free(self.ptr.as_ptr()) }
    }
}

/// One rendered request, holding its parser natively.
pub struct ChatRender {
    ptr: NonNull<c_void>,
}

// Owned by this handle alone and never aliased; parsing reads it, it is not shared across threads.
unsafe impl Send for ChatRender {}

impl ChatRender {
    /// The prompt, its generation prompt and the extra stops, as JSON. Read once per request; the
    /// parser and everything her sampler is built from stay native.
    pub fn describe(&self) -> Result<String, ChatRefusal> {
        returned("describe", |reply| unsafe { continuum_chat_render_describe(self.ptr.as_ptr(), reply) })
    }

    /// Parse what the model has written so far into the assistant message in the OpenAI shape
    /// (`content`, `reasoning_content`, `tool_calls`). `partial` parses an unfinished stream.
    pub fn parse(&self, text: &str, partial: bool) -> Result<String, ChatRefusal> {
        let said = Utterance { text: text.as_ptr().cast(), length: text.len(), partial: c_int::from(partial) };
        returned("parse", |reply| unsafe { continuum_chat_render_parse(self.ptr.as_ptr(), &said, reply) })
    }

    /// A content-only render without a model, for tests of the boundary itself.
    #[cfg(test)]
    fn content_only(reasoning: ReasoningFormat) -> Result<Self, ChatRefusal> {
        let mut reply = Reply::new();
        let status = unsafe { continuum_chat_render_content_only(reasoning.code(), &mut reply) };
        Ok(Self { ptr: reply.taken("render", status)? })
    }
}

impl Drop for ChatRender {
    fn drop(&mut self) {
        unsafe { continuum_chat_render_free(self.ptr.as_ptr()) }
    }
}

fn c_text(step: &'static str, text: &str) -> Result<CString, ChatRefusal> {
    CString::new(text).map_err(|_| ChatRefusal { step, message: "text holds a NUL byte".into() })
}

/// Run a native call that returns an owned string, and take it back.
fn returned(step: &'static str, call: impl FnOnce(*mut Reply) -> c_int) -> Result<String, ChatRefusal> {
    let mut reply = Reply::new();
    let status = call(&mut reply);
    let out: *mut c_char = reply.taken(step, status)?.as_ptr().cast();
    let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
    unsafe { continuum_chat_string_free(out) };
    Ok(text)
}
