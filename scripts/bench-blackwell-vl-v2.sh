#!/usr/bin/env bash
# Blackwell RTX 5090 sm_120 V2 sensory bench against the opaque manifest
# at test-data/images/manifest.json. Produces per-fixture PASS/FAIL based
# on grade_expected_substrings rather than visual review.
#
# V2 motivation (Codex methodology flag 2026-05-11): v1 used cat.jpg +
# Wikipedia commons, which is training-distribution-leaky. v2 uses
# manifest-anchored opaque fixtures so vision-vs-bluff is measurable.
#
# Idempotent: reuses omni-bench-work named volume (from v1 build), stages
# test-data/images into it via tar pipe (Docker Desktop WSL2 doesn't
# bind-mount /home paths cleanly).
#
# Usage:
#   scripts/bench-blackwell-vl-v2.sh
#
# Env:
#   MANIFEST_HOST   path to manifest.json (default: repo's test-data/images)
#   CUDA_ARCH       (default: 120-real for sm_120; use 'native' to auto-detect)
#   CUDA_IMAGE      (default: nvidia/cuda:12.8.0-devel-ubuntu22.04)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST_HOST="${MANIFEST_HOST:-$REPO_ROOT/test-data/images}"
CUDA_ARCH="${CUDA_ARCH:-120-real}"
CUDA_IMAGE="${CUDA_IMAGE:-nvidia/cuda:12.8.0-devel-ubuntu22.04}"
VOLUME="omni-bench-work"

if [ ! -f "$MANIFEST_HOST/manifest.json" ]; then
    echo "ERROR: manifest.json not found at $MANIFEST_HOST/manifest.json" >&2
    exit 1
fi

docker volume create "$VOLUME" >/dev/null

echo "=== stage fixtures + manifest into $VOLUME ==="
docker run --rm -i \
    -v "$VOLUME:/work" \
    --name "v2-stage-$(date +%s)" \
    "$CUDA_IMAGE" \
    sh -c 'mkdir -p /work/test-data/images && cd /work/test-data/images && tar xf -' \
    < <(cd "$MANIFEST_HOST" && tar c image-0.png image-1.png image-2.jpg image-3.jpg image-4.jpg image-5.jpg image-6.webp manifest.json)
echo "ok"

CONTAINER_NAME="v2-bench-$(date +%s)"
docker run --rm --gpus all \
    -v "$VOLUME:/work" \
    -w /work \
    --name "$CONTAINER_NAME" \
    "$CUDA_IMAGE" \
    bash -c '
set -euo pipefail
apt-get update -qq >/dev/null
apt-get install -y -qq python3 >/dev/null

# Verify llama.cpp build is cached in volume (from v1 bench harness)
if [ ! -x /work/llama.cpp/build/bin/llama-mtmd-cli ]; then
    echo "ERROR: /work/llama.cpp/build/bin/llama-mtmd-cli missing." >&2
    echo "  Run scripts/bench-blackwell-vl.sh first to seed the volume" >&2
    echo "  with llama.cpp build + Qwen models." >&2
    exit 1
fi

cat > /tmp/v2grade.py <<PYEOF
import json, subprocess, time, sys, argparse

ap = argparse.ArgumentParser()
ap.add_argument("--label", required=True)
ap.add_argument("--model", required=True)
ap.add_argument("--mmproj", required=True)
args = ap.parse_args()

with open("/work/test-data/images/manifest.json") as f:
    manifest = json.load(f)

results = []
for fx in manifest["fixtures"]:
    fname = fx["filename"]
    q = fx["grade_questions"][0]
    expected = fx["grade_expected_substrings"]
    image_path = f"/work/test-data/images/{fname}"
    t0 = time.time()
    try:
        proc = subprocess.run(
            ["/work/llama.cpp/build/bin/llama-mtmd-cli",
             "-m", args.model,
             "--mmproj", args.mmproj,
             "--image", image_path,
             "-p", q,
             "-ngl", "99",
             "-n", "120",
             "--temp", "0"],
            capture_output=True, text=True, timeout=180
        )
        # llama-mtmd-cli writes the model response to STDOUT and all
        # loading + encoding diagnostics + llama_perf summary to STDERR.
        response = (proc.stdout or "").strip()
    except Exception as e:
        response = f"(subprocess error: {e})"
    elapsed = time.time() - t0
    if not response:
        response = "(empty stdout)"

    resp_lower = response.lower()
    hits = [s for s in expected if s.lower() in resp_lower]
    threshold = max(1, len(expected) // 2)
    passed = len(hits) >= threshold
    ck = fx["content_kind"]
    lr = fx["leakage_risk"]
    verdict = "PASS" if passed else "FAIL"
    results.append((fname, ck, lr, q, expected, hits, response[:600], elapsed, verdict))
    print(f"  {fname:18} | {ck:30} | leakage={lr:35} | hits={len(hits)}/{len(expected)} | {verdict:4} | {elapsed:.1f}s")

print()
print("=== full responses ===")
for r in results:
    fname, ck, lr, q, expected, hits, response, elapsed, verdict = r
    print()
    print(f"--- {fname} ({verdict}) ---")
    print(f"  Q: {q}")
    print(f"  Expected: {expected}")
    print(f"  Hits: {hits}")
    print(f"  Response: {response}")

passes = sum(1 for r in results if r[8] == "PASS")
print()
print(f"=== SUMMARY: {args.label} = {passes}/{len(results)} fixtures PASS ===")
PYEOF

run_model() {
    local label="$1" model="$2" mmproj="$3"
    echo ""
    echo "=========================================================="
    echo "=== V2 BENCH: $label ==="
    echo "=========================================================="
    if [ ! -f "$model" ]; then echo "ERROR: missing $model (run scripts/bench-blackwell-vl.sh first)" >&2; return 1; fi
    if [ ! -f "$mmproj" ]; then echo "ERROR: missing $mmproj (run scripts/bench-blackwell-vl.sh first)" >&2; return 1; fi
    python3 /tmp/v2grade.py --label "$label" --model "$model" --mmproj "$mmproj" || true
}

run_model "Qwen2.5-Omni-7B" \
    /work/models/qwen25omni/Qwen2.5-Omni-7B-Q4_K_M.gguf \
    /work/models/qwen25omni/mmproj-Qwen2.5-Omni-7B-f16.gguf

run_model "Qwen3-Omni-30B-A3B-Instruct" \
    /work/models/qwen3omni30/Qwen3-Omni-30B-A3B-Instruct-Q4_K_M.gguf \
    /work/models/qwen3omni30/mmproj-Qwen3-Omni-30B-A3B-Instruct-bf16.gguf
'
