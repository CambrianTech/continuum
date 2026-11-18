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

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::NonNull;

use super::Model;
use crate::sys;

extern "C" {
    fn continuum_chat_init(
        model: *const sys::llama_model,
        override_template: *const c_char,
        out: *mut *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    fn continuum_chat_free(chat: *mut c_void);
    fn continuum_chat_string_free(text: *mut c_char);
    fn continuum_chat_render_new(
        chat: *const c_void,
        request: *const c_char,
        reasoning: c_int,
        out: *mut *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    fn continuum_chat_render_free(render: *mut c_void);
    fn continuum_chat_render_describe(render: *const c_void, out: *mut *mut c_char, error: *mut c_char, capacity: usize) -> c_int;
    fn continuum_chat_render_parse(
        render: *const c_void,
        text: *const c_char,
        length: usize,
        is_partial: c_int,
        out: *mut *mut c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    #[cfg(test)]
    fn continuum_chat_render_content_only(reasoning: c_int, out: *mut *mut c_void, error: *mut c_char, capacity: usize) -> c_int;
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
        let mut out = std::ptr::null_mut();
        let mut error = [0 as c_char; 512];
        let status = unsafe {
            continuum_chat_init(
                model.as_ptr(),
                override_template.as_ref().map_or(std::ptr::null(), |text| text.as_ptr()),
                &mut out,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if status != 0 {
            return Err(refusal("init", &error));
        }
        let ptr = NonNull::new(out).ok_or_else(|| missing("init"))?;
        Ok(Self { ptr })
    }

    /// Render an OpenAI-shaped request (`messages`, `tools`, `tool_choice`, `json_schema`,
    /// `grammar`, `enable_thinking`, `add_generation_prompt`, `parallel_tool_calls`), parsing its
    /// reasoning as `reasoning`.
    pub fn render(&self, request: &str, reasoning: ReasoningFormat) -> Result<ChatRender, ChatRefusal> {
        let request = c_text("render", request)?;
        let mut out = std::ptr::null_mut();
        let mut error = [0 as c_char; 512];
        let status = unsafe { continuum_chat_render_new(self.ptr.as_ptr(), request.as_ptr(), reasoning.code(), &mut out, error.as_mut_ptr(), error.len()) };
        if status != 0 {
            return Err(refusal("render", &error));
        }
        ChatRender::adopt(out)
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
    fn adopt(ptr: *mut c_void) -> Result<Self, ChatRefusal> {
        NonNull::new(ptr).map(|ptr| Self { ptr }).ok_or_else(|| missing("render"))
    }

    /// The prompt, the grammar and its lazy triggers, the preserved tokens, the extra stops and the
    /// thinking tags, as JSON. Read once per request; the parser state stays native.
    pub fn describe(&self) -> Result<String, ChatRefusal> {
        returned("describe", |out, error, capacity| unsafe { continuum_chat_render_describe(self.ptr.as_ptr(), out, error, capacity) })
    }

    /// Parse what the model has written so far into the assistant message in the OpenAI shape
    /// (`content`, `reasoning_content`, `tool_calls`). `partial` parses an unfinished stream.
    pub fn parse(&self, text: &str, partial: bool) -> Result<String, ChatRefusal> {
        returned("parse", |out, error, capacity| unsafe {
            continuum_chat_render_parse(self.ptr.as_ptr(), text.as_ptr().cast(), text.len(), c_int::from(partial), out, error, capacity)
        })
    }

    /// A content-only render without a model, for tests of the boundary itself.
    #[cfg(test)]
    fn content_only(reasoning: ReasoningFormat) -> Result<Self, ChatRefusal> {
        let mut out = std::ptr::null_mut();
        let mut error = [0 as c_char; 512];
        if unsafe { continuum_chat_render_content_only(reasoning.code(), &mut out, error.as_mut_ptr(), error.len()) } != 0 {
            return Err(refusal("render", &error));
        }
        Self::adopt(out)
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

fn missing(step: &'static str) -> ChatRefusal {
    ChatRefusal { step, message: "the native side returned nothing".into() }
}

/// Run a native call that returns an owned string, and take it back.
fn returned(step: &'static str, call: impl FnOnce(*mut *mut c_char, *mut c_char, usize) -> c_int) -> Result<String, ChatRefusal> {
    let mut out = std::ptr::null_mut();
    let mut error = [0 as c_char; 512];
    if call(&mut out, error.as_mut_ptr(), error.len()) != 0 {
        return Err(refusal(step, &error));
    }
    if out.is_null() {
        return Err(missing(step));
    }
    let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
    unsafe { continuum_chat_string_free(out) };
    Ok(text)
}

fn refusal(step: &'static str, error: &[c_char]) -> ChatRefusal {
    // The native guard writes with snprintf into a nonempty, zero-initialized buffer.
    let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy().into_owned();
    ChatRefusal { step, message }
}
