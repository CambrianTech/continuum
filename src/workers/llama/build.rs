//! Build llama.cpp via cmake and link it into our binary.
//!
//! Cross-platform:
//!   - macOS: GGML_METAL=ON (Metal framework available)
//!   - Linux + `cuda` feature: GGML_CUDA=ON (requires CUDA toolkit)
//!   - Default: CPU only with BLAS if available

use std::env;
use std::path::PathBuf;

fn main() {
    let submodule = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vendor")
        .join("llama.cpp");

    println!("cargo:rerun-if-changed={}", submodule.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut cfg = cmake::Config::new(&submodule);
    cfg.define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_TESTS", "OFF")
        .define("LLAMA_BUILD_SERVER", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // Metal on macOS
    if cfg!(feature = "metal") && target_os == "macos" {
        cfg.define("GGML_METAL", "ON")
           .define("GGML_METAL_EMBED_LIBRARY", "ON");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    } else {
        cfg.define("GGML_METAL", "OFF");
    }

    // linux/aarch64 (Apple Silicon hosts running Docker via Rancher/colima,
    // ARM servers, etc.): llama.cpp's ggml-cpu uses NEON FP16 vector
    // intrinsics (vaddq_f16 and friends) that require armv8.2-a+fp16 target
    // flags. GCC won't inline them under `-march=armv8-a` which is the
    // default on many Linux arm64 toolchains. Without this the build fails
    // with `inlining failed in call to 'vaddq_f16': target specific option
    // mismatch`. We tell llama.cpp's CMake to opt us into the armv8.2-a
    // baseline plus the dotprod/fp16 extensions that Apple Silicon (and
    // every modern ARM server) supports.
    //
    // On Apple host macOS (Metal path above) this doesn't apply — Apple
    // Clang handles it automatically via `-arch arm64` / `-mmacosx-version-
    // min` and the Metal backend bypasses the NEON kernels anyway.
    if target_os == "linux" && target_arch == "aarch64" {
        // GGML_NATIVE=OFF — the default-ON path uses `-mcpu=native` and
        // autodetects features via compile probes. Inside the Rancher/Lima
        // VM (and other virtualized arm64 hosts), that probe doesn't
        // reliably enable fp16 even on Apple-Silicon-class CPUs, so vfmaq_f16
        // / vaddq_f16 fail to inline. Forcing OFF makes CMake honor
        // GGML_CPU_ARM_ARCH instead.
        cfg.define("GGML_NATIVE", "OFF");
        cfg.define("GGML_CPU_ARM_ARCH", "armv8.2-a+dotprod+fp16");
    }

    // CUDA on Linux
    if cfg!(feature = "cuda") && target_os == "linux" {
        cfg.define("GGML_CUDA", "ON");
        println!("cargo:rustc-link-lib=cuda");
        println!("cargo:rustc-link-lib=cudart");
        println!("cargo:rustc-link-lib=cublas");
    } else {
        cfg.define("GGML_CUDA", "OFF");
    }

    let dst = cfg.build();

    // Link the static libraries produced by cmake
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/build/ggml/src", dst.display());
    println!("cargo:rustc-link-search=native={}/build/src", dst.display());
    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    // GGML backends register via C++ static initializers inside the backend's
    // static archive. Without +whole-archive, ld --as-needed / dead_strip
    // drops the archive because nothing from the main llama archive directly
    // references a symbol in it — so ggml_backend_metal_reg / _cuda_reg
    // never fire and the runtime falls back to CPU with a "backend missing"
    // error. Force-load the whole archive so every registration symbol is
    // preserved.
    if cfg!(feature = "metal") && target_os == "macos" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-metal");
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-blas");
    }
    if cfg!(feature = "cuda") && target_os == "linux" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-cuda");
    }

    // C++ stdlib + OpenMP (llama.cpp CPU backend uses GOMP_parallel on Linux).
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=c++");
    } else {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=gomp");
    }

    // Generate FFI bindings for llama.h
    let header = submodule.join("include").join("llama.h");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    let bindings = bindgen::Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg(format!("-I{}", submodule.join("ggml").join("include").display()))
        .allowlist_function("llama_.*")
        .allowlist_function("ggml_.*")
        .allowlist_type("llama_.*")
        .allowlist_type("ggml_.*")
        .allowlist_var("LLAMA_.*")
        .generate()
        .expect("Failed to generate bindings");
    bindings.write_to_file(&out_path)
        .expect("Failed to write bindings");
}
