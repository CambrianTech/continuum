#!/usr/bin/env python3
"""Node/TypeScript dependency audit + ratchet for the headless Rust core.

WHY THIS EXISTS
---------------
Joel, 2026-08-06: "I almost removed node entirely from this project and we did
for a while and yet you assholes made it part of core again? Do not take this
for granted and fix."

He is right, and it is measurable. PR #1585 removed the implicit TS-bridge
fallthrough — its own regression test still says so:

    "Pre-PR #1585 the executor silently routed to the TS bridge on
     /tmp/jtag-command-router.sock ... The fix: refuse the implicit
     fallthrough, surface a typed error"

The implicit path was closed. Then EXPLICIT delegation grew back in its place,
one `execute_ts_json` call at a time, each individually defensible. Today the
headless core cannot serve grid connections or four sentinel step types without
a Node process listening on a Unix socket — a socket whose path is hardcoded to
`/tmp`, so on Windows it cannot even fail informatively.

This is the same shape as every other defect found the same night: not one bad
decision, but a boundary that nothing enforced, eroding one reasonable-looking
commit at a time.

WHY A RATCHET AND NOT A CLEANUP
-------------------------------
A one-time removal regresses the week after — that is exactly how we got here
the SECOND time. This records today's per-bucket counts and FAILS if any bucket
grows. Removals are welcome and re-baselining is expected; additions are not.

The baseline is therefore a WORK LIST, not an allowance. Every number in it is
a Node dependency in the headless core that should reach zero.

USAGE
-----
    python3 tools/scripts/node-dependency-audit.py            # report + ratchet check
    python3 tools/scripts/node-dependency-audit.py --write    # re-baseline (must go DOWN)
    python3 tools/scripts/node-dependency-audit.py --list grid # show the sites

Python is fine here: this is operator tooling ABOVE core, never the runtime
itself. The thing it guards is precisely the rule that core stays Rust.
"""

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SRC = REPO / "core" / "continuum-core" / "src"
BASELINE = Path(__file__).resolve().parent / "node-dependency-baseline.json"

# Markers that mean "the ENGINE needs a foreign runtime to work".
MARKERS = [
    (re.compile(r"\.execute_ts_json\s*\("), "ts-bridge-call"),
    (re.compile(r"jtag-command-router\.sock"), "ts-socket-path"),
    (re.compile(r"CommandRouterServer"), "ts-router"),
    # Python is WORSE than Node here, and the reason is not taste. Joel,
    # 2026-08-06: Node at least can be supervised; Python "CANNOT BE GOVERNED
    # — there is no way for the governor to lease, cap, or reclaim from it."
    # An ungovernable process on a shared box defeats the entire
    # resource-authority design: the governor accounts for what it can gate,
    # so a Python child is compute the substrate cannot see, throttle, or take
    # back under pressure. On a weak single-machine node that is the whole
    # budget walking out the door.
    (re.compile(r'Command::new\(\s*"python3?"'), "python-spawn"),
    (re.compile(r'"python3?"\s*\.to_string\(\)'), "python-spawn"),
]

# Paths where "python" is a LANGUAGE A CITIZEN WRITES IN, not a runtime the
# engine depends on. A persona authoring and running her own Python script is
# explicitly allowed — that is her tool, executed as her work, and the governor
# still owns the process that hosts it. Flagging these would make the guard cry
# wolf, and a guard that cries wolf gets disabled.
CITIZEN_TOOL_SURFACE = (
    "ai/json_in_prompt_tools.rs",
    "commands/code/run.rs",
    "cognition/tool_executor/",
)

# Which subsystem owns a path. Order matters — first match wins.
BUCKETS = [
    ("grid", "modules/grid/"),
    ("sentinel", "modules/sentinel/"),
    ("chat", "modules/chat/"),
    ("executor", "runtime/command_executor.rs"),
    ("cognition", "cognition/"),
]


def bucket_for(rel: str) -> str:
    for name, prefix in BUCKETS:
        if rel.startswith(prefix):
            return name
    return "other"


