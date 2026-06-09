# Continuum × AIRC collaboration pilot (#1109)

This directory is the **repo-local front door** for human and agent
contributors. It tells you how the project coordinates across
multiple peers using [AIRC](https://github.com/CambrianTech/airc).

If you cloned this repo and want to help: start here.

## Files

| File | What it answers |
|---|---|
| [POLICY.md](POLICY.md) | What the rules are. Required reading. |
| [QUEUE.md](QUEUE.md) | What's in flight. PR-card format spec. |
| [ASSEMBLY-LINE.md](ASSEMBLY-LINE.md) | Heartbeat, stall threshold, pickup protocol — how the line stays moving when peers drop offline. |
| [ONBOARDING.md](ONBOARDING.md) | How to knock, get approved, join the private collaboration room. |
| [SAFETY.md](SAFETY.md) | Outside-agent etiquette + things that get you removed. |
| [manifest.json](manifest.json) | Machine-readable summary of this pilot — entry points, dependencies, version. |

## Why this exists

The Continuum project is collaboratively maintained by Joel +
multiple AI agents (Claude tabs, Codex sessions) + external
contributors. The AIRC pilot makes that collaboration **legible from
outside**: a fresh clone can read these files and learn how to
participate without DMing Joel for permission first.

Without this layer:

- New contributors have no way to discover the collaboration room.
- Active peers can't see each other's in-flight work (dupe PRs).
- Agents going offline silently stall the line for unknown durations.
- "Who decided what" disappears into AIRC scrollback.

This pilot is a paired effort with [airc#559](https://github.com/CambrianTech/airc/issues/559)
(public knock + approved handoff + shared queue primitives in the
AIRC binary). Continuum is the guinea pig; once it works here, the
shape generalizes to other repos.

## Status

- **Docs**: this PR (continuum#1109 → #1110).
- **Knock entrypoint**: `airc knock <owner/repo> <message>` — shipped in [airc#560](https://github.com/CambrianTech/airc/pull/560), merged to airc canary 2026-05-13.
- **Approve flow**: `airc approve <knock-issue-url>` with forward-secret encrypted invite — shipped in [airc#561](https://github.com/CambrianTech/airc/pull/561), merged 2026-05-13.
- **Queue tooling**: PR-card format spec in [QUEUE.md](QUEUE.md); runtime primitives (claim/release/done/nudge) in flight at [airc#562](https://github.com/CambrianTech/airc/issues/562).
- **Pilot scope**: install/Docker image gates (#1085, #1071), Rust persona work, LiveKit bridge, alpha gap cleanup (current release sprint).

Knock the repo: `airc knock CambrianTech/continuum "I want to help with X"`.
