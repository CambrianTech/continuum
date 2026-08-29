#!/bin/bash
# Boundary sequence for the Flash-Next hard-eight (docs/planning/FLASHNEXT-HARD-EIGHT-RUN.md).
# Run when `continuum benchmark/rounds` reports in_flight: 0. One command, receipts at every step.
set -euo pipefail

export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"

echo "== 1/5 reboot (deploys the Flash-Next catalog row + serving contract)"
continuum reboot

echo "== 2/5 pin Flash-Next"
continuum serving/pin --model "AtomicChat/Qwen3.8-Flash-Next-GGUF"

echo "== 3/5 readiness + the reasoning-separation probe"
# The lane smoke probe IS the required first-request warmup (a chat-sized first
# request Metal-OOMs and latches the backend — measured 2026-08-28).
continuum ping
# Reasoning check: triage saw chain-of-thought leaking into message.content
# (auto-detection failed silently). If the probe below shows reasoning-styled
# content, relaunch the lane with: -rea on --reasoning-format deepseek
echo 'PROBE: verify a chat completion returns clean content (no "We need to..." reasoning-speak).'
# THINKING-BUDGET CAP (external receipt, 8/28: Flash-Next spent 40,265 thinking
# tokens before a 21k answer on one hard prompt). Our serving window is 32k
# TOTAL — uncapped, it blows the window on thinking and the harness would
# misread "wasn't allowed to finish" as "can't solve". Lane must carry:
#   --reasoning-budget 8192
# A miss that ends AT the reasoning cap gets noted in the verdict — that is a
# geometry loss, not an intelligence loss, and the pager arc (bigger window at
# 37GB resident) is its retest condition.

echo "== 4/5 dispatch the hard eight (Ornith's misses — see the run doc for the full 20)"
continuum benchmark/dispatch --name swe-bench-lite \
  --instances django__django-10914,django__django-13809,pytest-dev__pytest-5103,pallets__flask-4045,sympy__sympy-21379,sympy__sympy-22005
continuum benchmark/dispatch --name swe-bench-verified \
  --instances pylint-dev__pylint-7114,sphinx-doc__sphinx-10325

echo "== 5/5 watch"
echo "continuum benchmark/rounds   # progress"
echo "Verdicts stamp harness_build + the serving regime names the model — the comparison is honest by construction."
