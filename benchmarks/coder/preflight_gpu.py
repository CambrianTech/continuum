#!/usr/bin/env python3
"""
preflight_gpu.py — refuse to sell a contended number as a clean one.

The whole point of the kicking machine is that a stranger reproduces our numbers
and gets OURS. That guarantee dies the moment a measurement is taken on a GPU that
is busy doing something else — on our box, the live persona-serving lane; on a
stranger's box, a game, another model, a training run. The number still prints; it
is just wrong, and nothing downstream says so. That is the exact failure that
produced a `None/7` flagship cell (the benchmark time-shared the citizens' awake
:58057 lane).

So before a run, we do two honest things:
  1. MEASURE GPU slack (via a Continuum core's `gpu/stats`, if one is up).
  2. EMIT a one-line provenance verdict — CLEAN / CONTENDED / UNKNOWN — that the
     caller stamps next to the numbers, and exit non-zero on CONTENDED unless the
     operator has explicitly accepted it (`--allow-contended`) or quiesced the box.

This is portable: with no core running (a pure-opponent RAW/opencode sweep) there is
nothing to contend for from our side, so we report UNKNOWN and let it proceed — the
gate only bites when a core IS up and its GPU is already loaded.

Usage:
  python3 preflight_gpu.py --cu /path/to/cu            # verdict to stdout, exit code is the gate
  python3 preflight_gpu.py --cu ... --allow-contended  # warn but never block
  python3 preflight_gpu.py --cu ... --threshold 0.15   # override the "quiet" bar

Exit codes:
  0  CLEAN or UNKNOWN or (CONTENDED + --allow-contended)  -> safe to measure
  3  CONTENDED and not accepted                           -> caller should quiesce first
"""
import argparse, json, os, shutil, subprocess, sys


def _resolve_cli():
    """Locate the continuum CLI.

    `uu` is THE official short alias (the double-U of contin-UU-m). `uu` is
    /usr/bin/cu (UUCP) on every Unix and was never ours — a default pointing at a
    `uu` binary resolved to a file that does not exist, so the harness failed at
    the first invocation instead of running. Prefer what is actually installed on
    PATH; fall back to the release build.
    """
    for name in ("uu", "continuum"):
        found = shutil.which(name)
        if found:
            return found
    return os.path.expanduser("~/.continuum/cache/cargo-target/release/continuum")


def measurement_in_flight():
    """Structural contention check: is ANOTHER benchmark/eval already using the GPU?

    `gpu/stats` pressure is momentary — it reads ~0 between an eval's per-task
    generations, so a pressure-only gate blesses a box that's mid-measurement. A
    running `cognition/eval` / `benchmark/run` (or another `kick.sh`) is an
    unambiguous, non-momentary signal that the GPU is spoken for. Returns the
    offending process line, or None.
    """
    try:
        out = subprocess.run(["pgrep", "-af", "cognition/eval|benchmark/run|coder/matrix|sweep_all"],
                             capture_output=True, text=True, timeout=5)
    except Exception:
        return None
    for line in out.stdout.splitlines():
        # ignore our own pgrep / this preflight
        if "pgrep" in line or "preflight_gpu" in line:
            continue
        if line.strip():
            return line.strip()
    return None


def gpu_slack(cu):
    """Return (pressure, used_mb, total_mb) from the live core, or None if no core."""
    try:
        out = subprocess.run([cu, "gpu/stats"], capture_output=True, text=True, timeout=15)
    except Exception:
        return None
    if out.returncode != 0 or not out.stdout.strip():
        return None
    try:
        d = json.loads(out.stdout)
    except json.JSONDecodeError:
        # some uu builds prefix a human line; grab the JSON object tail.
        s = out.stdout
        i = s.find("{")
        if i < 0:
            return None
        try:
            d = json.loads(s[i:])
        except json.JSONDecodeError:
            return None
    return (
        float(d.get("pressure", 0.0)),
        float(d.get("total_used_mb", 0.0)),
        float(d.get("total_vram_mb", 0.0)),
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--uu", default=_resolve_cli(), help="path to the continuum CLI (uu)")
    ap.add_argument("--threshold", type=float, default=0.10,
                    help="GPU pressure at/above which a measurement is CONTENDED (default 0.10)")
    ap.add_argument("--allow-contended", action="store_true",
                    help="warn on contention but do not block (exit 0)")
    args = ap.parse_args()

    # Structural check FIRST — a running measurement is unambiguous, where pressure
    # is momentary (reads ~0 between an eval's per-task generations). This is the
    # fix for the pressure-only gate blessing a box that's mid-eval.
    busy = measurement_in_flight()
    if busy:
        print("gpu-provenance: CONTENDED — another measurement already owns the GPU:",
              file=sys.stderr)
        print(f"    {busy}", file=sys.stderr)
        print("  wait for it to finish (or kill it) before starting a new run; two "
              "measurements time-sharing one GPU corrupt BOTH.\n"
              "  Pass --allow-contended to override.", file=sys.stderr)
        return 0 if args.allow_contended else 3

    slack = gpu_slack(args.uu)
    if slack is None:
        # No core, or gpu/stats unavailable -> nothing on OUR side to contend. The
        # opponent arms serve their own scratch lanes; report UNKNOWN and proceed.
        print("gpu-provenance: UNKNOWN (no core / no gpu-stats — opponent-only arms are self-served)")
        return 0

    pressure, used_mb, total_mb = slack
    if pressure >= args.threshold:
        verdict = (f"gpu-provenance: CONTENDED (pressure={pressure:.2f}, "
                   f"used={used_mb:.0f}/{total_mb:.0f} MB) — the GPU is already busy; "
                   f"numbers taken now are NOT clean")
        print(verdict, file=sys.stderr)
        print("  quiet it first (on a Continuum box: sleep the live personas with "
              "`uu cognition/set-sleep-mode --mode sleeping --duration-minutes N`,\n"
              "  or on any box stop the other GPU job), then re-run. "
              "Pass --allow-contended to measure anyway and stamp the number CONTENDED.",
              file=sys.stderr)
        return 0 if args.allow_contended else 3

    print(f"gpu-provenance: CLEAN (pressure={pressure:.2f}, used={used_mb:.0f}/{total_mb:.0f} MB) "
          f"— the GPU is quiet; measurements are trustworthy")
    return 0


if __name__ == "__main__":
    sys.exit(main())
