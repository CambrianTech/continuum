#!/bin/bash
# Sweep ONE residency axis and emit (setting, measured decode tok/s) as JSONL.
#
# This is the point of the harness: each line is exactly one observation that DivisionBandit.observe()
# consumes, and the whole file is the response surface the governor warm-starts from. Sweeping by hand
# and eyeballing the best value is the thing this replaces -- the agent should learn the surface.
#
#   usage: tools/dev/moe/sweep.sh division [n_cpu_moe ...]     # how many expert layers stay on CPU
#          tools/dev/moe/sweep.sh budget   [cache_gb ...]      # device-resident expert cache size
#
# division: fewer CPU layers => more experts resident on the GPU => less per-token transfer.
# budget:   larger cache     => higher hit rate, until the clamp or the card runs out.
#
# IMPORTANT (measurement discipline): a single point is not a measurement. Repeated identical configs
# on V4-Flash varied ~35% run to run, because decode rate tracks whether a token's experts happened to
# be resident. Establish the noise floor with --reps before believing any difference between points.

cd "$(dirname "${BASH_SOURCE[0]}")" || exit 1
source ./common.sh

AXIS="${1:-}"; shift || true
case "$AXIS" in
    division|budget) ;;
    *) die "usage: sweep.sh {division|budget} [values ...]" ;;
esac

REPS="${CONTINUUM_MOE_SWEEP_REPS:-1}"
POINTS=("$@")
if [ "${#POINTS[@]}" -eq 0 ]; then
    if [ "$AXIS" = division ]; then POINTS=(48 40 34 30 26); else POINTS=(0 6 12 18 24); fi
fi

MODEL=$(resolve_model) || exit 1
OUT="$OUT_DIR/${AXIS}-curve.jsonl"
: > "$OUT"

export GGML_OP_OFFLOAD_MIN_BATCH=1
export GGML_MOE_OFFLOAD_STATS=1
unset GGML_MOE_CONTAINER

echo "engine : $ENGINE"
echo "model  : $MODEL"
echo "sweep  : $AXIS over ${POINTS[*]}, $REPS rep(s) each -> $OUT"

for P in "${POINTS[@]}"; do
    for R in $(seq 1 "$REPS"); do
        if [ "$AXIS" = division ]; then
            NCPU="$P"; CACHE_GB=0
        else
            NCPU=48;   CACHE_GB="$P"
        fi
        LOG="$OUT_DIR/sweep-$AXIS-$P-r$R.log"
        export GGML_MOE_VRAM_CACHE_GB="$CACHE_GB"

        echo "--- $AXIS=$P rep=$R ---"
        "$ENGINE" --model "$MODEL" --n-gpu-layers 99 --n-cpu-moe "$NCPU" \
                  -c 8192 --port "$PORT" --host 127.0.0.1 > "$LOG" 2>&1 &
        SV=$!

        wait_for_listen "$LOG"; rc=$?
        if [ $rc -ne 0 ]; then
            # A boundary is DATA, not an error: it is where the card runs out, and the governor needs
            # to know that. Record it and keep going.
            status=$([ $rc -eq 1 ] && echo "fatal" || echo "timeout")
            echo "  $status at $AXIS=$P (curve boundary)"
            echo "{\"axis\":\"$AXIS\",\"value\":$P,\"rep\":$R,\"status\":\"$status\"}" >> "$OUT"
            reap_engine "$SV"; continue
        fi

        curl -s "http://127.0.0.1:$PORT/v1/chat/completions" -H 'Content-Type: application/json' \
             -d "$CODING_PROMPT" -o /dev/null -w "  http %{http_code} %{time_total}s\n"

        VRAM=$(vram_used_mb)
        TOKS=$(decode_tok_s "$LOG")
        HIT=$(grep -aoE "hit_rate=[0-9.]+" "$LOG" | tail -1 | cut -d= -f2)
        if [ -z "$TOKS" ]; then
            # No eval-time line => the request never decoded. Emitting 0 would poison the curve with a
            # measurement that never happened, so say so instead.
            echo "  NO DECODE MEASUREMENT at $AXIS=$P"
            echo "{\"axis\":\"$AXIS\",\"value\":$P,\"rep\":$R,\"status\":\"no_measurement\"}" >> "$OUT"
        else
            echo "  $AXIS=$P vram_used=${VRAM:-?}MB decode=$TOKS tok/s hit_rate=${HIT:-n/a}"
            echo "{\"axis\":\"$AXIS\",\"value\":$P,\"rep\":$R,\"vram_used_mb\":${VRAM:-0},\"decode_tok_s\":$TOKS,\"hit_rate\":${HIT:-null}}" >> "$OUT"
        fi
        reap_engine "$SV"
    done
done

echo "=== curve ==="; cat "$OUT"
