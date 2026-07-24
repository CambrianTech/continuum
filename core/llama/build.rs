//! Build llama.cpp via cmake and link it into our binary.
//!
//! Cross-platform:
//!   - macOS: GGML_METAL=ON (Metal framework available)
//!   - Linux + `cuda` feature: GGML_CUDA=ON (requires CUDA toolkit)
//!   - Linux + `vulkan` feature: GGML_VULKAN=ON (routes to host Metal via
//!     MoltenVK when run under krunkit+Podman on Apple Silicon; gets ~80% of
//!     native Metal perf. Also viable on Nvidia/AMD Linux hosts with
//!     libvulkan.)
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
        // We want libmtmd (multimodal projector + image/audio encoder) so
        // the in-process LlamaCppAdapter can route ContentPart::Image to
        // the model natively instead of dropping it. mtmd lives under
        // tools/mtmd in the upstream tree; tools/CMakeLists.txt adds it
        // via add_subdirectory(mtmd) only when LLAMA_BUILD_TOOLS=ON, and
        // tools/ itself is gated on (LLAMA_BUILD_COMMON AND LLAMA_BUILD_TOOLS).
        // So both flags must flip to ON. Side effect: a handful of tool
        // executables get built (llama-bench, llama-tokenize, etc.); they
        // produce static archives that we link selectively below — the
        // executable binaries themselves don't ship with us.
        .define("LLAMA_BUILD_COMMON", "ON")
        .define("LLAMA_BUILD_TOOLS", "ON")
        .define("BUILD_SHARED_LIBS", "OFF")
        // Static archives produced here get linked into continuum-core,
        // which is crate-type = ["cdylib", "rlib"] — lib.rs builds a
        // shared object. Without -fPIC the cuda variant fails at final
        // link with:
        //   /usr/bin/ld: libggml-cuda.a(...): relocation R_X86_64_PC32
        //   against symbol `stderr@@GLIBC_2.2.5' can not be used when
        //   making a shared object; recompile with -fPIC
        // CMake's POSITION_INDEPENDENT_CODE flag adds -fPIC to all C/C++
        // compilation including nvcc's host-side, so the resulting .a
        // archives can link into a .so. Cheap and safe everywhere.
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        // Disable NCCL (NVIDIA Collective Communications Library). It's
        // for multi-GPU all-reduce; single-GPU deploys (BigMama 5090,
        // Toby's Windows/WSL2, most users) never use it. The default
        // ggml/CMakeLists.txt option is ON, and find_package(NCCL)
        // succeeds inside nvidia/cuda:12.8.0-devel because NCCL headers
        // are present — but the runtime image doesn't ship libnccl.so,
        // and even if it did we'd be linking nccl* symbols into a
        // workload that never calls them. Final link fails with:
        //   undefined reference to `ncclCommInitAll' / `ncclAllReduce'
        //   / `ncclGroupStart' / `ncclGetErrorString' (etc.)
        // When we ship a multi-GPU build (later, separate image), flip
        // this back ON and add libnccl to the runtime apt list.
        .define("GGML_CUDA_NCCL", "OFF");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    // "msvc" on windows-msvc, "gnu" on windows-gnu/linux-gnu, "" elsewhere.
    // Used to pick the right C++ runtime + OpenMP libs below: MSVC has no
    // GCC-world `stdc++`/`gomp`.
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // windows-msvc: pin llama.cpp's C/C++ runtime to the RELEASE CRT (/MD) even
    // in Cargo's debug profile. Rust's msvc target ALWAYS links the release CRT
    // (msvcrt/ucrt) — it has no debug-CRT (/MDd) variant. Modern cmake with
    // CMP0091 NEW selects the runtime via CMAKE_MSVC_RUNTIME_LIBRARY, whose
    // default is MultiThreadedDebugDLL (/MDd) for the Debug config — and /MDd
    // is what DEFINES `_DEBUG`. With _DEBUG set, the MSVC headers (the PPL task
    // machinery pulled in transitively by llama.cpp) emit references to the
    // debug-only CRT reporter `_CrtDbgReport`/`_CrtDbgReportW`. Those symbols do
    // not exist in the release CRT that Rust links, so the FINAL link of
    // continuum_core.dll fails:
    //   libllama…rlib(llama.cpp.obj): error LNK2001: unresolved external symbol
    //     __imp__CrtDbgReport  →  LNK1120: 3 unresolved externals
    // Pinning MultiThreadedDLL (/MD, no _DEBUG) makes llama.cpp's CRT match
    // Rust's on both debug and release profiles. `cargo build --release` already
    // gets this for free via the Release config's natural /MD; this line fixes
    // the debug profile too. No-op off msvc (the flag only affects MSVC).
    if target_env == "msvc" {
        // Belt-and-suspenders: ensure CMP0091 is NEW so CMAKE_MSVC_RUNTIME_LIBRARY
        // is actually honored rather than the legacy /MDd-baked-into-flags path.
        cfg.define("CMAKE_POLICY_DEFAULT_CMP0091", "NEW")
            .define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL");
    }

    // macOS always links Accelerate.framework — ggml-cpu's ops.cpp uses
    // vDSP_vsmul / vDSP_vsub / vDSP_vsadd UNCONDITIONALLY on macOS (CMake's
    // GGML_ACCELERATE auto-detection enables it whenever the SDK is
    // present, regardless of the metal feature). Without this, building
    // without `--features=metal` (or test targets that compile separate
    // units without consistent feature propagation) fails with
    // `ld: symbol(s) not found for architecture arm64` on _vDSP_vsub /
    // _vDSP_vsmul / _vDSP_vsadd. Apple's CoreFoundation is needed by
    // GGML's NSError plumbing on Mac too.
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=framework=Accelerate");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }

    // Metal on macOS — additional frameworks beyond the always-Mac set above.
    if cfg!(feature = "metal") && target_os == "macos" {
        cfg.define("GGML_METAL", "ON")
            .define("GGML_METAL_EMBED_LIBRARY", "ON");
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=MetalKit");
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

    // Vulkan on Linux — the Carl-on-Mac path. Containers on Apple Silicon
    // cannot access Metal directly (no GPU passthrough in Apple's hypervisor),
    // but Podman + krunkit routes Vulkan API calls out to a Vulkan-to-Metal
    // layer (MoltenVK) running on the host, achieving ~80% of native Metal
    // throughput. Runtime needs libvulkan.so.1 + a Vulkan ICD present.
    if cfg!(feature = "vulkan") && target_os == "linux" {
        cfg.define("GGML_VULKAN", "ON");
        println!("cargo:rustc-link-lib=vulkan");
    } else {
        cfg.define("GGML_VULKAN", "OFF");
    }

    let dst = cfg.build();

    // Link the static libraries cmake produced. cmake's MULTI-config generators
    // (Visual Studio / windows-msvc) nest libs under a per-config subdir as
    // `<name>.lib`, unlike the single-config Makefile/Ninja layout on Unix
    // (`lib<name>.a` directly in the search dir) — without the subdir, MSVC
    // fails "could not find native static library `common`". So link_search
    // emits the base path AND probes the filesystem for whichever config subdir
    // cmake ACTUALLY produced (Debug/Release/...), rather than trusting Cargo's
    // PROFILE which can diverge when a build.rs pins a cmake config (per M5's
    // review). No-op on single-config Unix (no such subdir exists).
    let link_search = |rel: &str| {
        let base = dst.join(rel);
        println!("cargo:rustc-link-search=native={}", base.display());
        for cfg_dir in ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"] {
            let sub = base.join(cfg_dir);
            if sub.is_dir() {
                println!("cargo:rustc-link-search=native={}", sub.display());
            }
        }
    };
    link_search("lib");
    link_search("build/ggml/src");
    link_search("build/src");
    link_search("build/tools/mtmd");
    link_search("build/common");
    println!("cargo:rustc-link-lib=static=llama");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    // libmtmd: multimodal projector + image/audio encoder. Loaded via
    // mtmd_init_from_file(mmproj_path, model, params); produces image
    // tokens that get evaluated alongside text via mtmd_helper_eval_chunks.
    // Depends on libcommon (string utils, base64 decoder).
    println!("cargo:rustc-link-lib=static=mtmd");
    println!("cargo:rustc-link-lib=static=common");
    // GGML backends register via C++ static initializers inside the backend's
    // static archive. Without +whole-archive, ld --as-needed / dead_strip
    // drops the archive because nothing from the main llama archive directly
    // references a symbol in it — so ggml_backend_metal_reg / _cuda_reg
    // never fire and the runtime falls back to CPU with a "backend missing"
    // error. Force-load the whole archive so every registration symbol is
    // preserved.
    // ggml-blas is built UNCONDITIONALLY on macOS — GGML's CMake enables
    // BLAS whenever Accelerate is auto-detected (always present in the
    // macOS SDK). The registry constructor in ggml-backend-reg.cpp.o
    // statically references `ggml_backend_blas_reg`, so the libggml-blas.a
    // archive must be linked regardless of the metal feature. Without
    // this, building with --no-default-features (or with feature paths
    // that don't enable metal) fails with:
    //   Undefined symbols for architecture arm64:
    //     "_ggml_backend_blas_reg" in ggml-backend-reg.cpp.o
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-blas");
    }
    if cfg!(feature = "metal") && target_os == "macos" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-metal");
    }
    if cfg!(feature = "cuda") && target_os == "linux" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-cuda");
    }
    if cfg!(feature = "vulkan") && target_os == "linux" {
        println!("cargo:rustc-link-lib=static:+whole-archive=ggml-vulkan");
    }

    // C++ stdlib + OpenMP (llama.cpp CPU backend uses GOMP_parallel on Linux).
    if target_os == "macos" {
        println!("cargo:rustc-link-lib=c++");
    } else if target_env == "msvc" {
        // windows-msvc: there is NO GCC-world `stdc++`/`gomp`. The MSVC C++
        // runtime is auto-linked through the `/DEFAULTLIB` directives the MSVC
        // compiler embeds in the ggml/llama `.obj` files, and OpenMP (when
        // ggml's CMake enables it) is pulled in the same way via the
        // `/openmp`-emitted `/DEFAULTLIB:vcomp` directive. So emit neither GCC
        // lib here — doing so is what caused `LNK1181: cannot open input file
        // 'stdc++.lib'`. windows-gnu (MinGW) keeps the GCC libs via the else.
    } else {
        // Linux / other GNU targets.
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=gomp");
    }

    // Generate FFI bindings for llama.h.
    //
    // We additionally include `ggml-metal.h` on Mac with the metal feature so
    // bindgen emits `ggml_backend_metal_reg` etc. — the symbols our
    // `backend_init()` calls explicitly to force-register the static Metal
    // backend.
    //
    // Why explicit registration is needed even with +whole-archive on Mac:
    // verified 2026-04-19 that `nm` on the linked test binary shows ZERO
    // `ggml_backend_metal_*` symbols even though `libggml-metal.a` defines
    // them and `libggml.a`'s `ggml-backend-reg.cpp` references them via
    // `register_backend(ggml_backend_metal_reg())` (which runs only if
    // `GGML_USE_METAL` is `#define`d — it is, per the CMake cache). Apple's
    // ld translates rustc's `+whole-archive=ggml-metal` to `-force_load` but
    // dead_strip can still drop the symbols when the only consumer is a
    // C++ static initializer in a sibling archive. Calling the registration
    // function explicitly from Rust at startup creates a hard reference
    // path the linker cannot strip — fixes "all 32 layers assigned to
    // device CPU" symptom that was forcing CPU-only inference at 33 tok/s
    // on M5.
    let llama_header = submodule.join("include").join("llama.h");
    let mtmd_header = submodule.join("tools").join("mtmd").join("mtmd.h");
    let mtmd_helper_header = submodule.join("tools").join("mtmd").join("mtmd-helper.h");
    let mut builder = bindgen::Builder::default()
        .header(llama_header.to_str().unwrap())
        .header(mtmd_header.to_str().unwrap())
        .header(mtmd_helper_header.to_str().unwrap())
        .clang_arg(format!(
            "-I{}",
            submodule.join("ggml").join("include").display()
        ))
        .clang_arg(format!("-I{}", submodule.join("include").display()))
        .clang_arg(format!(
            "-I{}",
            submodule.join("tools").join("mtmd").display()
        ))
        .allowlist_function("llama_.*")
        .allowlist_function("ggml_.*")
        .allowlist_function("mtmd_.*")
        .allowlist_type("llama_.*")
        .allowlist_type("ggml_.*")
        .allowlist_type("mtmd_.*")
        .allowlist_var("LLAMA_.*")
        .allowlist_var("MTMD_.*");

    if cfg!(feature = "metal") && target_os == "macos" {
        let metal_header = submodule.join("ggml").join("include").join("ggml-metal.h");
        builder = builder.header(metal_header.to_str().unwrap());
    }
    if cfg!(feature = "cuda") && target_os == "linux" {
        let cuda_header = submodule.join("ggml").join("include").join("ggml-cuda.h");
        builder = builder.header(cuda_header.to_str().unwrap());
    }
    if cfg!(feature = "vulkan") && target_os == "linux" {
        let vk_header = submodule.join("ggml").join("include").join("ggml-vulkan.h");
        builder = builder.header(vk_header.to_str().unwrap());
    }

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");
    let bindings = builder.generate().expect("Failed to generate bindings");
    bindings
        .write_to_file(&out_path)
        .expect("Failed to write bindings");
}
