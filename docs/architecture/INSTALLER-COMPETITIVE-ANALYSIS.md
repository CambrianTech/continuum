# Installer Competitive Analysis — Does the Competition "Update" or "Install Again"?

*Carl installer card 54e0d729. Primary-source receipts gathered 2026-07-24 on Joel's M-series MacBook:
the captured Hermes/Unsloth install journey (`~/Development/setup-screenshots/hermes-unsloth-locally/`,
24 screenshots + a project JSONL, June 18–19), a live `opencode` 1.17.15 install on PATH, and the
well-documented behavior of the standard dev/consumer tool classes.*

## 1. The answer: a clean class split

**Nobody serious re-runs an installer as the primary update path.** The field splits into two classes:

| Class | Install | Update | Installer re-run means… |
|---|---|---|---|
| **Dev-CLI tools** (rustup, uv, opencode, Homebrew, ollama-on-Linux) | idempotent `curl \| sh` (or brew/npm) | **a product self-update verb**: `rustup update`, `uv self update`, `opencode upgrade`, `brew upgrade` | *safe repair* — the installer is written to be re-runnable, and for ollama on Linux re-running the curl script IS the documented update path |
| **Consumer-GUI apps** (VS Code, Chrome, Docker Desktop, Steam, Claude Code, ollama Mac app, Hermes desktop) | download app / one-shot wizard | **background auto-update** + restart prompt; user never sees an installer again | *something is broken* — re-running the installer reads as a failure state to this audience |

The convergent shape across both classes: **one idempotent convergence path, two invocations.** The
dev class exposes it as an explicit verb and keeps the installer re-runnable as repair; the consumer
class hides it behind background auto-update. The good products (rustup, unsloth, opencode) make the
installer and the update verb literally the same logic, so "install again" is never destructive — it
converges.

Receipts from this machine:

- **Unsloth Studio installer embeds its updater**: the June 18 terminal capture of
  `curl -fsSL https://unsloth.ai/install.sh | sh` shows the install path itself running
  `setup: running unsloth studio update...` — installer and updater are one engine.
- **Unsloth Studio does in-product component updates**: June 19 screenshot shows a background toast
  *"Updating llama.cpp… b9692 → b9704-mix-2d6bd50 — 11 MB download · No restart needed after update."*
  Per-component, delta-sized, no restart. This is the best update moment in the whole competitor
  corpus — and it's exactly the shape Carl generalizes.
- **opencode ships the verb pair**: `opencode upgrade [target]` (with `--method curl|npm|pnpm|bun|brew|choco|scoop`
  — it detects/honors how it was installed) and `opencode uninstall`. Joel's copy is an npm-style
  install (`~/.local/bin/opencode → ../lib/node_modules/opencode-ai/bin/opencode.exe`).

## 2. Per-competitor table

