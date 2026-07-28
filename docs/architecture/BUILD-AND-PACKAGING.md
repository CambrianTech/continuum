# Build, Startup, Testing & Packaging — the Foundation

> **Premise** (Joel, 2026-06-21): *"Startup and testing are major weaknesses. We wanted clean startup, so good that reliably these personas are on airc for you to code with, across our grid too, as a foundation, from install to our iterative development, so we can build task trays, web, mobile, on top anew, in dockerized nodes we can roll out like kubernetes. We need adherence to strict principles and design first."*

This is the **design-first** contract for how the system is installed, started, tested, packaged, and rolled out. It is deliberately strict: startup and testing have been the two weakest links, and the cost of leaving them ad-hoc is the recurring "old Node tangent" that derails real work. Everything below exists to make the **foundation reliable** — a headless Rust core with personas live on airc — so that task trays, web, and mobile can be built *on top, anew*, and rolled out as dockerized nodes.

If you are about to add a start script, a test harness, a Dockerfile, or a package step, read this first. If it disagrees with reality, fix reality or fix this doc — don't add a parallel path.

---

## 1. The foundation thesis

```
install ──▶ start (headless Rust core) ──▶ personas live on airc ──▶ iterate WITH them
                                                                         │
                                          ┌──────────────────────────────┤  built ON TOP, anew
                                          ▼              ▼                ▼
                                      task trays        web            mobile
                                          └──────────────┴────────────────┘
                                                  dockerized nodes ──▶ k8s rollout (grid)
```

The bottom of the stack — **install → start → personas on airc** — must be boringly reliable. It is the surface every other thing is built on and the substrate we *develop with* (the personas are collaborators across the grid, not a demo). The layers above (clients) are rebuilt fresh on the uniform client SDK; they are consumers, never privileged.

---

## 2. Strict principles (non-negotiable)

1. **Design first.** A new startup path, test layer, or deployable unit gets designed here before it gets code. No second start orchestrator, no parallel test framework, no bespoke Dockerfile that duplicates an existing one.
2. **One reliable startup, pure Rust.** The headless core starts via **`continuum start`** → `tools/scripts/start-server.sh` → `cargo run` the `continuum-core-server`. No Node in the core start/build/test path. The legacy Node orchestrator is quarantined (`legacy/`), out of every build/CI/Docker path. (`[[validate-via-pure-rust-not-npm-jtag]]`, `[[rust-is-the-core-node-is-the-shell]]`.)
3. **The core is headless; clients are equal consumers.** Desktop/web/mobile/CLI/task-trays all ride the uniform `continuum_client` `Connection` over a transport; none is privileged (`[[headless-core-many-clients]]`, `[[client-sdk-platform-architecture]]`). `continuum` is the reference CLI client *and* the lifecycle surface.
4. **Modular units = build units = containers.** Each deployable is a workspace member → a binary → (optionally) a container. The unit boundary is the same at every layer; you don't re-slice the system per packaging target.
5. **Testing is a first-class deliverable, layered and deterministic.** Unit + integration tests are pure `cargo test` with **zero live deps** (no airc, no unsloth, no models, no network). Live behavior is proven by an explicit, opt-in smoke path (`continuum start` + `continuum ping` + a persona turn), never baked into CI's default gate. (Reinforces the test doctrine in CLAUDE.md: one test mod per file, stress behind `stress-tests`, fixtures behind `test-fixtures`, every test names the invariant it guards.)
6. **No Node in the foundation.** Node exists only to *build the web client SDK/app* — a leaf consumer. It is never a dependency of core build, core start, core test, or the grid. If a foundation step reaches for `npm`/`tsx`, that's the bug.
7. **Move-first to excise legacy.** Retiring something means moving it to `legacy/` and letting the build/refs break to map the coupling, then fixing to the pure replacement — never patching the old thing in place (`[[move-first-let-compiler-find-the-smell]]`).
8. **Single dynamic command surface — no duplicated lists, ever.** Every command is callable through `continuum <command> [json]` because `continuum` forwards the command *string* to the core and the core routes it through the ONE registry (`command_registry` + the `DynCommand` object map). `continuum` enumerates nothing; the persona tool surface, the ACL, and codegen all read the *same* registry. A command is declared in exactly ONE file (its `CommandSpec`/`ActionCommand` + `register_command!`); removing or renaming it updates that one place and everything follows. **No central command list, no switch-on-name, no per-caller catalog** — if removing a command means editing 20 strings or `match` arms, that's the bug (the anti-pattern CLAUDE.md forbids). Command *discovery* is dynamic too: `commands/list` returns the live registry, so clients/trays never hardcode a catalog.

