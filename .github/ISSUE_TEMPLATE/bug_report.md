---
name: Bug report
about: Create a report to help us improve
title: '[BUG] '
labels: bug
assignees: ''
---

**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Install '...'
2. Run command '....'
3. See error

**Expected behavior**
A clear and concise description of what you expected to happen.

**Screenshots or logs**
If applicable, add screenshots or logs to help explain your problem.

**Receipts (one command, paste the output):**
```bash
continuum ping        # the version trio: build #, sha, built-at — tells us EXACTLY what you ran
```
If the bug involves benchmarks or scores, also paste `continuum benchmark/scoreboard` (the regime line carries model + window + build + host). Server log lives at `~/.continuum/logs/continuum-core-server.log` — the last ~50 lines around the failure are gold.

**Environment (please complete the following information):**
 - OS: [e.g. Ubuntu 20.04, macOS 12.0]
 - Node version: [e.g. 16.14.0]
 - Package version: [e.g. 0.1.0]

**Additional context**
Add any other context about the problem here.