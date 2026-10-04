//! Model-native template preparation and private/public output classification.
//! This does not enable live inference: consumers must apply ALL template sampling
//! metadata through their existing scheduler before they can use the prompt.
use crate::{sys, Context, Model};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
    ptr::NonNull,
};

/// Structured OpenAI-compatible messages/tools are passed intact to common-chat.
/// No role/content flattening or synthesized tool history is performed here.
#[derive(Serialize)]
pub struct ChatOptions {
    pub request_id: String,
    pub template_override: String,
    pub messages: serde_json::Value,
    pub tools: serde_json::Value,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    pub enable_thinking: bool,
    pub template_kwargs: std::collections::BTreeMap<String, String>,
    pub grammar: String,
    pub json_schema: String,
}

/// All template-derived constraints travel together; callers must not use the
/// prompt alone. No Debug implementation: prompts can contain private context.
#[derive(Deserialize)]
pub struct ChatMetadata {
    pub prompt: String,
    pub generation_prompt: String,
    pub format: String,
    pub parser: String,
    pub grammar: String,
    pub grammar_lazy: bool,
    pub grammar_triggers: Vec<GrammarTrigger>,
    pub preserved_tokens: Vec<String>,
    pub additional_stops: Vec<String>,
    pub parser_closers: Vec<String>,
    pub supports_thinking: bool,
    pub thinking_start_tag: String,
    pub thinking_end_tags: Vec<String>,
    pub message_delimiters: serde_json::Value,
}

#[derive(Deserialize)]
pub struct GrammarTrigger {
    /// Native common_grammar_trigger_type (token/word/pattern/full-pattern).
    pub kind: i32,
    pub value: String,
    pub token: i32,
}

/// Only Content may be forwarded to public text presentation. Tool name/argument
/// strings are incremental and must be assembled before execution.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatDelta {
    Content {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Tool {
        index: usize,
        id: String,
        name: String,
        arguments: String,
    },
}

/// Ordinary sampling controls. Template constraints remain native and indivisible.
pub struct ChatSampling {
    pub temperature: f32,
    pub repeat_penalty: f32,
    pub top_k: i32,
    pub top_p: f32,
    pub seed: u32,
}

/// One sequence's native common sampler. It borrows the already-bound model,
/// never the parser handle, and stays on the scheduler's decoding thread.
pub struct CommonSampler<'model> {
    ptr: NonNull<sys::continuum_chat_sampler>,
    _model: PhantomData<&'model Model>,
}

impl CommonSampler<'_> {
    /// Reuse the native normalized set; do not tokenize preserved markers twice.
    pub fn preserves_token(&self, token: i32) -> bool {
        unsafe { sys::continuum_chat_sampler_preserves_token(self.ptr.as_ptr(), token) != 0 }
    }

    /// Native common sampling and acceptance happen together exactly once.
    pub fn sample(&mut self, context: &mut Context, logit_index: i32) -> Result<i32, String> {
        let mut token = 0;
        let error = unsafe {
            sys::continuum_chat_sampler_sample(self.ptr.as_ptr(), context.as_ptr(), logit_index, &mut token)
        };
        check_error(error)?;
        Ok(token)
    }
}

impl Drop for CommonSampler<'_> {
    fn drop(&mut self) {
        unsafe { sys::continuum_chat_sampler_free(self.ptr.as_ptr()) };
    }
}

/// One request's native parser, bound to the already selected model's lifetime.
/// Deliberately not Send/Sync: ownership stays with the decoding consumer.
pub struct PreparedChat<'model> {
    ptr: NonNull<sys::continuum_chat>,
    metadata: ChatMetadata,
    _model: PhantomData<&'model Model>,
}

impl Model {
    pub fn prepare_chat(&self, options: &ChatOptions) -> Result<PreparedChat<'_>, String> {
        PreparedChat::prepare(self.as_ptr(), options)
    }
}

impl<'model> PreparedChat<'model> {
    #[cfg(test)]
    pub(crate) fn for_template(options: &ChatOptions) -> Result<Self, String> {
        Self::prepare(std::ptr::null(), options)
    }

