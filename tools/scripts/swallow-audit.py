#!/usr/bin/env python3
"""Error-swallow audit + ratchet for the Rust workspace.

WHY THIS EXISTS
---------------
Joel, 2026-08-05: "this philosophy of 'errors are bad' has ruined most of my
projects and I have to berate you about it or you won't fix it. Same for unwrap
or try/catch which you've violated probably in every file you've touched. It's
WHY THIS IS BRITTLE."

He was right, and it is measurable: 1,826 production sites where a failure is
converted into something that looks like success. Four bugs in ONE day traced to
exactly this — `pgrep`'s failure swallowed by `.ok()` so reboot never swapped the
binary; `current_rss_mb()` ending in `.unwrap_or(0)` so the OOM guard compared a
constant zero; `Err(_) => Vec::new()` in delivery-truth so a dead daemon read as
"nothing to report"; `|| true` in a hook so a broken client looked like an empty
memory.

A swallowed error is not a smaller failure — it is an UNBOUNDED one. The failure
still happens; what's removed is the bound on how long it stays invisible.

WHAT THIS IS
------------
Not a one-time cleanup script — those regress the week after. This is a RATCHET:
it records today's per-bucket counts as a baseline and FAILS CI if any bucket
grows. Existing debt is paid down deliberately; new debt cannot be added. Same
shape as the repo's ts-eslint-baseline-ratchet.

Sites are judged by WHAT A FAILURE DOES, not by syntax:

  FABRICATES  substitutes a plausible value for an unknown one. The worst class,
              because the fiction flows downstream and gets reasoned about. A
              default is only legitimate when it is DISTINGUISHABLE from a real
              value, or reported (see `governed_vram_ceiling_or_report`).
  DISCARDS    a Result carrying a real failure is dropped wholesale.
  PANICS      .unwrap()/.expect() on a fallible op — loud, but takes the process.
  LOCK        .lock().unwrap() — conventional Rust; only fails after another
              panic already poisoned the mutex. Lowest priority, not zero.
  TEST        inside #[cfg(test)] / tests/ / benches/ — legitimate, not counted.

USAGE
    python3 tools/scripts/swallow-audit.py            # report + ratchet check
    python3 tools/scripts/swallow-audit.py --write    # re-baseline (must go DOWN)
    python3 tools/scripts/swallow-audit.py --list FABRICATES   # show the sites

Exit 0 = at or under baseline. Exit 1 = a bucket grew (or the baseline is stale
because debt was paid and not re-recorded — also worth knowing).
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
RUST_ROOT = REPO / "core"
BASELINE = REPO / "tools" / "scripts" / "swallow-baseline.json"

# Vendored/generated trees are not ours to fix; excluded from the ratchet so a
# submodule bump can never fail an unrelated PR.
SKIP_FRAGMENTS = ("/target/", "/vendor/", "llama.cpp", "/.git/", "/vendored/")

# A numeric/empty default substituted for an unknown value.
FABRICATE = re.compile(
    r"\.unwrap_or\(\s*(0|0\.0|0u\d+|0i\d+|1|false|\"\")\s*\)|\.unwrap_or_default\(\)"
)
# A Result dropped on the floor.
DISCARD = re.compile(r"Err\(_\)\s*=>|let\s+_\s*=\s*\w|\.ok\(\)\s*[;,)]")
PANIC = re.compile(r"\.unwrap\(\)|\.expect\(")
# Checked BEFORE panic so a poisoned-mutex unwrap is not miscounted as a
# fallible-op unwrap — they carry very different risk.
LOCK = re.compile(r"\.(lock|read|write)\(\)\s*\.unwrap\(\)|\.lock\(\)\.expect\(")

BUCKETS = ("FABRICATES", "DISCARDS", "PANICS", "LOCK")


def classify(line: str) -> str | None:
    if LOCK.search(line):
        return "LOCK"
    if FABRICATE.search(line):
        return "FABRICATES"
    if PANIC.search(line):
        return "PANICS"
    if DISCARD.search(line):
        return "DISCARDS"
    return None


def scan() -> tuple[Counter, dict[str, Counter], dict[str, list[str]]]:
    counts: Counter = Counter()
    by_file: dict[str, Counter] = defaultdict(Counter)
    sites: dict[str, list[str]] = defaultdict(list)

    for path in sorted(RUST_ROOT.rglob("*.rs")):
        sp = str(path)
        if any(frag in sp for frag in SKIP_FRAGMENTS):
            continue
        is_test_path = "/tests/" in sp or "/benches/" in sp or sp.endswith("_test.rs")
        # A file we cannot read is itself a finding — never silently skipped,
        # which would be this script committing the sin it audits.
        try:
            lines = path.read_text(errors="replace").splitlines()
        except OSError as exc:
            print(f"AUDIT ERROR: cannot read {path}: {exc}", file=sys.stderr)
            raise SystemExit(2) from exc

        # Everything at/after the first `#[cfg(test)]` is test-only code.
        cfg_test_at = next(
            (i for i, ln in enumerate(lines) if ln.strip().startswith("#[cfg(test)]")),
            len(lines),
        )
        rel = str(path.relative_to(REPO))

        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith(("//", "///", "*", "#[doc")):
                continue
            bucket = classify(line)
            if bucket is None:
                continue
            if is_test_path or i >= cfg_test_at:
                counts["TEST"] += 1
                continue
            counts[bucket] += 1
            by_file[bucket][rel] += 1
            sites[bucket].append(f"{rel}:{i + 1}: {stripped[:120]}")

    return counts, by_file, sites


def changed_files(ref: str) -> list[str]:
    """Rust files changed vs `ref`. A git failure is NOT swallowed into an empty
    list — an empty list would silently pass the gate, which is this script's own
    sin. It raises."""
    import subprocess

    proc = subprocess.run(
        ["git", "diff", "--name-only", "--diff-filter=d", ref, "--", "core/"],
        cwd=REPO,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(f"AUDIT ERROR: `git diff {ref}` failed: {proc.stderr.strip()}", file=sys.stderr)
        raise SystemExit(2)
    return [f for f in proc.stdout.split() if f.endswith(".rs")]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--write", action="store_true", help="re-record the baseline (counts must not grow)")
    ap.add_argument("--list", metavar="BUCKET", choices=BUCKETS, help="print every site in a bucket")
    ap.add_argument(
        "--changed",
        metavar="GIT_REF",
        help="BOY-SCOUT GATE: every Rust file changed vs GIT_REF must leave with ZERO "
        "production swallows. Joel, 2026-08-05: 'All smell must be removed as we work "
        "on a file.' Touching a file makes its debt yours.",
    )
    args = ap.parse_args()

    counts, by_file, sites = scan()

    if args.changed:
        touched = changed_files(args.changed)
        if not touched:
            print(f"no Rust files changed vs {args.changed} — boy-scout gate not applicable.")
            return 0
        dirty: dict[str, dict[str, int]] = {}
        for f in touched:
            per = {b: by_file[b].get(f, 0) for b in BUCKETS if by_file[b].get(f, 0)}
            if per:
                dirty[f] = per
        print(f"boy-scout gate — {len(touched)} Rust file(s) changed vs {args.changed}\n")
        if not dirty:
            print("CLEAN — every touched file leaves with zero production swallows.")
            return 0
        print("GATE FAILED — you touched these files, so their smell is now yours to remove:\n")
        for f, per in sorted(dirty.items(), key=lambda kv: -sum(kv[1].values())):
            summary = ", ".join(f"{b}={n}" for b, n in per.items())
            print(f"  {f}  ({summary})")
            for b in per:
                for s in sites[b]:
                    if s.startswith(f + ":"):
                        print(f"        {s}")
        print(
            "\nFix each one so a failure stays VISIBLE: propagate the Result, or keep a\n"
            "value distinguishable from a real one AND record the substitution\n"
            "(probe!/tracing::warn!). See governed_vram_ceiling_or_report in\n"
            "modules/serving_daemon.rs for the shape."
        )
        return 1

    if args.list:
        for s in sites[args.list]:
            print(s)
        return 0

    prod = sum(counts[b] for b in BUCKETS)
    print("error-swallow audit — Rust workspace (core/)\n")
    print(f"{'bucket':<12}{'count':>8}   a failure at these sites…")
    print("-" * 74)
    for b, desc in [
        ("FABRICATES", "…becomes a made-up value a decision consumes  ← WORST"),
        ("DISCARDS", "…is dropped wholesale, caller sees success"),
        ("PANICS", "…kills the process (loud, at least)"),
        ("LOCK", "…only after a prior panic poisoned the lock"),
    ]:
        print(f"{b:<12}{counts[b]:>8}   {desc}")
    print(f"{'TEST':<12}{counts['TEST']:>8}   (legitimate test code — not ratcheted)")
    print(f"\nPRODUCTION TOTAL: {prod}")

    if args.write:
        BASELINE.write_text(
            json.dumps({b: counts[b] for b in BUCKETS} | {"_production_total": prod}, indent=2) + "\n"
        )
        print(f"\nbaseline written → {BASELINE.relative_to(REPO)}")
        return 0

    if not BASELINE.exists():
        print(f"\nNo baseline at {BASELINE.relative_to(REPO)} — run with --write to record one.")
        return 1

    base = json.loads(BASELINE.read_text())
    grew, shrank = [], []
    for b in BUCKETS:
        was, now = base.get(b, 0), counts[b]
        if now > was:
            grew.append((b, was, now))
        elif now < was:
            shrank.append((b, was, now))

    for b, was, now in shrank:
        print(f"  ↓ {b}: {was} → {now} (debt paid — re-baseline with --write)")

    if grew:
        print("\nRATCHET FAILED — new error-swallowing was added:\n")
        for b, was, now in grew:
            print(f"  ↑ {b}: {was} → {now}  (+{now - was})")
            worst = by_file[b].most_common(5)
            for f, n in worst:
                print(f"        {n:>3}  {f}")
        print(
            "\nA hook/handler that must not break its caller may still not DESTROY the\n"
            "error: record it (probe!/tracing::warn!/a receipt) and keep a value that is\n"
            "distinguishable from a real one. See governed_vram_ceiling_or_report in\n"
            "modules/serving_daemon.rs for the shape this repo expects.\n"
            "Inspect the sites:  python3 tools/scripts/swallow-audit.py --list <BUCKET>"
        )
        return 1

    print("\nratchet OK — no new error-swallowing.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
