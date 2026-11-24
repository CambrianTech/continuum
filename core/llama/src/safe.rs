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
mod expert_eval_cb;
pub use expert_eval_cb::*;
mod chat;
pub use chat::{ChatRefusal, ChatRender, ChatTemplates, ReasoningFormat, SamplingKnobs, TurnSampler};
mod learn;
mod optimizer_slots;
pub use optimizer_slots::{OptimizerLease, Role, Slot};
pub use learn::{BufferType, Finished, FreshGeometry, Label, LearnRefusal, LearnRun, Learner, MemoryRole, MemoryRow, Optimizer, Progress, Recovered, Refused, Reply, ReplyAdmission, Shadow, Walk};
mod persona;
pub use persona::{BindRefusal, PersonaBlocks, PersonaCheckpoint, PersonaOwner, PersonaRefusal};
mod decode_refusal;
pub use decode_refusal::DecodeRefusal;
mod position;
pub use position::{Pos, PosOutOfRange};

// ─── Tests ───────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "embedding-model-tests"))]
mod embedding_output_tests;
