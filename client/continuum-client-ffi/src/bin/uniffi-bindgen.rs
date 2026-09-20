//! Standalone `uniffi-bindgen` entrypoint for this crate (uniffi 0.31 library
//! mode). The native SDK build steps invoke it to emit Swift/Kotlin source
//! from the compiled cdylib:
//!
//! ```sh
//! cargo run --bin uniffi-bindgen -- generate \
//!   --library target/debug/libcontinuum_client_ffi.{dylib,so,dll} \
//!   --language swift --out-dir bindings/swift
//! ```
//!
//! Keeping the binding generator IN the crate (rather than relying on a
//! globally-installed `uniffi-bindgen` whose version can drift from the
//! `uniffi` lib version) is the single-source-of-truth discipline: the
//! generator and the runtime are pinned to the same version by Cargo.
fn main() {
    uniffi::uniffi_bindgen_main()
}
