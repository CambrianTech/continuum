#!/bin/bash
# scripts/precommit-config.sh — modular precommit configuration.
#
# Sourced by scripts/git-precommit.sh at start. Sets the gate flags + the
# test list. The hook falls back to safe defaults if this file is missing,
# but having the file means defaults are now CHECKED IN AND DOCUMENTED
# rather than implicit (continuum#1190 — config never-loaded smell).
#
# Edit this file (don't edit defaults inline in git-precommit.sh) when
# changing precommit behavior. Bump CONFIG_VERSION when introducing a
# breaking change so reviewers see the diff.
#
# To temporarily disable a gate locally without committing the change,
# export the variable BEFORE the commit, e.g.:
#   ENABLE_TYPESCRIPT_CHECK=false git commit -m "..."
# (every flag uses `${VAR:-default}` so a pre-set env var wins.)

# Config schema version. Bump when adding/renaming variables so review
# can flag breaking changes.
export PRECOMMIT_CONFIG_VERSION="1.0.0"

# ---- Gate flags --------------------------------------------------------------

# Phase 1: TypeScript compilation (npm run build:ts)
export ENABLE_TYPESCRIPT_CHECK="${ENABLE_TYPESCRIPT_CHECK:-true}"

# Phase 2: System restart strategy ("on_code_change" | "always" | "never").
# "on_code_change" = restart only if code-relevant files staged.
export RESTART_STRATEGY="${RESTART_STRATEGY:-on_code_change}"

# Phase 2: Browser test (PRECOMMIT_TESTS via vitest in tests/precommit/).
# Tests run sequentially. Most tests are capped at 60s; chat-roundtrip gets a
# larger cap because local persona inference can be backpressured while still
# producing a valid reply inside the smoke-test budget.
#
#   browser-ping       — server didn't crash, browser is reachable (low bar)
#   chat-roundtrip     — a persona actually replies to a chat probe (#1186 PR-1)
#                        catches: cognition pipeline silently broken, persona
#                        seed regressed, chat_messages write path broken,
#                        empty-reply cognition-failure mode
#
# Adapter unit tests + path-tier dispatcher (only run heavy tests when
# relevant paths touched) are #1186 PR-2 / PR-3 follow-ups.
export ENABLE_BROWSER_TEST="${ENABLE_BROWSER_TEST:-true}"
export PRECOMMIT_TESTS="${PRECOMMIT_TESTS:-tests/precommit/browser-ping.test.ts tests/precommit/chat-roundtrip.test.ts}"
export PRECOMMIT_TEST_TIMEOUT_SECONDS="${PRECOMMIT_TEST_TIMEOUT_SECONDS:-60}"
export PRECOMMIT_CHAT_ROUNDTRIP_TIMEOUT_SECONDS="${PRECOMMIT_CHAT_ROUNDTRIP_TIMEOUT_SECONDS:-120}"

# Phase 3: Artifact collection (test reports, screenshots). Disabled until
# Phase 2 actually produces artifacts worth collecting.
export ENABLE_ARTIFACTS="${ENABLE_ARTIFACTS:-false}"

# ---- Notes for future config edits ------------------------------------------
#
# - Branch-state guard (continuum#1187) is hard-coded ON in the hook;
#   not a flag because turning it off defeats the purpose.
# - Phase 0 command-generator-ownership guard is also hard-coded; same logic.
# - Phase 1.5 strict-lint baseline ratchet is hard-coded; the baseline file
#   src/clippy-baseline.txt + src/eslint-baseline.txt are the knobs.