def is_ignorable(line: str) -> bool:
    """Comments and doc-comments describe the bridge; they do not depend on it.

    A doc that explains why the bridge is forbidden must not itself trip the
    guard — otherwise documenting the rule would break the build, and people
    would stop documenting it.
    """
    s = line.strip()
    return s.startswith("//") or s.startswith("*") or s.startswith("#")


def scan():
    """-> {bucket: [(rel_path, lineno, marker, text)]}"""
    found = {}
    for path in sorted(SRC.rglob("*.rs")):
        rel = path.relative_to(SRC).as_posix()
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        # `pub async fn execute_ts_json` is the DEFINITION — the escape hatch
        # itself. Counting it would make the hatch impossible to delete last.
        in_test = False
        for n, line in enumerate(lines, 1):
            if "#[cfg(test)]" in line:
                in_test = True
            if in_test or is_ignorable(line):
                continue
            if "pub async fn execute_ts_json" in line:
                continue
            for rx, marker in MARKERS:
                if marker == "python-spawn" and rel.startswith(CITIZEN_TOOL_SURFACE):
                    continue
                if rx.search(line):
                    found.setdefault(bucket_for(rel), []).append(
                        (rel, n, marker, line.strip())
                    )
                    break
    return found


def counts(found):
    return {k: len(v) for k, v in sorted(found.items())}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="re-baseline (must go down)")
    ap.add_argument("--list", metavar="BUCKET", help="show sites in a bucket")
    ap.add_argument("--widen-detection", action="store_true", help="allow an UPWARD re-baseline ONLY because the scanner improved")
    args = ap.parse_args()

    found = scan()
    current = counts(found)

    if args.list:
        for rel, n, marker, text in found.get(args.list, []):
            print(f"{rel}:{n}  [{marker}]  {text}")
        return 0

    prior = {}
    if BASELINE.exists():
        prior = json.loads(BASELINE.read_text(encoding="utf-8")).get("buckets", {})

    total_now = sum(current.values())
    total_was = sum(prior.values()) if prior else total_now

    print("Foreign-runtime dependencies in the headless Rust ENGINE (each one is a Node or Python process the engine cannot run without — and Python cannot be governed at all):\n")
    for bucket in sorted(set(current) | set(prior)):
        now, was = current.get(bucket, 0), prior.get(bucket, 0)
        arrow = "  " if now == was else ("DOWN" if now < was else "UP  ")
        print(f"  {arrow}  {bucket:<10} {was:>3} -> {now:>3}")
    print(f"\n  total {total_was} -> {total_now}")

    if args.write:
        if prior and total_now > total_was and not args.widen_detection:
            print("\nREFUSING to re-baseline UPWARD. The baseline is a work list, not an allowance.")
            print("If the SCANNER got better (not the code got worse), pass --widen-detection.")
            return 1
        if args.widen_detection and total_now > total_was:
            # A ratchet that cannot distinguish "we added dependencies" from
            # "we started detecting more" will either block honest improvements
            # to itself, or get its refusal routed around. Naming the case is
            # the fix: the number goes up ONLY because the scan widened, and
            # saying so out loud is what stops it becoming a quiet allowance.
            print(f"\n--widen-detection: scan widened, {total_was} -> {total_now}.")
            print("These were ALWAYS here; we simply could not see them before.")
        BASELINE.write_text(
            json.dumps({"buckets": current, "total": total_now}, indent=2) + "\n",
            encoding="utf-8",
        )
        print("\nbaseline written")
        return 0

    grown = [b for b in current if current[b] > prior.get(b, 0)] if prior else []
    if grown:
        print(f"\nFAIL: Node dependency GREW in: {', '.join(sorted(grown))}")
        print("The headless core must not gain new Node dependencies. Joel removed Node once")
        print("already and it grew back one reasonable-looking commit at a time.")
        print("Inspect:  python3 tools/scripts/node-dependency-audit.py --list <bucket>")
        return 1

    if prior and total_now < total_was:
        print("\nDown. Re-baseline with --write to lock the gain in.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
