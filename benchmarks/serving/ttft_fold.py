#!/usr/bin/env python3
"""Fold `inference.prefill.complete` probe rows into a TTFT / KV-reuse receipt.

The claim this instruments: agent-loop serving on consumer hardware, measured as
time-to-first-token under real context churn — the axis FreeToken (Berkeley,
arXiv 2608.16157) benchmarks against stock llama.cpp (their table: FreeToken
worst-case TTFT <=44s, llama.cpp 232s, KTransformers 946s). Our serving is
policy on top of a llama.cpp lineage (slot affinity + prefix-stable prompt
ordering + save/restore), so the honest comparison is our measured distribution
beside their published one, conditions fully disclosed.

Every live stream emits one probe row at the prefill->decode edge with QUEUE and
INGEST separated (`queued_ms` = slot wait, `ingest_ms` = real prefill work), the
cache's actual contribution (`cached` of `total` tokens), and the ingest rate.
TTFT here = queued_ms + ingest_ms: everything before the first decoded token
except decode itself. This folds a window of those rows into the distribution.

Usage:
  python3 benchmarks/serving/ttft_fold.py --since-hours 6 --note "hard-rs Take 3 battery"
  python3 benchmarks/serving/ttft_fold.py --since-ms 1787460000000 --json out.json

Provenance rules (benchmarks/RESULTS.jsonl doctrine): rows are only comparable
when the window covers a single core build — pass a window that starts at the
deploy you are claiming numbers for, and record git SHA + model + machine.
"""

import argparse
import json
import statistics
import subprocess
import sys
import time


def query_rows(since_ms: int, limit: int) -> list[dict]:
    cmd = [
        "continuum", "debug/probes/query",
        "--class", "inference.prefill.complete",
        "--limit", str(limit),
        "--sinceMs", str(since_ms),
    ]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    events = json.loads(out).get("events", [])
    rows = []
    for e in events:
        f = e.get("fields", {})
        try:
            rows.append({
                "at_ms": e["capturedAtMs"],
                "total": int(f["total"]),
                "cached": int(f["cached"]),
                "fresh": int(f["fresh"]),
                "queued_ms": int(f["queued_ms"]),
                "ingest_ms": int(f["ingest_ms"]),
                "ingest_tok_per_s": int(f["ingest_tok_per_s"]),
                "provider": f.get("provider", ""),
            })
        except (KeyError, ValueError):
            continue  # a malformed row is dropped, never invented
    return rows


def pct(xs: list[float], p: float) -> float:
    if not xs:
        return 0.0
    xs = sorted(xs)
    k = (len(xs) - 1) * p
    lo, hi = int(k), min(int(k) + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def fold(rows: list[dict]) -> dict:
    ttft_s = [(r["queued_ms"] + r["ingest_ms"]) / 1000.0 for r in rows]
    reuse = [r["cached"] / r["total"] for r in rows if r["total"] > 0]
    ingest = [r["ingest_tok_per_s"] for r in rows if r["fresh"] > 0]
    total_tok = sum(r["total"] for r in rows)
    cached_tok = sum(r["cached"] for r in rows)
    # The ablation column: what the same prompts would have cost with zero
    # reuse at each stream's own measured ingest rate. Not a simulation of a
    # different engine — the same rows, cache contribution removed.
    uncached_s = [
        r["total"] / r["ingest_tok_per_s"] + r["queued_ms"] / 1000.0
        for r in rows if r["ingest_tok_per_s"] > 0
    ]
    return {
        "streams": len(rows),
        "window_start_ms": min(r["at_ms"] for r in rows),
        "window_end_ms": max(r["at_ms"] for r in rows),
        "ttft_s": {
            "median": round(statistics.median(ttft_s), 2),
            "p95": round(pct(ttft_s, 0.95), 2),
            "worst": round(max(ttft_s), 2),
        },
        "kv_reuse": {
            "aggregate_pct": round(100.0 * cached_tok / total_tok, 1) if total_tok else 0.0,
            "median_stream_pct": round(100.0 * statistics.median(reuse), 1) if reuse else 0.0,
            "tokens_total": total_tok,
            "tokens_from_cache": cached_tok,
        },
        "ingest_tok_per_s": {
            "median": round(statistics.median(ingest), 0) if ingest else 0,
            "p95": round(pct(ingest, 0.95), 0) if ingest else 0,
        },
        "zero_reuse_equivalent_ttft_s": {
            "median": round(statistics.median(uncached_s), 2) if uncached_s else 0.0,
            "worst": round(max(uncached_s), 2) if uncached_s else 0.0,
        },
        "prompt_tokens": {
            "median": int(statistics.median([r["total"] for r in rows])),
            "max": max(r["total"] for r in rows),
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--since-hours", type=float, default=None)
    ap.add_argument("--since-ms", type=int, default=None)
    ap.add_argument("--limit", type=int, default=20000)
    ap.add_argument("--note", default="")
    ap.add_argument("--json", dest="json_out", default=None,
                    help="also write the receipt as JSON to this path")
    args = ap.parse_args()

    if args.since_ms is not None:
        since = args.since_ms
    elif args.since_hours is not None:
        since = int((time.time() - args.since_hours * 3600) * 1000)
    else:
        ap.error("pass --since-hours or --since-ms (an unbounded window mixes builds)")

    rows = query_rows(since, args.limit)
    if not rows:
        print("no inference.prefill.complete rows in the window — the instrument "
              "emits per live stream, so an empty fold means no serving happened "
              "in the window (or the window predates the probe store).", file=sys.stderr)
        return 1

    sha = subprocess.run(["git", "rev-parse", "--short", "HEAD"],
                         capture_output=True, text=True).stdout.strip()
    receipt = {"git_sha": sha, "note": args.note, **fold(rows)}

    r = receipt
    print(f"streams: {r['streams']}   window: "
          f"{time.strftime('%Y-%m-%d %H:%M', time.localtime(r['window_start_ms']/1000))} -> "
          f"{time.strftime('%H:%M', time.localtime(r['window_end_ms']/1000))}   sha: {sha}")
    print(f"TTFT (s): median {r['ttft_s']['median']}  p95 {r['ttft_s']['p95']}  "
          f"worst {r['ttft_s']['worst']}")
    print(f"KV reuse: {r['kv_reuse']['aggregate_pct']}% aggregate "
          f"({r['kv_reuse']['tokens_from_cache']:,}/{r['kv_reuse']['tokens_total']:,} tokens), "
          f"median stream {r['kv_reuse']['median_stream_pct']}%")
    print(f"ingest: median {r['ingest_tok_per_s']['median']:.0f} tok/s   "
          f"prompts: median {r['prompt_tokens']['median']:,} tok, max {r['prompt_tokens']['max']:,}")
    print(f"zero-reuse equivalent TTFT (same rows, cache removed): "
          f"median {r['zero_reuse_equivalent_ttft_s']['median']}s  "
          f"worst {r['zero_reuse_equivalent_ttft_s']['worst']}s")

    if args.json_out:
        with open(args.json_out, "w") as fh:
            json.dump(receipt, fh, indent=2)
        print(f"receipt -> {args.json_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
