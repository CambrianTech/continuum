# Portable CPU dispatch: recovery contract

Status: implementation proposal, not a deployed fix. Codex owns the Rust binding,
release packaging and BigMama verification; Fable independently reviews it.
CONTINUUM-ENGINE.md remains the serving design authority.

## Failure established on BigMama

The running core is release 4f96d32b973d, PID 18088. Its last retained embedding
lease, at 2026-11-16 20:58:33.564 UTC, admitted 32 texts and 1,529,872,384 host
bytes. That is exactly the existing 2,048-token window's calculated context
cost. Longer texts already use windows; adding another cap is not this fix.

A small native dump at 02:36 UTC captured four CPU-active threads in the same
scalar signed-byte dot-product loop. Exact executable RVAs: 0x8e51832,
0x8e51952, 0x8e519b4, 0x8e519e9. Disassembly shows scalar multiply/add and a
34-byte stride, not a thread-pool barrier. The specific symbol and full caller
stack remain unverified. This establishes the sampled execution path, not that
scalar arithmetic alone explains the entire elapsed time.

The publisher sets CONTINUUM_PORTABLE_CPU=1. core/llama/cpu_target.rs then
disables every optional x86 ISA extension. The resulting portable artifact
cannot use the receiving machine's vector instructions. Kimi's new turn waits
before cognition, in genome selection's embedding path; the earlier refused
turn did close. Do not remove completion custody or restart to erase this evidence.

## One selection owner, one complete artifact

Use vendored ggml's CPU backend scoring and runtime dispatch. Do not introduce
our own CPUID selector or require AVX2 on every recipient. For distributed x86
builds the native contract is GGML_NATIVE=OFF, GGML_CPU_ALL_VARIANTS=ON,
GGML_BACKEND_DL=ON and BUILD_SHARED_LIBS=ON. The baseline executable must remain
valid on the declared baseline CPU. Baseline and vector backends are deliberate
hardware variants, not recovery fallbacks for missing package files.

This is a coupled build, loader, packaging and installation change:

1. Replace the portable static link configuration, including MSVC cache identity
   checks. Link the shared llama/ggml dependencies with the correct import libraries;
   do not link a nonexistent static ggml-cpu target when CPU variants are modules.
   Keep required common/mtmd dependencies consistent with the actual CMake targets.
2. Load backend modules only from the verified installed artifact directory through
   ggml_backend_load_all_from_path. The present unqualified load_all searches the
   working directory too; that is not an acceptable release provenance boundary.
   Use the existing initialization owner and avoid double registration. A missing
   required backend or incomplete package is a reported installation failure.
3. Have the build emit its complete native runtime inventory. Packaging must include
   every CPU variant and its dependencies, not just DLLs visible in executable
   imports. Extend the existing runtime closure and integrity owners; do not create
   a second installer. Inspect dynamically loaded modules as closure roots as well.
4. Verify bootstrap CLI dependencies before adoption, and retain every installed
   module required by core and CLI across upgrade/rollback. On macOS x86, use artifact
   relative install names and verify the packaged closure; the current system-only
   dylib check must become a check of system libraries plus this exact owned closure.
5. Record the actually loaded module path, hash and CPU feature selection. A successful
   process start without that evidence does not prove vector dispatch.

No unrelated thread-count, persistent-context, scheduler or embedding-quality changes
belong in this first comparison. The standalone llama-server has a separate build
owner and must be audited independently; changing the Rust binding does not fix it.

## Acceptance evidence

Keep the existing scalar artifact as the comparison input. Replay the same recorded
32-text workload, including mixed lengths and multi-window inputs, with the same model,
tokenization, pooling and thread count. Record wall time, CPU time, peak memory,
embedding validity, cosine/ranking agreement and the exact loaded native artifact.
Do not claim identical floating-point vectors across ISA implementations without proof.

Exercise cancellation while queued and while native work runs: the existing worker
must retain the context and lease until completion, with no overlapping contexts and
no lost completion. Exercise a missing module, corrupted inventory, stale native build
cache, unavailable higher ISA and clean-machine package startup. Missing files must
fail before serving; a supported older CPU must select its legitimate supported module.

Review the exact head and its CI, then deploy through the existing lifecycle owner.
Verify running core and module hashes and observe 15 minutes of actual resident work:
gene selection returns, Kimi completes acts, Sahar obtains service, and memory remains
accounted for. No performance or recovery claim follows from tests or liveness alone.
