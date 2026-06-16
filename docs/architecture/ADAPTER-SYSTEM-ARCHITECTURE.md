# Adapter System Architecture — One Core, Thin Everywhere, Run On Anything

**Status.** Canonical doctrine for how we architect the boundary between the core and every surface that touches it — CLIs, web, iOS/Android, AR, Unity, agentic clients, personas, Python, anything. Crystallized 2026-06-15 in a working conversation with Joel; this document is the durable, shareable artifact. If a boundary/SDK/client question disagrees with this doc, this doc wins.

**Companion to:**
- [MODULE-ARCHITECTURE.md](MODULE-ARCHITECTURE.md) — everything is a module, everything to a module is a command; §9 (Pure-Rust built-ins vs WASM shipped modules) is the distribution half of this story.
- [../UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) — `Commands.execute` / `Events` — the two primitives every modality calls by name.
- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the RTOS-style core every Rust module inherits.
- [ADAPTER-MARKETPLACE.md](ADAPTER-MARKETPLACE.md) / [CHANNEL-ADAPTER-INTEGRATION.md](CHANNEL-ADAPTER-INTEGRATION.md) — adapters as *shipped artifacts* and adapters *inside* cognition, respectively. This doc is the boundary/SDK doctrine that sits under both.

**Audience.** Any human or AI agent authoring an SDK, a client, a language binding, a modality, or proposing a new boundary. Read this first; do not hand-roll a parallel boundary.

---

## 1. The Principle: adapter-obsessed, not language-obsessed

> The invariant is the **adapter**: one stable interface, many swappable concrete implementations, runnable anywhere. The language and the artifact format are implementation details chosen to never tax the hardware.

We are not "Rust-obsessed." We are **adapter-obsessed.** Rust happens to be today's best *adapter substrate* — zero-cost abstractions (the seam costs nothing), cross-compiles to every architecture and target, FFIs to everything — so it gives us the clean, swappable boundary **without** the usual abstraction tax. If a better substrate appears, the doctrine survives; the substrate is replaceable, the adapter shape is not.

Everything outward of the core is *a packaging of an adapter*: a Docker image, an Android **AAR/Maven** artifact, an iOS **xcframework** (all archs), a **WASM** component — the *same* modular, run-everywhere strategy in different envelopes. Uniform interface on the outside; freedom to hit the metal on the inside.

## 2. One core. All logic. Speed first.

- **Put all logic that can be into the ONE core, compiled native.** Speed is the *first* question asked, not a later optimization.
- **Logic never lives in an interpreter.** Not a slow JS engine, not a script-loader, not an interpreted plugin. The failure mode we design against: build something complex in a slow interpreted layer, then later rip it out because it is *both* slow *and* nobody understands it — performance debt and comprehension debt at once. One native core kills both.
- **Portability must never compromise speed, cryptographic, or other hardware features (the GPU especially).** The adapter seam must be zero-cost, or "runs everywhere" silently became "runs everywhere, slowly." (This is why, e.g., the Mac compose profile runs the core *natively* for Metal instead of in a container — the artifact stays modular; the implementation refuses to give up the GPU.)

Decision rule: **"Can this be in the core?" → if yes, it is, once. Clients are thin. Speed first.**

## 3. What "thin" means: zero logic, not small

"Thin" is the most misused word in boundary design. Here it has a precise meaning:

> **Thin = zero logic, pure delegation. NOT small.**

A *generated framework* that wraps the core is still thin no matter how much surface it covers, because it holds **no decisions** — it forwards them. Coding a framework that wraps the core is no big deal; it is still thin. The liability is never lines of code in a modality — it is **logic** in a modality.

So the test flips from *"how much code?"* to *"does this layer decide anything, or just route?"*

| | Thin (fine) | Thick (liability) |
|---|---|---|
| Forwards a call to the core by name | ✅ | |
| Reshapes a type for idiomatic ergonomics (see §4) | ✅ | |
| Branches on business rules / makes a decision | | ❌ |
| Reimplements something the core already does | | ❌ |
| Holds state that the core should own | | ❌ |

## 4. The one exception: idiomatic type-shaping

The *only* "logic" allowed in a thin SDK is a **conversion/transform — middleware-like — and only to keep the typings sane and idiomatic for that language's external API users.**

A Kotlin, Swift, or TypeScript developer opening a class definition should see something **rational in their own language**: an idiomatic enum, a `Result`, a `Date` instead of an `i64`, `camelCase` instead of `snake_case` — not a raw FFI blob. That is *presentation of types*, not decisions.

The bright line:
- **Shaping a type = fine.** (No branches that matter, no business rules — it just makes the boundary read sanely.)
- **Deciding anything = thick = liability.**

Do not mistake a legitimate idiomatic-type transform for a "zero logic" violation; equally, do not let real logic smuggle itself in under the "transform" label.

## 5. The hierarchy: SDKs stack, they don't all re-bind the core

Modalities get access on a **hierarchical level** — SDKs *wrap each other*, they don't each re-bind the core:

```
                     ┌─────────────────────────────────────────────┐
   the one core ───► │ logic lives here, once, native, fast         │
   (Rust today)      └─────────────────────────────────────────────┘
                                      │  generated, idiomatic-typed
                     ┌────────────────┴────────────────┐
                     ▼                                  ▼
            native SDK (Swift / iOS)          native SDK (Kotlin / Android)
                     ▲                                  ▲
                     └───────────────┬──────────────────┘
                                     │ wraps the native SDKs (not the core directly)
                          Flutter / React Native
                                     ▲
                                     │ call by name
   CLI · web · AR · Unity · agentic · personas · Python · …anything
```

