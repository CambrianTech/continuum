# legacy/quarantine — archived orphans (never deleted)

Moved here 2026-08-14 under the #424 Opus-damage cleanup, per Joel's standing
rule: **quarantine in legacy/, never delete.** Each entry was verified to have
ZERO live references (grep across apps/, sdk/, scripts/, core src, package.json,
.github/workflows) before the move. If you need one back, `git mv` it out — full
history is preserved.

| Entry | Was | Why it's an orphan |
|---|---|---|
| `tools-generator/` | `tools/generator/` | Node-era codegen suite (77 files). Zero inbound imports from the live tree; its outputs (`shared/generated/`, command scaffolds) belong to the retired Node shell. Rust↔TS bindings are now ts-rs `export_to = "protocol/typescript/..."` directly from the core (#80/#247). |
| `core-protocol/` | `core/protocol/` | Diverged dead copy of the protocol root. The canonical TS protocol is repo-root `protocol/typescript/`; this copy had drifted (e.g. stale AgentSolveParams/AgentSolveResult) and nothing imported it. |
| `shared-generated/` | `shared/generated/` | Node-era ts-rs output dir. Its only referencer was `tools/generator/` (also quarantined). The last two live emitters (`PagerCaptureEvent`, `ServingSnapshot`) were re-pointed to `protocol/typescript/{capacity,serving}/` in the same change. |
| `archive-worker/` | `core/archive/` | Standalone bin crate whose only spawn wiring (`core/workers-config.json` → `workers/target/release/archive-worker`) points at a directory that does not exist — dead since the Node-era workers/ layout was retired. Removed from workspace members so it stops burning compile time; the sqlite archival concern, if revived, belongs behind a ServiceModule in the core. |

The live-reference check that gated each move: `grep -rln <name> apps/ sdk/
scripts/ tools/ core/continuum-core/src/ package.json .github/workflows/`
(node_modules and .claude/worktrees excluded).