| Competitor | Install verb | Update verb | Repair story | GPU / model provisioning |
|---|---|---|---|---|
| **Hermes desktop (Nous)** | 11-step GUI wizard (incl. "Install Python dependencies", "Build desktop app" *on the user's machine*) | consumer-GUI class (background app update); no user-facing verb | none observed; wizard re-run is the only lever | **None.** Provider-picker wall: Nous Portal subscription (recommended), OAuth or BYO API keys. Local = "point at your own OpenAI-compatible endpoint" |
| **Unsloth Studio** (the local-stack half of the Hermes journey) | `curl -fsSL unsloth.ai/install.sh \| sh` — idempotent, runs `studio update` inside install | same script re-run + in-product per-component updates (llama.cpp toast, "no restart needed") | installer re-run is the repair | Partial: auto-downloads a 4B fallback model on first message; prebuilt llama.cpp "installed and validated". But GPU tiering is NVIDIA/ROCm-centric — see friction #3 |
| **opencode 1.17.15** | `curl \| bash` or npm/brew/… | `opencode upgrade [target]` — self-update, install-method-aware | `upgrade` doubles as repair; explicit `uninstall` | **None by design.** BYO provider/API key (`opencode providers`). No local GPU stack, no model management |
| **rustup** | `curl \| sh`, idempotent | `rustup update` (updates toolchains AND itself via `rustup self update`) | re-run installer = safe converge | n/a |
| **uv** | `curl \| sh` | `uv self update` | re-run installer = safe converge | n/a |
| **ollama** | Linux: `curl \| sh`; Mac: app | Linux: **re-run the same curl script — that IS the documented update path**; Mac app auto-updates | same script, idempotent | Ships llama.cpp runtime + model pulls; the closest to us, but no per-component verification gate, no training stack |
| **VS Code / Chrome / Docker Desktop / Steam / Claude Code** | app install | background auto-update + restart prompt | reinstall app (rare, reads as broken) | n/a |

## 3. The Hermes install journey — receipts and friction ledger

The screenshots capture Joel doing what a real user does: install Hermes, discover it provisions no
model, install a second product to get one, and try to use it on the repo he cares about.
**11 discrete friction moments:**

1. **11-step install wizard** ("3 of 11 steps"), including *creating a Python venv*, *installing
   Python dependencies*, and *"Build desktop app"* — compiling the product on the user's machine at
   install time. Any of those steps is a place to fail with no repair verb.
2. **The provider wall.** First-run screen is a picker of ~10 cloud providers with the vendor
   subscription flagged RECOMMENDED. The agent you just installed cannot think yet; the model is
   *your* problem. Local = a raw URL field ("point at vLLM/llama.cpp/Ollama").
3. **A second installer for the local stack.** To run anything locally Joel ran a *different
   product's* installer (`curl unsloth.ai/install.sh | sh`). Hermes and its local stack never became
   one install. **GPU misdetection on the way in:** Apple Silicon with 64 GB unified memory reads as
   `gpu: none (CPU-only)` then `none (chat-only / GGUF)` — "Training and GPU inference require an
   NVIDIA or AMD ROCm GPU" — while the very next boot log says `Hardware detected: MLX — Apple
   Silicon (arm64)`. Two components, two contradictory GPU verdicts, zero verification gate.
4. **Bootstrap-password ceremony.** Terminal prints "DEFAULT ADMIN ACCOUNT CREATED… password saved
   to ~/.unsloth/studio/auth/.bootstrap_password. Open the Studio UI to sign in and change it," then
   a forced `/change-password` page — for a localhost-bound single-user tool.
5. **Terminal wall as the finish line.** "Install done" is a screenful of uvicorn logs, Ctrl+C
   instructions ("on macOS, Control+C not Command+C"), and a manual relaunch recipe
   (`unsloth studio -H 0.0.0.0`) for network access.
6. **No model at first chat.** The chat accepts a message with no model loaded; only then does a
   toast appear — "No downloaded models found. Fetching Qwen3.5-4B-MTP" — while the UI shows a
   misleading "Generating…". Provisioning happens implicitly, at the worst moment, per-surface.
7. **The inventory disconnect.** The Fine-tuning Studio, minutes after chat downloaded that model,
   says "No local models found. Enter path manually." and red-errors "Select a base model first."
   Two surfaces of one product with two model inventories.
8. **Sandbox lies → confabulation.** Asked to look at `~/Development/continuum`, the agent's
   terminal tool runs inside `~/Documents/Unsloth Studio/Projects/continuum-5858a69e/sandbox` (JSONL
   receipt). The local `ls` fails, the agent silently falls back to *web-searching* "continuum
   project github repository," and produces a confident architecture diagram of the wrong thing —
   complete with an "UNSLOUT (Training Backend)" box. No error surfaced to the user.
9. **Re-prompt/tool-fluency wobble** in the llama-server logs: `Re-prompt 1/1: model responded
   without calling tools` — the local 4B fallback isn't tool-reliable, and the product paper-tapes
   it in a log the user never sees.
10. **Model catalog reality gap.** The Hub proudly offers 217 GB GGUFs (GLM-5.2) on a 64 GB machine;
    nothing prices downloads against the actual device.
11. **HF-token leakage into basic flows.** Fine-tune setup surfaces a Hugging Face token field (with
    the browser password-manager popping over it) before the user has selected anything gated.

**Worst three:** #3 (two installers, contradictory GPU verdicts, no verification), #8 (sandbox
failure silently converted into web-search confabulation), and #7 (per-surface model inventories).

The one genuinely good moment — the llama.cpp delta-update toast (#1 receipt above) — is the pattern
Carl adopts and generalizes.

## 4. What none of them do — Carl's mandate

Every competitor either (a) provisions no GPU stack at all (Hermes, opencode — BYO provider), or
(b) provisions one without a verification contract (Unsloth, ollama — download and hope). **None of
them do per-component GPU provisioning + verification.** That is Carl's differentiator and its gate:

- **The GPU-CONTRACT gate.** Every provisioned component (runtime, driver shim, model tier, lane
  config) is verified against the *actual device* before install declares success — real numbers, not
  a clamp premise. The Hermes journey's contradictory `CPU-only` / `MLX detected` verdicts are
  exactly what a contract gate makes impossible: one probe, one verdict, every component checked
  against it, install fails loud if the contract can't be met. (Rule references: fallbacks are
  illegal — fail loud; verify real device numbers.)
- **One convergence engine, two invocations** (Joel's approved synthesis):
  1. **Product-triggered background self-update** — the wow surface. The running product converges
     itself the way Unsloth updated llama.cpp: per-component, delta-sized, no restart where possible,
     visible as a receipt not a ceremony. Consumer-GUI class behavior, because Carl's default-to-WOW
     surface is a consumer product.
  2. **Installer re-run as universal repair + headless path** — the same engine invoked from
     `curl … | bash`. Idempotent like rustup/unsloth: fresh machine → install; broken machine →
     repair; current machine → no-op with receipts. This is also the CI gate
     (`carl-install-smoke`) and the headless/server path. Dev-CLI class behavior, because operators
     and CI live here.

  Same engine, same component graph, same GPU-CONTRACT verification — only the trigger differs.
  This resolves the class split instead of picking a side: users get Chrome-class invisibility,
  operators get rustup-class explicitness, and "install again" is never a distinct (drifting)
  code path. One logical installer, one place — the two-`install.sh` drift called out in
  `docs/CARL-INSTALL-SMOKE-HANDOFF.md` is the in-house violation of exactly this rule.

## 5. The Hermes anti-pattern list (what Carl must beat)

Their install pain is our wow opportunity. Carl must structurally preclude each:

| # | Hermes anti-pattern | Carl requirement |
|---|---|---|
| 1 | 11-step wizard, compile-on-install | One command / one click; prebuilt artifacts; steps are receipts, not gates the user babysits |
| 2 | Provider wall before first thought | First boot lands in live mode with a working local persona — install IS the product |
| 3 | Second installer for the local stack; contradictory GPU verdicts | ONE install provisions the full local GPU stack; GPU-CONTRACT gate: one device probe, per-component verification, fail loud |
| 4 | Bootstrap-password file + forced change ceremony | Self-provisioned identity; no operator steps (managed-product doctrine) |
| 5 | Terminal-log wall as the finish line | Finish = the world, running; feedback streams during install, never a log dump |
| 6 | No model until first message; "Generating…" while secretly downloading | Models provisioned and verified at install per device tier; never lie about state |
| 7 | Per-surface model inventories ("No local models found" after downloading one) | One model catalog, one source of truth, every surface reads it |
| 8 | Sandbox failure silently becomes web-search confabulation | Execution failures surface as failures; never blind, receipts required |
| 9 | Tool-call failures paper-taped in hidden logs | Tool fluency is verified/trained, defects mined from captures, not swallowed |
| 10 | 217 GB downloads offered to a 64 GB machine | Catalog priced against the live device; fit-first |
| 11 | Credential fields ambushing basic flows | Keys requested only when the action needs them, at the action |

## Sources

- `~/Development/setup-screenshots/hermes-unsloth-locally/` — 24 screenshots, June 18–19 2026
  (Hermes wizard, provider wall, Unsloth curl install, first-chat model fetch, Fine-tuning Studio,
  Hub, llama.cpp update toast, export menu) + `project-continuum-2026-06-19T18-16-10.jsonl`
  (sandboxed terminal tool calls → web-search fallback).
- `~/.local/bin/opencode` (1.17.15): `opencode --help`, `opencode upgrade --help` — live output.
- Class baselines (documented, stable behavior): rustup (`rustup update`/`self update`), Homebrew
  (`brew upgrade`), uv (`uv self update`), ollama (Linux curl-rerun = update; Mac app auto-update),
  VS Code / Chrome / Docker Desktop / Steam / Claude Code (background auto-update + restart prompt).
- In-house: `docs/CARL-INSTALL-SMOKE-HANDOFF.md` (the two-install.sh drift; carl-install-smoke gate).
