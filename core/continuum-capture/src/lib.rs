//! The capture of a mind's turns: the one fixture-directory policy and the prompt-capture
//! recorder, its per-path owner and its playback store. Cut out of continuum-cognition as a leaf
//! of its crate tree (docs/architecture/COGNITION-CRATE-TREE.md, step 9); cognition re-exports
//! each module under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

/// Where captures land: the one fixture-directory policy shared by every live capture
/// (persona turns, prompt and workspace captures) and their readers.
pub mod capture;
/// The capture's binary frame: a typed header as a MessagePack array and the bulk as raw byte runs.
pub mod frame;
/// The capture's binary log: frames appended per generation through the owner's disk class, a binary index beside them.
pub mod capture_log;
/// The captures' retention under disk pressure: closed generations, oldest first, never a live one.
pub mod capture_pool;
/// The capture's write-behind: a SAGE component that hears capture records and appends them to their owner's log.
pub mod capture_writer;
/// The one-time move of earlier builds' JSONL prompt captures into frames, verified before removal.
pub mod jsonl_migration;
/// A prompt call as frames: its submission, its end and an unwritable record, and their one decode.
pub mod call_frames;
/// Where each captured call is in its owner's log, by key: read from frame headers, extended from each index tail.
pub mod call_index;
/// A persona's captured calls read back: pages, one call whole, and the completed calls.
pub mod call_playback;
pub mod prompt_capture;
