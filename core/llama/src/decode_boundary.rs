//! Translate native decode exceptions before they can unwind across Rust.
use crate::sys;
use std::ffi::{c_char, CStr};

extern "C" {
    fn continuum_llama_decode(
        ctx: *mut sys::llama_context,
        batch: sys::llama_batch,
        result: *mut i32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}

pub(crate) unsafe fn decode(
    ctx: *mut sys::llama_context,
    batch: sys::llama_batch,
) -> Result<i32, String> {
    // Bounded diagnostics only; native compute inputs are unchanged.
    let mut error = [0 as c_char; 512];
    let mut result = 0;
    let exception =
        unsafe { continuum_llama_decode(ctx, batch, &mut result, error.as_mut_ptr(), error.len()) };
    outcome(exception, result, &error)
}

fn outcome(exception: i32, result: i32, error: &[c_char]) -> Result<i32, String> {
    if exception == 0 {
        return Ok(result);
    }
    // The native guard writes with snprintf into a nonempty, zero-initialized buffer.
    let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
    Err(format!("llama_decode C++ exception: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    extern "C" {
        fn continuum_decode_boundary_test(
            kind: i32,
            result: *mut i32,
            error: *mut c_char,
            capacity: usize,
        ) -> i32;
    }
    #[test]
    fn native_exceptions_are_errors_and_native_status_is_preserved() {
        for kind in 0..4 {
            let mut error = [0 as c_char; 512];
            let mut result = 0;
            let exception = unsafe {
                continuum_decode_boundary_test(kind, &mut result, error.as_mut_ptr(), error.len())
            };
            let value = outcome(exception, result, &error);
            if kind == 0 {
                assert_eq!(value, Ok(2));
            } else {
                assert!(value
                    .expect_err("native exception must return an error")
                    .contains("C++ exception:"));
            }
        }
    }
}
