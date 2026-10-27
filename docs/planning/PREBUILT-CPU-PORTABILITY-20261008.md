# Published CPU instruction contract — 8a3f42ec

The Windows 9f11 core crashed during startup on an Intel Core Ultra 9 285K.
Its private crash dump records exception `0xc000001d` at core RVA `0x6e313d5`.
The matching preserved executable disassembles those exact fault bytes as
`vmovups zmm0,zmmword ptr [...]`: an AVX-512 instruction. The host's runtime
feature probe reports AVX2/FMA supported and AVX512F unsupported. The dump
contains process memory and stays private; only this bounded diagnosis is recorded.

The 8169-to-9f11 source changes did not change the native build or llama pin.
The shared llama build nevertheless left `GGML_NATIVE` enabled on Windows;
MSVC's vendor configuration probes the build host and can select `/arch:AVX512`.
A successful CI run or `--version` cannot establish that such an artifact can
initialize a model on a different CPU. Precise native function attribution is
unavailable without symbols; the unsupported instruction itself is verified.

`core/llama/cpu_target.rs` now owns the published x86 CPU definitions.
The existing core binary publisher explicitly selects `CONTINUUM_PORTABLE_CPU=1`.
The shared build script disables host probing and explicitly clears every
optional x86 SIMD extension, including defaults and previously cached settings.
This uses the x86_64 SSE2 baseline rather than quietly imposing a new AVX2
minimum. It does not change GPU feature selection, ARM compilation, or the
separately installed native llama-server. Local source builds retain native
selection; changing build mode is tracked by Cargo and resets that selection.

This conservative portable CPU path may reduce CPU inference throughput.
Runtime CPU variants remain separate work: upstream requires dynamic backends,
while this product intentionally uses static backends after previous GPU
initialization failures. This repair does not reintroduce that loader path or
claim equivalent throughput. Measure embedding initialization and latency on
the actual non-AVX512 host before closing installed acceptance.

The lightweight native-helper CI job exercises a previously native cache,
portable overrides without an AVX2 floor, GPU setting preservation, ARM scope,
and switching back to a local build. It also checks the publisher selects the
mode. The standalone Windows regression passed (one test, 0.00 seconds), without
building the core or using the shared Cargo cache. Independent review and CI
remain required. Full artifact build and real model initialization remain
required; no installed repair is claimed by source changes alone.
