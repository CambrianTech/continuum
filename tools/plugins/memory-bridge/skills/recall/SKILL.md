---
name: recall
description: Search past lessons by query. Session-start recall already auto-surfaces the most relevant lessons; use this to look up something specific on demand.
disable-model-invocation: true
allowed-tools: Bash(${CLAUDE_PLUGIN_ROOT}/scripts/recall.sh *)
---

# Recall

Search stored lessons for: **$ARGUMENTS**

Run:
```
${CLAUDE_PLUGIN_ROOT}/scripts/recall.sh "$ARGUMENTS"
```

The script runs the substrate's 6-layer relevance recall (recency / semantic /
importance / cross-context …) via `cu memory/multi-layer-recall`, scoped to the
current project, and prints the matching lessons most-relevant-first.

`disable-model-invocation: true` — explicit lookup only. Automatic session-start
recall is handled by the plugin's SessionStart hook, so use this to hunt a specific
past lesson.
