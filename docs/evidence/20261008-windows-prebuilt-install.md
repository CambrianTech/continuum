# Fresh Windows prebuilt installation — card68a33e89

The public Windows installer previously provisioned Rust, MSVC, CMake, LLVM and CUDA, then compiled both the core and serving engine. The existing CI archive only contained the core applications; its update companion path still invoked an engine source build. A developer machine's installed engine hid this gap.

For the core and serving engine, the ordinary installer now consumes the published Windows NVIDIA flavor. `-DeveloperBuild` explicitly retains contributor compilation; an absent or incompatible publication refuses instead of selecting that path. Preparation, activation and recovery retain the same provision-once supervisor owner from PR4869.

The first-CLI transport checks the requested revision/platform/archive identity and archive SHA256 from the configured GitHub release source, pins that archive, then extracts only exact regular CLI/runtime members into a fresh directory. The runtime subset comes from actual publisher import inspection: review found VCOMP140.DLL in an older GPU-free published CLI, so GPU-free alone does not establish fresh-host loader readiness. The published CLI's `prepare-prebuilt` entry then uses the existing artifact consumer's compatibility, GPU, hashing and extraction path. Build-key inputs are projected from the Rust owner by the existing bootstrap generator. The common extractor refuses traversal, duplicate members and links. This checksum is transport integrity from the trusted release source, not an independent signature.

The existing Windows engine builder runs in CI with explicit published CUDA targets and the shared portable CPU definitions from PR4870. Its existing `engine-install.json` owns the complete application DLL namespace and provenance; the outer archive hash binds that namespace. Fresh installation and Windows native update staging use the same receipt validation/copy path. No new engine receipt schema or fallback GPU policy is introduced. CPU-only Windows remains unsupported by the currently published flavor.

Local evidence: all eight existing artifact-policy tests passed, including extended fresh-engine and extraction regressions. The existing PowerShell5.1 installer fixture passed 34 scenario groups, including public default preparation choosing published artifacts, explicit developer preparation retaining its toolchain checks, and the GPU-less CI engine/import contract. Bootstrap projection drift and PowerShell/Rust parsing checks passed. Strict lifecycle Clippy passed in 1.21 seconds; the full core CLI check passed in 3 minutes 49 seconds. After the runtime-closure correction, the full 34-group PowerShell fixture passed again, including actual tar extraction of the declared first-CLI DLL and shared CMake/OpenMP closure checks. Final incremental core CLI check passed in 2.94 seconds and strict lifecycle Clippy passed. Independent cleanup_fix source review approved the complete draft, including the pinned bootstrap extraction and failure cleanup.

Installed acceptance remains outstanding: build and publish the complete compatible archive, exercise the ordinary public installer on a machine without developer toolchains, verify running core/engine revisions and actual consumer behavior, then verify two routine handoffs and recovery without task re-registration. No live service changes, bucket returns, or learning claims were made for this patch.

The named shared runtime closure owner replaces approximately 26 lines of workflow CUDA-only traversal and 15 lines of engine-specific DLL copying. Both publisher and engine now call windows-runtime-closure.ps1 and the existing CMake import resolver; one exact redistributable list is consumed by both. Existing fixture coverage is extended in place.

A separate full fresh-host dependency remains: Mod-Airc invokes the AIRC public installer, whose current no-argument path still builds from source. That shared AIRC installer needs its own supported prebuilt preparation before the complete dependency chain can be called source-free. This patch makes no such complete-chain claim. The existing LiveKit download helper is also not currently wired into the fresh native installer; multimodal readiness is not demonstrated here.

CI job113579104054 exposed a missing dependency in the existing process fixture's miniature repository: the expanded bootstrap generator now also reads the canonical artifact inputs and their PowerShell projection. The fixture now copies those real source files and checks prebuilt-input drift as well as launcher drift. The full Windows PowerShell 5.1 process fixture passed after this correction (7.3 seconds); no generator or runtime behavior was weakened.

Packaging follow-up: the actual Windows publisher compiled its engine, then
failed because windows-engine-receipt imported windows-prepared, whose path
validation relied on ConvertTo-CoreImagePath defined only in windows-service.
Normal service fixtures had preloaded that unrelated module and masked the gap.
The existing normalization implementation now belongs to windows-prepared;
service and engine receipt consumers both reach it through their current imports.
No duplicate implementation remains. A fresh isolated PowerShell runspace in the
existing service fixture loads only win-modules and validates a scratch engine
receipt. Full Windows PowerShell5.1 fixture passed35groups. Actual artifact
republication remains pending; this did not rebuild native code or install live.

TLS packaging follow-up: CMake selected OpenSSL under Program Files/OpenSSL,
but the runtime search roots included only VC and CUDA. The shared runtime owner
now also reads the engine's configured OPENSSL_INCLUDE_DIR and admits the bin
folder from that same installation; no PATH replacement, TLS disabling, or
import exclusion is used. Dynamic imports still fail closed if unavailable.
The existing PowerShell fixture covers configured-package discovery and actual
CMake capture/hash verification for libssl and libcrypto. Full PS5 fixture35groups
passed. A separate quick real-PE check used installed Git OpenSSL with the real
MSVC dumpbin and CMake: shared Copy-CoreRuntimeClosure captured both TLS DLLs and
verified their staged dependency graph. Native engine rebuild/publication remains
pending; no serving state changed.

Transitive VC packaging follow-up (2026-10-09): the publisher already staged
selected MSVC redistributables, but CMake's discovery of an external OpenSSL DLL
resolved its VC imports from System32 before the supplied runtime directories.
The existing fixture reproduced this with all nine declared VC names. Capture
now accepts that discovery result only when a unique selected runtime source and
the already-staged application copy have identical SHA256; it never copies the
System32 file. The mandatory second application-local verification retains the
System32 refusal. Missing or corrupted staged copies still fail closed.

The existing PowerShell5.1 fixture passed35groups, including transitive discovery,
all nine runtime names/hashes, missing-copy and corrupt-copy regressions. A bounded
real-PE audit with actual MSVC dumpbin/CMake inspected copies of engine-b,
published616058 CLI and Git OpenSSL together: all six discovered CUDA/TLS/OpenMP
DLLs were captured and the final graph verified. It did not execute those binaries;
the temporary directory was removed. Raw local receipts: 20261009-vc-closure-before.log,
20261009-vc-closure-after.log and 20261009-full-real-pe-closure.json in the team-proof
state directory. This covers the installed sample, not the unpublished CI engine;
actual corrected publication and fresh-user installation remain pending.
