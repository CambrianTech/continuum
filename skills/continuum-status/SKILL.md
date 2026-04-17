---
name: continuum:status
description: Show the current state of a Continuum installation — containers, personas, DMR backend, grid nodes, widget URL.
user-invocable: true
allowed-tools: Bash
argument-hint: ""
---

# Continuum Status

Run the CLI yourself and translate the output into something useful.

## Run

```bash
continuum status
```

The CLI prints container status (which are up/healthy/unhealthy), tailscale grid nodes if configured, and the widget URL.

## Interpret + report

Don't just dump the output. Tell the user what matters:

- **All containers healthy, widget URL reachable** → "Continuum is running at X. Open it to chat with personas, or use `/continuum:chat @<persona> <msg>` from here."
- **Some containers unhealthy** → name which ones and suggest `continuum logs <svc>` + possibly `continuum doctor`.
- **Nothing running** → "Not started. Run `continuum start` (or click the continuum tray icon if installed)."
- **Grid nodes visible** → mention them briefly, don't flood the output.

## When to suggest follow-ups

- Unhealthy node-server → `continuum logs node-server` then `/continuum:doctor`
- DMR backend shown as `latest-cpu` instead of `latest-metal` / `latest-cuda` → point the user at `docs/SETUP.md` for the Docker Desktop AI toggle
- Widget URL unreachable even though containers are up → port conflict; `lsof -i :9003`

## Related

- `/continuum:update` — pull latest
- `/continuum:doctor` — diagnose
- `/continuum:chat` — send a message to a persona from here

## Notes

This skill is for devs still in Claude Code who want a quick read on their local continuum without leaving the IDE. Carl (end-user audience) never needs this — they see status via the widget's own UI.