Each tier is thin (delegation + at most an idiomatic-type transform) and wraps the tier beneath. **A bug fixed once in the core propagates *up the whole stack*** — regenerate the native SDK, Flutter/RN inherit it, no tier re-implements anything. This is what makes "an SDK for literally every kind of device on the planet" tractable: one core of logic, a generated native tier per platform, cross-platform frameworks wrapping *those*, every modality above calling by name.

## 6. Boundaries: elegant, automated, ideally generated

The boundary code must be **elegant and automated — ideally practically generated.** Generation earns its place *only* if it:
1. stays **beautiful** (idiomatic, readable output),
2. doesn't **kink the flywheel** (the build/test/dev loop), and
3. adds **no tech debt**.

If a codegen tool emits ugly output or slows the loop, it is worse than the problem it solved — reject it.

**Recommended stack (better than hand-written cbindgen, which stops typing at the C header and leaves memory management manual):**

| Boundary | Tool | Why |
|---|---|---|
| Multi-language native SDK (Swift, Kotlin, Python, …) | **UniFFI** | Annotate the core; generate idiomatic, strongly-typed bindings with errors-as-types, async, callbacks, correct ownership. Collapses N×M hand-glue. |
| Node / desktop | **napi-rs** | Auto `.d.ts`, N-API stable ABI, async, zero-copy buffers. Complements `ts-rs` wire types. |
| Flutter / Dart | **flutter_rust_bridge** (or UniFFI's Dart backend) | Auto Dart bindings, async, streams. |
| Wire / IPC / grid (messages, not calls) | serde-JSON + **ts-rs** for ergonomics; **FlatBuffers** / prost for hot, cross-language, zero-copy paths | Strong typing across the message seam. |

**Never** reach for script-loading or interpreter glue at the boundary — it is slow, throughput-limited, *thick and slow at once*.

**The performance caveat that keeps "speed first" honest:** UniFFI / napi-rs / flutter_rust_bridge *marshal* — excellent for the control/command surface, not raw-pointer-fast for huge payloads. So keep the split: **generated strongly-typed SDK on the command surface; raw FFI + shared memory for the hot seam** (GPU handles, renderer frames). Simple SDK on top; zero-copy underneath where it counts.

**WASM note (see MODULE-ARCHITECTURE §9):** shipped/third-party/per-user modules can be WASM components — one artifact, every platform. But *logic never goes in an interpreter*: if it must ship as a loadable module, **AOT-compile it** (near-native). Interpreter-mode WASM (e.g. the iOS no-JIT path: WAMR/wasm3/Pulley) is acceptable only for thin/untrusted/sandboxed *edge* cases, never for core logic or hot paths.

## 7. The acid test, and modalities as liabilities

> **Fix a bug once, in the core, and it is fixed for every modality — because no modality re-implemented it.**

This is the operational definition of every rule above. If a fix forces you to touch N surfaces, those surfaces were thick, and they were liabilities the whole time. The flip side is the payoff: a *new* modality should cost ~zero logic — generate the wrapper and it inherits every past and future fix for free.

> **Modalities cannot become liabilities — which they will, if they are not thin.**

A modality that re-decides anything is the second copy that drifts. The goal is not "less code" for its own sake; it is: *every decision lives once, every modality is a generated pass-through, and both a new modality and a bug fix cost nothing extra.* **Maximal reach, minimal logic, zero duplication, nothing coded twice.**

## 8. How this already lives in continuum (and where the debt hides)

Already the spine, just not always named:
- **The polymorphism pattern** (trait + `AlgorithmRegistry` + swap-by-name) — adapters, in `CLAUDE.md`.
- **AI provider adapters, inference adapters**, `GovernorSilicon`/`TargetSilicon` (AppleM / CUDA / ROCm / Vulkan) — *same Rust, different metal, selected by adapter.*
- **`HeuristicInferenceAdapter`**, the RAG `RecordingRagSource` / `ReplayRagSource` — adapters for test vs live.
- **The IPC `bindings/modules/*.ts` mixins + `ts-rs` generated types** — thin generated wrappers, no logic. ✅
- **The command generator** (scaffold + README + help) — boundary automation; "everything is a command" + dynamic discovery + call-by-name means modalities *call*, never reimplement.
- **`crate-type = ["cdylib","rlib"]` + `src/ffi/mod.rs`** ("FFI bindings for Node.js and Swift") — the native AAR/xcframework foundation, the opaque-handle FFI pattern.

⚠️ **Danger zone:** anywhere a client still holds logic that belongs in the core — e.g. chat-specific persona logic in the TS layer (see [PERSONA-COGNITION-PIPELINE.md](PERSONA-COGNITION-PIPELINE.md)). Those are thick modalities; the acid test will keep flagging them until they are pure pass-throughs. Migrating that logic core-ward *is* the work.

## 9. Authoring checklist

When you add an SDK, a client, a binding, or a modality, every box must be checkable:

- [ ] **No decisions.** This layer routes; it does not branch on business rules or hold state the core should own.
- [ ] **The only transform is idiomatic type-shaping** (§4), and you can name exactly which type it makes sane.
- [ ] **Generated, or trivially could be.** If hand-written, justify why codegen would be uglier / slower / debt — not just "it was easy to type."
- [ ] **Wraps the tier beneath, not the core directly** (if a cross-platform framework). §5.
- [ ] **Speed not compromised.** Hot/GPU/crypto paths go through raw FFI + shared memory, not the marshaling SDK.
- [ ] **No interpreter for logic.** Native or AOT; interpreters only at the untrusted edge.
- [ ] **Passes the acid test.** A bug fixed in the core needs zero changes here.

If any box fails, the modality is a liability in waiting. Make it thinner, or move the logic into the core.
