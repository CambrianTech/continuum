# Where the code stands

Survey of canary c819052ef7, 2026-11-20. Receipts are file:line on canary; implementor counts
include test doubles. Ranked by how much logic each cluster rewrites per leaf. Each cluster closes by
lifting the shared logic to its narrowest ancestor, moving every leaf onto it, and deleting the
copies in the same change.

[Parts](README.md)

1. **Component lifecycle, health and initialization: six unrelated roots.**
   - **The roots:** `ServiceModule` (102 implementors; `continuum-kernel/src/runtime/service_module.rs:247`), `AIProviderAdapter` (14; `continuum-engine/src/ai/adapter/inference_device.rs:389`), `SpeechToText`/`TextToSpeech` (`continuum-live/src/live/audio/stt/mod.rs:73`, `tts/mod.rs:135`), `GridTransport` (`continuum-grid/src/grid/transport.rs:88`), `Daemon` (`runtime/daemon.rs:113`) and `BrainRegion` (`runtime/brain_region.rs:399`).
   - **What each declares:** its own initialize, shutdown and health surface. Only `AIProviderAdapter` extends another trait.
   - **The resulting copies:**
     - 34 `fn shutdown` bodies.
     - 19 health checks in 15 files, while `ServiceModule` has no health method.
     - Parents hand-write their child loops: STT then TTS shutdown appears twice almost line for line (`continuum-live/src/live/audio/resource_lifecycle.rs:212-250`, `continuum-core/src/modules/live/voice_module/resources.rs:75-105`), and the grid loops over its transports in four places (`continuum-grid/src/grid/mod.rs:266, 292, 337, 517`).
   - **What already exists:** the only folding base is `Lifecycle` (`continuum-persona-engine/src/persona/lifecycle.rs:6`), used only inside that crate. The runtime does fan drain, save and shutdown over its modules (`runtime/runtime/runtime.rs:52-68`).
   - **Target:** the [lifecycle](LIFECYCLE.md) on the `Part` root (`continuum-base::part`, decided with Codex), with [model parts](MODELS.md) as the first family. Owner: Fable. Done so far: `Part` identity for the 13 speech backends (#6204, approved); `Lifecycle`, the composite walk and her five parts on it (#6206).
2. **Pressure, footprint, leases and admission: parallel models.**
   - **Overlapping bases:** `ResourcePool` (16 implementors; `paging/pool.rs:188`), `ResourceConsumer` (10; `resources/consumer.rs:126`), `FootprintSource` (1), `EvictionRegistry` (`gpu/eviction_registry.rs:101`) and `BrainRegion::pressure_profile`.
   - **The resulting copies:**
     - About 150 Lease, Permit, Grant, Gate and Admission types.
     - Name collisions (`Admission` four times, `LaneAdmission` twice).
     - `memory_pressure_monitor.rs:99` and `disk_pressure.rs:547` each define their own `pressure()` beside `ResourcePool`.
   - **Target (owner: Codex):** the one governor of [SHARED-RESOURCES.md](../SHARED-RESOURCES.md). The hold, its expiry and its visibility are written once in a lease root. Memory, slots, ports and seats are specializations of it. Each holder supplies only its typed reduction (the `Reduction` example above).
3. **Small utilities re-rolled per crate.**
   - **Backoff:** three formulas (`continuum-core/src/airc/daemon_liveness.rs:52`, `airc/reattach.rs:48`, `continuum-mind/src/persona/host.rs:817`) and about seven retry helpers, with no shared base.
   - **Monitors:** `Daemon` is the base, yet `watch::channel` is created 115 times in 66 files, and hand-written interval monitors bypass it (`continuum-grid/src/grid_allocator.rs`, `continuum-mind/src/modules/cognition.rs`, `continuum-vitals/src/activity_gate.rs`).
   - **Home paths:** the base delegates correctly (`continuum-kernel/src/paths/mod.rs:62` calls `continuum_base::home`), but `serving-replay/src/main.rs:37` keeps its own copy and 25 raw `HOME` reads in 20 files bypass it.
   - **Other families:**
     - 36 registries, with collisions (`Registry` three times, `AdapterRegistry` twice).
     - 13 keyed caches with no common base.
     - 48 validators and 65 refusal types with no common base.
     - Atomic writes repeated in 47 files.
     - Token estimators bypassing `continuum-base/src/token_estimate.rs`.
   - **Target:** each family gets one floor owner (`continuum-base` or a crate named for the pattern, plan phase 5). The copies move onto it and are deleted.
4. **The mind is not one tree yet.**
   - **Two disconnected roots:** `PersonaUnit` (`continuum-persona-engine/src/persona/unit.rs:9`) composes her five parts under `Lifecycle`, and nothing outside its crate uses it. `PersonaCognition` (`continuum-mind/src/persona/unified.rs:27`) holds about a dozen parts with no common trait and no shutdown, suspend or footprint. `PersonaAircRuntime` tears down through an ad hoc `Drop`.
   - **Target:** the §9.4 tree under one `Persona` root, on the `Part` lineage of cluster 1, with her regions ([BRAIN-REGIONS-SUBSTRATE.md](../BRAIN-REGIONS-SUBSTRATE.md), "Fusion") as children.

Size today: 52 crates under `core/`, and the largest files sit just under the 1,000-line gate (25
files between 947 and 999 lines, one vendored file at 1,269). They pass the gate without being
small. The gate is a ceiling, not the goal: a file holds one object's own difference.


## Order of cuts

1. `Lifecycle`, `Outcome`, the composite walk and `ReloadableModel` land in `continuum-base::part`;
   persona engine's `Lifecycle` moves onto it and its copy is deleted.
2. The audio backends become model parts ([MODELS.md](MODELS.md)); their statics, their own
   lifecycle methods and the duplicated release loops are deleted.
3. The inference adapters, then the grid transports, then `ServiceModule`, each its own change.
4. Cluster 2 proceeds in parallel (Codex); cluster 3's utilities each get one floor owner as they are
   touched; cluster 4 follows cut 1.
