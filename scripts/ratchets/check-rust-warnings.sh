#!/bin/bash
# check-rust-warnings.sh — continuum-core rustc warning-count ratchet.
#
# CLAUDE.md's law is "every warning fixed", and on 2026-09-16 canary tip carried
# ~100 of them in continuum-core (unused imports and variables, values assigned and
# never read, dead scaffolding) because nothing GATED it — a requirement that is not a
# gate is lost. This makes the debt monotonic the way the ESLint ratchet does: fail on
# growth, and fail on shrink unless the baseline is lowered in the same branch, so a
# cleanup win cannot evaporate between PRs.
#
# Input: the `--message-format=json` stream cargo already writes in CI
# (lib-artifacts.jsonl; the render-diagnostics form sends messages to stderr as text
# and is useless here). Counted: distinct rustc LINT warnings in continuum-core —
# `reason == compiler-message`, level `warning`, with a lint `code` (the ts-rs
# "failed to parse serde attribute" proc-macro notes carry no code and are not ours to
# ratchet here), keyed by (code, file, line) so a rebuild never double-counts.
#
# The baseline is the count on the CI platform (ubuntu, default features); a local
# macOS/metal count differs and is not the gate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BASELINE_FILE="$SCRIPT_DIR/rust-warnings-baseline.txt"
UPDATE_BASELINE=0
JSONL=""
for arg in "$@"; do
  case "$arg" in
    --update-baseline) UPDATE_BASELINE=1 ;;
    --help|-h)
      echo "Usage: $0 [--update-baseline] <cargo --message-format=json output file>"
      echo "  Default: require the continuum-core lint-warning count to EQUAL the baseline."
      echo "  --update-baseline: rewrite the baseline to the current count."
      exit 0
      ;;
    *) JSONL="$arg" ;;
  esac
done
if [[ -z "$JSONL" || ! -f "$JSONL" ]]; then
  echo "::error::check-rust-warnings: pass the cargo --message-format=json-render-diagnostics output file"
  exit 2
fi

COUNT="$(python3 - "$JSONL" <<'PY'
import json, sys
seen = set()
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    line = line.strip()
    if not line.startswith("{"):
        continue
    try:
        rec = json.loads(line)
    except json.JSONDecodeError:
        continue
    if rec.get("reason") != "compiler-message":
        continue
    if (rec.get("target") or {}).get("name") not in ("continuum-core", "continuum_core"):
        continue
    msg = rec.get("message") or {}
    if msg.get("level") != "warning":
        continue
    code = (msg.get("code") or {}).get("code")
    if not code:
        continue
    span = next((s for s in msg.get("spans") or [] if s.get("is_primary")), None) or {}
    seen.add((code, span.get("file_name"), span.get("line_start")))
for code, file, line in sorted(seen, key=lambda k: (str(k[1]), k[2] or 0)):
    print(f"  {code:<28} {file}:{line}", file=sys.stderr)
print(len(seen))
PY
)"

if [[ "$UPDATE_BASELINE" == "1" ]]; then
  echo "$COUNT" > "$BASELINE_FILE"
  echo "rust-warnings baseline written: $COUNT"
  exit 0
fi
if [[ ! -f "$BASELINE_FILE" ]]; then
  echo "::error::no baseline at ${BASELINE_FILE#$REPO_ROOT/}; current continuum-core lint-warning count is $COUNT — commit it with --update-baseline"
  exit 1
fi
BASELINE="$(tr -d '[:space:]' < "$BASELINE_FILE")"
echo "continuum-core rustc lint warnings: $COUNT (baseline $BASELINE)"
if (( COUNT > BASELINE )); then
  echo "::error::warnings grew from $BASELINE to $COUNT — fix them (every warning is fixed, CLAUDE.md); never raise the baseline"
  exit 1
fi
if (( COUNT < BASELINE )); then
  echo "::error::warnings fell to $COUNT below the baseline $BASELINE — lower the baseline in this branch so the win sticks: scripts/ratchets/check-rust-warnings.sh --update-baseline <jsonl>"
  exit 1
fi
echo "ok — warning debt is monotonic"
