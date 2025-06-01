Thanks. Here's the clear delta:

* **Current Continuum packages**: `adapters/`, `cli/`, `core/`
* **Design packages**: `agent-orchestrator/`, `llama-bootstrap/`

Now let’s merge this together into a comprehensive `ROADMAP.md`, listing everything that must be done to migrate and integrate your design fully into the existing Continuum repo.

---

## 🛣️ `ROADMAP.md` — Migrating Agent System to Continuum

> *This file lists all tasks required to integrate the full Claude/LLaMA runtime, orchestration layer, local bootstrap, and memory system into the existing Continuum project structure.*

---

## ✅ Phase 0: Prep and Comparison

### What we have in Continuum:

* ✅ `packages/` exists but only includes: `adapters/`, `cli/`, `core/`
* ✅ Main repo includes standard monorepo setup with `lerna`, `tsconfig.json`, and CI/CD

### What needs to be merged from Design:

* 📦 `packages/agent-orchestrator/`
* 📦 `packages/llama-bootstrap/`
* 🔁 `.claude/` structure + boot scripts
* 🧠 Claude integration logic
* 🧪 Claude delegation and LLaMA task offloading

---

## 🧱 Phase 1: Core Code Integration

### ⬇️ Step-by-step:

* [ ] Copy `packages/agent-orchestrator/` into Continuum’s `packages/`
* [ ] Copy `packages/llama-bootstrap/` into `packages/`
* [ ] Add `llama-runner.ts` in `llama-bootstrap/`
* [ ] Update `launch-tmux.sh` to pipe output for Claude access
* [ ] Create `index.ts` stub for each package if missing

---

## 🧠 Phase 2: Claude Runtime Loop

* [ ] Add `.claude/` memory scaffold:

  * `chat.md`, `musings.md`, `memory.json`, `comments.md`, `tasks/llama-todo.md`, etc.
* [ ] Add `launch-claude.sh` to root or `packages/claude-agent/`
* [ ] Setup `claude-agent/index.ts` (API to read/write memory + delegate)
* [ ] Write system prompt boot logic (`system.md` → initial Claude persona)

---

## 🔁 Phase 3: Agent Delegation & Routing

* [ ] Connect `agent-orchestrator/router.ts` to:

  * Claude (via Claude API or Claude Code)
  * Local LLaMA (via `llama-runner`)
* [ ] Make routing decisions based on:

  * `task.type`
  * `.env` values (cost policy, override preferences)
* [ ] Log all routing calls to `.claude/api-usage.log`

---

## 🌐 Phase 4: Claude + LLaMA Queue

* [ ] Claude writes subtasks to `tasks/llama-todo.md`
* [ ] Add watcher or polling loop in `agent-orchestrator` that:

  * Reads `llama-todo.md`
  * Sends task to LLaMA
  * Appends result to `delegation.log` or returns to Claude

---

## 🖥️ Phase 5: Claude’s Web UI ("Claude's Corner")

* [ ] Create `packages/claude-agent/ui/`

  * Vite+React
  * Show `chat.md`, `musings.md`, `comments.md`
  * Editable `.env` (loaded live)
  * Task logs and agent usage breakdown
* [ ] Wire into `launch-claude.sh`

---

## 💸 Phase 6: Budget & Sync System

* [ ] Add `.env` with fields:

  * `MAX_CLAUDE_CALLS_PER_HOUR`
  * `USE_CLAUDE_FOR=...`
  * `USE_LLAMA_FOR=...`
* [ ] Claude reads and obeys budget restrictions
* [ ] Cost tracking into `.claude/api-usage.log`
* [ ] Add optional sync-to-remote toggle (Phase 7+)

---

## 📚 Final Integration Tasks

* [ ] Update root `README.md` to link to `packages/claude-agent/README.md`
* [ ] Merge and clean `design/README.md` → `packages/claude-agent/README.md`
* [ ] Add `ROADMAP.md` and keep progress updated
* [ ] Test local bootstrap via:

  ```bash
  ./packages/llama-bootstrap/install.sh
  ./packages/llama-bootstrap/launch-tmux.sh
  ```

---

Let me know if you want this version saved directly into your repo structure as `ROADMAP.md`, or if you'd like a single `.patch` file to apply everything at once.