    fn prepare(model: *const sys::llama_model, options: &ChatOptions) -> Result<Self, String> {
        if options.request_id.is_empty() {
            return Err("prepared chat requires a request identity".into());
        }
        let request = serde_json::to_string(options)
            .map_err(|_| "chat request serialization failed".to_string())?;
        let request = CString::new(request).map_err(|_| "invalid chat request".to_string())?;
        let mut ptr = std::ptr::null_mut();
        let error = unsafe { sys::continuum_chat_prepare(model, request.as_ptr(), &mut ptr) };
        check_error(error)?;
        let ptr = NonNull::new(ptr).ok_or_else(|| "native chat returned no handle".to_string())?;
        // Establish RAII immediately; metadata decode failures must also free C++ state.
        let metadata = unsafe { sys::continuum_chat_metadata(ptr.as_ptr()) };
        let decoded = if metadata.is_null() {
            Err("native chat returned no metadata".to_string())
        } else {
            serde_json::from_slice(unsafe { CStr::from_ptr(metadata) }.to_bytes())
                .map_err(|_| "invalid native chat metadata".to_string())
        };
        match decoded {
            Ok(metadata) => Ok(Self {
                ptr,
                metadata,
                _model: PhantomData,
            }),
            Err(error) => {
                unsafe { sys::continuum_chat_free(ptr.as_ptr()) };
                Err(error)
            }
        }
    }

    /// This applies sampling constraints only; callers still own preserved-token
    /// projection, additional stops and native terminal parsing.
    pub fn sampler(&self, settings: &ChatSampling) -> Result<CommonSampler<'model>, String> {
        let settings = sys::continuum_chat_sampling {
            temperature: settings.temperature,
            repeat_penalty: settings.repeat_penalty,
            top_k: settings.top_k,
            top_p: settings.top_p,
            seed: settings.seed,
        };
        let mut ptr = std::ptr::null_mut();
        let error = unsafe { sys::continuum_chat_sampler_create(self.ptr.as_ptr(), &settings, &mut ptr) };
        check_error(error)?;
        Ok(CommonSampler {
            ptr: NonNull::new(ptr).ok_or_else(|| "native prepared sampler returned no handle".to_string())?,
            _model: PhantomData,
        })
    }

    pub fn metadata(&self) -> &ChatMetadata {
        &self.metadata
    }

    /// Feed only complete UTF-8 text pieces; callers retain incomplete byte tails.
    /// Set final_chunk once at model completion; errors terminally retire the parser.
    pub fn append(&mut self, text: &str, final_chunk: bool) -> Result<Vec<ChatDelta>, String> {
        self.append_observed(text, final_chunk, 0)
    }

    /// Terminal bytes must come from the decoder's observed receipt, never from
    /// a stop-name reconstruction. Native parsing verifies they are structural.
    pub fn finish_observed(&mut self, text: &str, closing: &str) -> Result<Vec<ChatDelta>, String> {
        let mut complete = String::with_capacity(text.len() + closing.len());
        complete.push_str(text);
        complete.push_str(closing);
        self.append_observed(&complete, true, closing.len())
    }

    fn append_observed(&mut self, text: &str, final_chunk: bool, hidden_bytes: usize) -> Result<Vec<ChatDelta>, String> {
        let mut deltas = std::ptr::null();
        let error = unsafe {
            sys::continuum_chat_append(
                self.ptr.as_ptr(),
                text.as_ptr().cast(),
                text.len(),
                i32::from(final_chunk),
                hidden_bytes,
                &mut deltas,
            )
        };
        check_error(error)?;
        if deltas.is_null() {
            return Err("native chat returned no deltas".into());
        }
        serde_json::from_slice(unsafe { CStr::from_ptr(deltas) }.to_bytes())
            .map_err(|_| "invalid native chat deltas".to_string())
    }
}

fn check_error(error: *const std::ffi::c_char) -> Result<(), String> {
    if error.is_null() {
        Ok(())
    } else {
        // The bridge exports static categories only, never C++ exception payloads.
        Err(unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned())
    }
}

impl Drop for PreparedChat<'_> {
    fn drop(&mut self) {
        unsafe { sys::continuum_chat_free(self.ptr.as_ptr()) };
    }
}

