//! A turn's sampler: llama.cpp's `common_sampler`, the object llama-server sampled every turn with,
//! built by the render that produced her prompt ([`ChatRender::sampler`]) or from plain knobs
//! ([`TurnSampler::plain`]). It owns the tool-call grammar (lazy, applied only outside her
//! reasoning), the generation-prompt prefill, the reasoning budget and the distribution chain, so the
//! engine re-implements none of them.

use std::ffi::{c_char, c_int, c_void};
use std::ptr::NonNull;

use super::{c_text, ChatRefusal, ChatRender, Reply};
use crate::safe::{Context, Model};
use crate::sys;

/// The knobs a request states; every other sampling parameter is llama.cpp's default, as
/// llama-server ran it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingKnobs {
    /// `<= 0` samples greedily.
    pub temperature: f32,
    /// `<= 0` keeps the whole vocabulary.
    pub top_k: i32,
    /// `1.0` disables nucleus sampling.
    pub top_p: f32,
    /// `1.0` disables the repetition penalty.
    pub repeat_penalty: f32,
}

/// The knobs and the think budget a rendered turn's sampler is built with, as the native side reads
/// them (`think_budget` < 0: the model's default, unbounded).
#[repr(C)]
struct TurnSampling {
    knobs: SamplingKnobs,
    think_budget: i32,
}

extern "C" {
    fn continuum_turn_sampler_from_render(model: *const sys::llama_model, render: *const c_void, turn: *const TurnSampling, reply: *mut Reply) -> c_int;
    fn continuum_turn_sampler_plain(model: *const sys::llama_model, knobs: *const SamplingKnobs, grammar: *const c_char, reply: *mut Reply) -> c_int;
    /// Her next token, or `LLAMA_TOKEN_NULL` (negative) when a stage refused, with why in `reply`.
    fn continuum_turn_sampler_sample(turn: *mut c_void, ctx: *mut sys::llama_context, idx: i32, reply: *mut Reply) -> i32;
    fn continuum_turn_sampler_free(turn: *mut c_void);
}

/// One turn's sampler state: what she has written so far, as the grammar, the reasoning budget and
/// the penalties track it.
pub struct TurnSampler {
    ptr: NonNull<c_void>,
}

// Owned by this handle alone and never aliased: built on her act's thread, sampled on the engine's.
unsafe impl Send for TurnSampler {}

impl TurnSampler {
    /// A sampler for a turn with no chat render: `knobs`, and `grammar` when the request demands a
    /// shape.
    pub fn plain(model: &Model, knobs: SamplingKnobs, grammar: Option<&str>) -> Result<Self, ChatRefusal> {
        const STEP: &str = "plain sampler";
        let grammar = grammar.map(|text| c_text(STEP, text)).transpose()?;
        let grammar = grammar.as_ref().map_or(std::ptr::null(), |text| text.as_ptr());
        let mut reply = Reply::new();
        let status = unsafe { continuum_turn_sampler_plain(model.as_ptr(), &knobs, grammar, &mut reply) };
        Ok(Self { ptr: reply.taken(STEP, status)? })
    }

    /// Her next token from output `idx` of the last decode on `ctx`, accepted into every stage that
    /// tracks what she wrote. A token the grammar cannot accept refuses the step by name.
    pub fn sample(&mut self, ctx: &Context<'_>, idx: i32) -> Result<i32, ChatRefusal> {
        let mut reply = Reply::new();
        let token = unsafe { continuum_turn_sampler_sample(self.ptr.as_ptr(), ctx.ptr.as_ptr(), idx, &mut reply) };
        if token < 0 {
            return Err(reply.refusal("sample"));
        }
        Ok(token)
    }
}

impl Drop for TurnSampler {
    fn drop(&mut self) {
        unsafe { continuum_turn_sampler_free(self.ptr.as_ptr()) }
    }
}

impl ChatRender {
    /// The sampler this render describes for its turn, as llama-server derives one from the same
    /// chat params: the tool-call grammar and its lazy triggers, the generation prompt, and a
    /// reasoning budget of `think_budget` tokens bounded by the template's thinking tags (`None`: the
    /// model's default, unbounded).
    pub fn sampler(&self, model: &Model, knobs: SamplingKnobs, think_budget: Option<u32>) -> Result<TurnSampler, ChatRefusal> {
        const STEP: &str = "render sampler";
        let think_budget = match think_budget {
            None => -1,
            Some(tokens) => i32::try_from(tokens).map_err(|_| ChatRefusal { step: STEP, message: format!("think budget {tokens} exceeds i32") })?,
        };
        let turn = TurnSampling { knobs, think_budget };
        let mut reply = Reply::new();
        let status = unsafe { continuum_turn_sampler_from_render(model.as_ptr(), self.ptr.as_ptr(), &turn, &mut reply) };
        Ok(TurnSampler { ptr: reply.taken(STEP, status)? })
    }
}
