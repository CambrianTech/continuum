//! The model's own chat handling, through `chat_boundary.cpp` (llama.cpp common/chat): render a
//! conversation through the GGUF's template with its tools and reasoning, and parse what the model
//! wrote into content, reasoning and tool calls. This is what llama-server does for an OpenAI
//! request, in-process, so an in-process mind keeps native tool calls (her hands) and reasoning.
//!
//! One JSON document crosses each way, in the OpenAI shape the adapters already speak; the typed
//! request and reply live with the adapter. A C++ exception is caught on the native side and comes
//! back as a [`ChatRefusal`], never an unwind across Rust.

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
    fn continuum_chat_apply(chat: *const c_void, request: *const c_char, out: *mut *mut c_char, error: *mut c_char, capacity: usize) -> c_int;
    fn continuum_chat_parse(
        rendered: *const c_char,
        text: *const c_char,
        is_partial: c_int,
        out: *mut *mut c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    fn continuum_chat_string_free(text: *mut c_char);
}

/// The chat handling refused: which step, and what the native side said.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("chat {step} refused: {message}")]
pub struct ChatRefusal {
    pub step: &'static str,
    pub message: String,
}

/// A model's chat templates, loaded once and used for every render.
pub struct ChatTemplates {
    ptr: NonNull<c_void>,
}

// The native templates are owned by this handle alone and never aliased; moving them to another
// thread is sound. Rendering is not claimed to be safe from two threads at once (no `Sync`).
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
        let ptr = NonNull::new(out).ok_or_else(|| ChatRefusal { step: "init", message: "no templates were returned".into() })?;
        Ok(Self { ptr })
    }

    /// Render an OpenAI-shaped request (`messages`, `tools`, `tool_choice`, `json_schema`,
    /// `enable_thinking`, `reasoning_format`, `add_generation_prompt`, `parallel_tool_calls`) into
    /// the prompt, the grammar and its lazy triggers, the stops, and the parser state [`parse`]
    /// takes back.
    pub fn apply(&self, request: &str) -> Result<String, ChatRefusal> {
        let request = c_text("apply", request)?;
        returned("apply", |out, error, capacity| unsafe { continuum_chat_apply(self.ptr.as_ptr(), request.as_ptr(), out, error, capacity) })
    }
}

impl Drop for ChatTemplates {
    fn drop(&mut self) {
        unsafe { continuum_chat_free(self.ptr.as_ptr()) }
    }
}

/// Parse what the model wrote under `rendered` (an [`ChatTemplates::apply`] result) into the
/// assistant message in the OpenAI shape: `content`, `reasoning_content`, `tool_calls`. `partial`
/// parses an unfinished stream.
pub fn parse(rendered: &str, text: &str, partial: bool) -> Result<String, ChatRefusal> {
    let rendered = c_text("parse", rendered)?;
    let text = c_text("parse", text)?;
    returned("parse", |out, error, capacity| unsafe {
        continuum_chat_parse(rendered.as_ptr(), text.as_ptr(), c_int::from(partial), out, error, capacity)
    })
}

fn c_text(step: &'static str, text: &str) -> Result<CString, ChatRefusal> {
    CString::new(text).map_err(|_| ChatRefusal { step, message: "text holds a NUL byte".into() })
}

/// Run a native call that returns an owned string, and take it back.
fn returned(step: &'static str, call: impl FnOnce(*mut *mut c_char, *mut c_char, usize) -> c_int) -> Result<String, ChatRefusal> {
    let mut out = std::ptr::null_mut();
    let mut error = [0 as c_char; 512];
    if call(&mut out, error.as_mut_ptr(), error.len()) != 0 {
        return Err(refusal(step, &error));
    }
    if out.is_null() {
        return Err(ChatRefusal { step, message: "no result was returned".into() });
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
