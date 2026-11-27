//! What the native engine says, kept only where it matters. Installed once at backend init.
//!
//! libllama and ggml log every level to stderr by default, and the default is all a process gets
//! until it installs a callback. On the M5 the core's stderr reached 15,004,123,137 bytes (2026-11-27):
//! 1.24 million `llama_graph_n_input_tensors` lines, every graph reserve, and a grammar line per
//! sampled token, written by a service no one reads, into a file nothing rotates. Debug and info are
//! dropped here, in the callback, before any allocation; warnings and errors reach the sink the
//! process installs ([`set_native_log_sink`]: the core's probes), or stderr when none is installed.

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

use super::sys;

/// A native message worth keeping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLog {
    Warn,
    Error,
}

/// Where kept native messages go. One per process, set before or after backend init.
pub type NativeLogSink = fn(NativeLog, &str);

static SINK: OnceLock<NativeLogSink> = OnceLock::new();

thread_local! {
    /// The level of this thread's last message, which a `CONT` line continues.
    static LAST: Cell<Option<NativeLog>> = const { Cell::new(None) };
}

/// Route kept native messages to `sink`. The first sink wins; a second returns false.
pub fn set_native_log_sink(sink: NativeLogSink) -> bool {
    SINK.set(sink).is_ok()
}

/// What a native level keeps, given the level of the line a `CONT` continues.
fn kept(level: sys::ggml_log_level, last: Option<NativeLog>) -> Option<NativeLog> {
    match level {
        sys::ggml_log_level_GGML_LOG_LEVEL_WARN => Some(NativeLog::Warn),
        sys::ggml_log_level_GGML_LOG_LEVEL_ERROR => Some(NativeLog::Error),
        sys::ggml_log_level_GGML_LOG_LEVEL_CONT => last,
        _ => None,
    }
}

unsafe extern "C" fn native_log(level: sys::ggml_log_level, text: *const c_char, _: *mut c_void) {
    let keep = LAST.with(|last| {
        let keep = kept(level, last.get());
        if level != sys::ggml_log_level_GGML_LOG_LEVEL_CONT {
            last.set(keep);
        }
        keep
    });
    let Some(keep) = keep else { return };
    if text.is_null() {
        return;
    }
    // SAFETY: ggml passes a NUL-terminated message that lives for the call.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    match SINK.get() {
        Some(sink) => sink(keep, text.trim_end()),
        None => eprint!("{text}"),
    }
}

/// Install the callback for libllama and ggml. Called once, inside backend init.
pub(super) fn install() {
    // SAFETY: the callback is a plain function that outlives every native call; no user data.
    unsafe { sys::llama_log_set(Some(native_log), std::ptr::null_mut()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the native flood returning (debug or info kept), a warning or error lost,
    // or a continuation line kept or dropped against the line it continues.
    #[test]
    fn keeps_warnings_and_errors_and_their_continuations_and_nothing_else() {
        assert_eq!(kept(sys::ggml_log_level_GGML_LOG_LEVEL_DEBUG, None), None);
        assert_eq!(kept(sys::ggml_log_level_GGML_LOG_LEVEL_INFO, None), None);
        assert_eq!(kept(sys::ggml_log_level_GGML_LOG_LEVEL_WARN, None), Some(NativeLog::Warn));
        assert_eq!(kept(sys::ggml_log_level_GGML_LOG_LEVEL_ERROR, None), Some(NativeLog::Error));
        assert_eq!(
            kept(sys::ggml_log_level_GGML_LOG_LEVEL_CONT, Some(NativeLog::Error)),
            Some(NativeLog::Error)
        );
        assert_eq!(kept(sys::ggml_log_level_GGML_LOG_LEVEL_CONT, None), None, "continues a dropped line");
    }
}
