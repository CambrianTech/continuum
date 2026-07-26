---
name: remember
description: Store a durable lesson for future sessions. Use when you learn something worth not re-forgetting — a correction, a project fact, a version, a gotcha. Saved to the continuum memory substrate; auto-surfaces in relevant future sessions (including after context compaction).
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/scripts/remember.sh *)
---

# Remember

Store this lesson so it survives amnesia resets: **$ARGUMENTS**

Run:
```
${CLAUDE_PLUGIN_ROOT}/scripts/remember.sh "$ARGUMENTS"
```

The script resolves the agent's persona (its airc peer id), scopes to the current
project, and writes the lesson to the shared continuum corpus via
`cu memory/append-memory`. Confirm to the user in one line what was remembered.

Good lessons name the invariant/version/correction concretely ("use CUDA 13.2 not
12.9 for VS2026", not "fix the build"). Other agents (M5, Codex) recall from the same
store; the memory is tagged with which agent learned it.