---

## 3. Modular units (workspace member → binary → container → role)

The Cargo workspace (root `Cargo.toml`) is the single source of build units. The deployables:

| Unit | Workspace member | Binary | Container | Role |
|---|---|---|---|---|
| **Core** | `core/continuum-core` | `continuum-core-server` | `docker/continuum-core.Dockerfile` (+`-cuda`, `-vulkan`) | The headless substrate: commands, events, persona hosting. The foundation. |
| **MCP sidecar** | `core/continuum-core` | `continuum-mcp` | (co-located w/ core) | Rust MCP server over stdio → core IPC socket. What MCP clients (Studio, Claude Code) spawn. |
| **CLI / lifecycle** | `core/continuum-core` (bin) → **should migrate to `apps/cli`** | `continuum` | n/a (or thin) | `continuum start`/`stop` (lifecycle) + `continuum <command>` (uniform client). Replaces Node `./jtag`. |
| **Inference** | `core/inference-grpc` | `inference-grpc` | (per topology) | GPU inference lane host. |
| **LiveKit bridge** | `core/livekit-bridge` | `livekit-bridge` | `docker/livekit-bridge.Dockerfile` | Realtime media bridge. |
| **unsloth gateway** | *external* | — | external service | Universal model gateway (`/v1` chat+embeddings, `/api`). Not ours to build; a service we address. (`[[unsloth-universal-model-gateway]]`) |
| **Clients (new)** | `client/*`, `apps/*` | web / mobile / desktop / task-trays | `docker/node-server.Dockerfile`, `docker/widget-server.Dockerfile` | Built ON TOP, on the uniform client SDK. Node only here. |

**Principle 4 in practice:** `continuum`'s correct long-term home is `apps/cli` (it's a client, not core). It currently lives as a `continuum-core` bin for fast iteration (precedent: `continuum-mcp`); migrating it to `apps/cli` is tracked, not blocking.

---

## 4. Startup (weakness #1 — fixed)

**The one path:**

```bash
continuum start      # build (cargo, per-platform GPU features) + run the core detached, wait until it answers ping
continuum ping       # confirm
continuum stop       # stop the detached core (process-group SIGTERM)
```

- `continuum start` locates `tools/scripts/start-server.sh` (env `CONTINUUM_START_SCRIPT` override, else walk up from cwd), spawns it in its own session (`setsid`) so the core outlives the CLI, logs to `/tmp/continuum-core-start.log`, writes a pidfile, and polls `ping` until ready (or fails loud with the log tail). Idempotent.
- `start-server.sh` is the **implementation detail**: pure-Rust `cargo run` with per-platform features (Darwin arm64 → `metal,accelerate`; Intel Mac → cpu-only; Linux/Windows → detected), airc context auto-discovered, builds core + mcp + continuum. **No Node.**
- `npm start` (root and `src/`) just calls `start-server.sh` — kept only as muscle-memory alias; it is not a Node orchestrator.

**Forbidden:** a second start script; reintroducing `parallel-start.sh` (quarantined in `legacy/node-startup/`); any start step that blocks on a flaky external (model download, scene-gen) — those are separate, optional, and must degrade, never gate the core coming up.

**Graceful degrade is mandatory.** A missing optional dependency (ONNX/ORT dylib, unsloth embeddings, STT model, Tailscale) logs a WARN and the core still serves. The live boot already demonstrates this (unsloth `/v1/embeddings` 501 → lexical fallback; ORT missing → isolated). A missing *optional* must never abort startup. (`#26`: faculties degrade, never panic.)

---

## 5. Testing (weakness #2 — the discipline)

Three layers, strictly separated by their dependency surface:

| Layer | Command | Deps | Gate |
|---|---|---|---|
| **Unit + integration** | `cargo test -p continuum-core` | NONE (no airc/unsloth/models/network) | CI default — must be green |
| **Stress / concurrency** | `cargo test --features stress-tests … stress` | in-process only | opt-in; sign-off + perf curves |
| **Live smoke** | `continuum start && continuum ping && <persona turn>` | airc + a model | manual / scheduled; never the CI default |

