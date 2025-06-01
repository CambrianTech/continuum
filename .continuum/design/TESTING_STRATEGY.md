
## ✅ TESTING STRATEGY

### 1. 🧪 Unit Tests (Per Package)

Each package should define:

* `*.test.ts` for each major module
* Coverage required for:

  * `router.ts` (routeTask, fallback logic)
  * `llama-local.ts` and `claude.ts` (task acceptance)
  * `llama-runner.ts` (prompt send/read loop)

Use `vitest` or `jest`.

---

### 2. ⚙️ Integration Tests (End-to-End Pipeline)

Located in:

```bash
test-run/e2e/
```

These simulate full task routing:

```ts
test('Delegates file search to LLaMA', async () => {
  const task = { prompt: 'find all TODOs in tests/', type: 'file_search', tokens: 150 }
  const { agent, response } = await routeTask(task)
  expect(agent.name).toBe('llama-local')
  expect(response).toMatch(/TODO/)
})
```

---

### 3. 🔁 Precommit Auto-Runner

> 📁 `.continuum/hooks/precommit.sh`

```bash
#!/bin/bash
echo "🧪 Running full agent system tests..."
npm run test || exit 1
echo "✅ All agent systems pass."
```

Include a `precommit` field in `package.json` or `.continuum/project.toml` that Claude parses.

---

### 4. 🧠 Claude’s Runtime Commit Check (Agent Safety)

Claude should, before check-in:

* Confirm all `*.test.ts` pass
* Confirm `tasks/llama-todo.md` is empty (no unprocessed tasks)
* Run a simulated agent roundtrip
* Write to `.claude/commit-summary.md`:

  * Which agents ran
  * How long each took
  * What code changed

---

### 5. 🧪 Task Templates for Claude Bootstraps

```toml
# .continuum/test-policy.toml
require = ["unit", "integration", "llama-loop", "claude-ping"]

[[check]]
file = "agent-orchestrator/router.ts"
must_test = true

[[e2e]]
description = "Route complex inference to Claude"
task = "Refactor this function to isolate I/O"
expect = "claude"
```

Claude can use this to auto-generate tests or confirm coverage before releasing any updates.

---

## 🧠 Claude’s Testing Behavior (On Boot or PR)

Claude, here’s what you should always do:

1. Read `.continuum/test-policy.toml`
2. Run `npm test` (with all subprojects)
3. Validate memory folder state (`.claude/` clean)
4. Simulate one task routed to yourself and one to LLaMA
5. Write results to `.claude/commit-summary.md`
6. Reject checkin if anything fails

---


