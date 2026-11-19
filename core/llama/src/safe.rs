//! Safe Rust wrapper over llama.cpp FFI.

use std::ffi::CString;
use std::marker::PhantomData;
use std::os::raw::c_char;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Once;

use crate::sys;

mod backend_init;
pub use backend_init::*;
mod chat_template;
pub use chat_template::{
    ChatPrompt, ChatRefusal, ChatTemplates, GrammarTrigger, ParsedFunction, ParsedReply, ParsedToolCall, ThinkBound,
};
mod expert_eval_cb;
pub use expert_eval_cb::*;
mod persona;
pub use persona::{BindRefusal, PersonaBlocks, PersonaCheckpoint, PersonaOwner, PersonaRefusal};

// ─── Tests ───────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;