Rules (extend, don't reinvent — these are the CLAUDE.md test rules, restated as foundation policy):
- **Deterministic by default.** Anything depending on airc/unsloth/models/network is a *live smoke*, not a unit test. The persona→command path is proven deterministically (`cognition::tool_executor::…::persona_executes_ping_via_typed_object_path`) with no live deps — that is the pattern.
- **One `#[cfg(test)] mod` per file**; stress behind `#[cfg(feature = "stress-tests")]`; fixtures behind `#[cfg(any(test, feature = "test-fixtures"))]`.
- **Every test names the invariant it guards** (`// what this catches:`), and regressions link the issue/commit.
- **Shared cargo target.** Always `export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"` for hand-run cargo so artifacts land in the one cache, not ghost `target/` dirs.

The integration-smoke gap (`#22`: boot core + continuum-mcp smoke) is the next test investment: a scripted `continuum start` → assert socket + `continuum ping` + one persona turn, run on demand, so "did the wiring actually come up" is a one-command answer.

---

## 6. Packaging & rollout — dockerized nodes → k8s

The Docker surface already exists and is the rollout unit:

- **Compose topologies:** `docker-compose.yml` (base), `docker-compose.gpu.yml`, `docker-compose.mac.yml`, `docker-compose.airc.yml` — select by host capability.
- **Per-unit Dockerfiles:** `docker/continuum-core.Dockerfile` (+`-cuda`, `-vulkan` for GPU backends), `docker/livekit-bridge.Dockerfile`, `docker/model-init.Dockerfile`, plus client images (`node-server`, `widget-server`).
- **Build/push:** `scripts/ensure-docker.sh`, `scripts/push-current-arch.sh`, `scripts/push-image.sh` (`npm run docker:push*`).

**Target shape (design, not all built):** each modular unit (§3) is one container image; a **continuum node** = `{ continuum-core + continuum-mcp + continuum }` co-located, addressing the local core over its IPC socket, joining the grid over airc. A node is the **kubernetes rollout unit** — replicate nodes across the grid, each a citizen-host (`[[docker-personas-as-grid-peers]]`). unsloth is a sibling service addressed over `/v1`, scaled independently. Clients (task trays/web/mobile) are separate deployments consuming the grid through the SDK.

**Addressing per topology** (`#23`, `#27`): in-node = IPC socket; cross-node = airc peer URI; the command/route layer already abstracts both (`COMMAND-ORGANIZATION.md` §"agnostic of machine and environment"). k8s service discovery maps onto airc peer discovery, not a parallel mechanism.

**Open before this is real:** a single `node` compose profile that bundles core+mcp+continuum cleanly; a k8s manifest/Helm chart per node; image addressing that doesn't assume localhost. These are the deliberate next steps, designed here before coded.

---

## 7. The Node boundary (where it legitimately lives)

Node/TS is **only** for building the web client SDK and the web/desktop apps that consume it (`client/`-and-`apps/`-level, leaf consumers, rebuilt anew on the uniform SDK per `[[old-web-client-is-reinvented-not-resurrected]]`). It is **never** in: core build, core start, core test, MCP, `continuum`, the grid, or any Dockerfile for a foundation unit. The legacy `src/` Node tree and its `package.json` orchestration are being retired into clients-built-anew; until then, treat anything there as legacy, not the path.

---

## 8. Status & next slices

**Done / live (this foundation pass):**
- Pure-Rust startup via `continuum start`/`stop`; legacy `parallel-start.sh` quarantined to `legacy/`; `.gitignore` corrected for the moved model path.
- `continuum` CLI = lifecycle + uniform client; `continuum ping` green.
- Self-routing command infra (`DynCommand` + `ActionCommand`), persona executes through it (deterministic test **and** live: Asha reasons/recalls/replies on the clean build).

**Next (design-first, in order):**
1. **Live-smoke harness** (`#22`): scripted `continuum start → ping → persona turn` assertion.
2. **unsloth `/v1/embeddings`** (`#40`): neural recall instead of lexical fallback.
3. **`continuum` → `apps/cli`** migration (unit boundary hygiene, §3).
4. **`node` compose profile + k8s manifest** (§6): the rollout unit, bundled and addressable.
5. **Clients anew** (`#29`): task trays / web / mobile on the uniform SDK, as containers.

## 9. See also

- [COMMAND-ORGANIZATION.md](COMMAND-ORGANIZATION.md) — the self-routing command infra these units expose.
- [COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md) — module/handler floor.
- [CLIENT-SDK-PLATFORM-ARCHITECTURE.md](CLIENT-SDK-PLATFORM-ARCHITECTURE.md) — clients/SDK tiers (the layers built on top).
- Memory: `[[headless-core-many-clients]]`, `[[rust-is-the-core-node-is-the-shell]]`, `[[validate-via-pure-rust-not-npm-jtag]]`, `[[move-first-let-compiler-find-the-smell]]`, `[[docker-personas-as-grid-peers]]`.
