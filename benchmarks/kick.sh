#!/usr/bin/env bash
#
# kick.sh — Tom Haywood's Kicking Machine. One handle: kick every opponent's ass,
# repeatably, on any machine.
#
# It serves each opponent model from its authoritative Hugging Face source
# (auto-downloaded, no operator-specific paths), runs the SAME gym + SAME rustc
# grader against three arms — RAW (model one-shot), OURS (the same model through the
# full Continuum cognition loop), and the competitor's own agentic harness
# (opencode) — across a ladder of progressively harder gyms, appends every cell to
# the durable ledger (benchmarks/RESULTS.jsonl), and re-renders the evidence board.
#
# Repeatable BY DESIGN: fix a cognition bug, run `./benchmarks/kick.sh` again, and
# the delta is right there in the board. Reproducible BY DESIGN: a stranger who
# cloned the repo runs the SAME command and gets the SAME numbers — that is the
# point (leave no doubt about the claims).
#
#   ./benchmarks/kick.sh                 # full ladder, 40 tasks/gym, whole fleet
#   ./benchmarks/kick.sh --limit 10      # quick shakeout
#   ./benchmarks/kick.sh --gyms humaneval-rs           # one rung
#   ./benchmarks/kick.sh --models benchmarks/coder/models.json   # a different fleet
#
# Requirements (checked below; a stranger sees exactly what's missing):
#   rustc            — the grader compiles+runs each answer (pass = exit 0)
#   python3 + huggingface_hub — opponent download + harness glue
#   a Continuum core — serves the OURS lane (`cu start`); RAW/opponent arms don't need it
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CODER="$HERE/coder"
CU="${CU:-$HOME/.continuum/cache/cargo-target/debug/cu}"

# Easy → unsaturated. Frontier is where OURS pulls decisively ahead of RAW and the
# competitor: the harder the task, the more the cognition loop (act→observe→act,
# tools, iteration) matters. humaneval saturates for capable models; keep it as the
# shakeout rung.
GYMS=("humaneval-rs" "hard-rs" "frontier-rs")
LIMIT=40
MODELS="$CODER/models-fleet.json"
ALLOW_CONTENDED=0   # by default, refuse to measure on a GPU that's already busy

while [ $# -gt 0 ]; do
  case "$1" in
    --limit)  LIMIT="$2"; shift 2 ;;
    --gyms)   IFS=' ' read -r -a GYMS <<< "$2"; shift 2 ;;
    --models) MODELS="$2"; shift 2 ;;
    --allow-contended) ALLOW_CONTENDED=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }
die() { printf '\033[31mkick: %s\033[0m\n' "$*" >&2; exit 1; }

say "preflight"
command -v rustc  >/dev/null || die "rustc not found — install Rust (https://rustup.rs); the grader needs it."
command -v python3 >/dev/null || die "python3 not found."
python3 -c "import huggingface_hub" 2>/dev/null || die "huggingface_hub missing — pip install huggingface_hub"
[ -x "$HOME/.continuum/bin/llama-server" ] || die "llama-server not at ~/.continuum/bin/llama-server — build/install it (npm start wires it)."
if "$CU" ping >/dev/null 2>&1; then
  echo "  core: up (OURS lane available)"
  OURS_OK=1
else
  echo "  core: DOWN — RAW + opponent arms will still run; the OURS column needs 'cu start'."
  OURS_OK=0
fi
echo "  rustc: $(rustc --version)"

# GPU-provenance gate: a benchmark cell is only reproducible if the GPU was quiet
# when it was measured. The OURS arm auto-quiesces the live personas (eval-preemption
# lease), but the RAW/opponent scratch lanes still share the one GPU, so a busy box
# corrupts them silently. Refuse a contended run unless the operator accepts it.
GATE_FLAG=""; [ "$ALLOW_CONTENDED" = 1 ] && GATE_FLAG="--allow-contended"
if [ -f "$CODER/preflight_gpu.py" ]; then
  if ! python3 "$CODER/preflight_gpu.py" --cu "$CU" $GATE_FLAG; then
    die "GPU is CONTENDED — numbers taken now are not clean. Quiet the box (the OURS
       eval auto-quiesces personas; stop any other GPU job), then re-run — or pass
       --allow-contended to measure anyway and stamp the cells CONTENDED."
  fi
fi
echo "  gyms:  ${GYMS[*]}   limit: $LIMIT/gym   fleet: $MODELS"

say "resolve fleet (portable — downloads any missing opponent to your HF cache)"
FLEET_RESOLVED="$(mktemp -t fleet-resolved.XXXXXX.json)"
python3 "$CODER/resolve_fleet.py" --in "$MODELS" --out "$FLEET_RESOLVED"

for gym in "${GYMS[@]}"; do
  [ -f "$HERE/../docs/genome/$gym.jsonl" ] || { echo "  [skip gym] $gym — no docs/genome/$gym.jsonl"; continue; }
  say "KICK: $gym  (serve → RAW + OURS + opencode → grade → teardown, per model)"
  # sweep_all serves each opponent on its scratch port, runs matrix (which appends
  # every cell to RESULTS.jsonl), and tears the server down before the next model.
  python3 "$CODER/sweep_all.py" --models "$FLEET_RESOLVED" --benchmark "$gym" --limit "$LIMIT" --cu "$CU" \
    || echo "  [gym $gym] sweep returned nonzero — cells that DID land are in the ledger; continuing"
done

say "render the evidence board from the ledger"
python3 "$HERE/render_results.py" || echo "  (render skipped — inspect benchmarks/RESULTS.jsonl directly)"

say "done"
echo "  ledger:  benchmarks/RESULTS.jsonl   (append-only source of truth)"
echo "  board:   benchmarks/coder/MATRIX.md + benchmarks/charts/*.svg"
[ "$OURS_OK" = 0 ] && echo "  NOTE: OURS column is blank — start a core ('cu start') and re-run to fill it."
rm -f "$FLEET_RESOLVED"
