#!/usr/bin/env python3
"""
emergence_meter.py — the objective read on "is the society organizing itself?"
(docs/design/EMERGENT-TEAM-ORGANIZATION.md § The emergence meter).

Reads the day's core log + the airc work board and prints the counters that make
"it just happens when they're freely communicating" falsifiable:

  channel     — persona.turn.spoke / persona.turn.start (is the channel alive?)
  voices      — which personas spoke, how often (heterogeneity of participation)
  initiative  — card claims/completions per persona (acting unprompted)
  self-edits  — persona/identity/set calls (are they differentiating?)
  peer-chains — spokes within 10min of ANOTHER persona's spoke (conversation,
                not broadcast-response; operator-triggered turns excluded)

Usage: python3 benchmarks/team/emergence_meter.py [--log PATH] [--since HH:MM]
"""
import argparse
import glob
import json
import os
import re
import subprocess
import sys
from collections import defaultdict

ANSI = re.compile(r"\x1b\[[0-9;]*m")
TS = re.compile(r"^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}):")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", default=None)
    ap.add_argument("--since", default="00:00", help="HH:MM UTC floor")
    args = ap.parse_args()

    log = args.log or max(
        glob.glob(os.path.expanduser("~/.continuum/logs/continuum-core-server.*.log")),
        key=os.path.getmtime,
    )
    starts = defaultdict(int)
    spokes = []  # (hhmm, persona)
    edits = defaultdict(int)
    with open(log, errors="replace") as f:
        for line in f:
            line = ANSI.sub("", line)
            m = TS.match(line)
            if not m or m.group(2) < args.since:
                continue
            hhmm = m.group(2)
            who = None
            wm = re.search(r"persona=(\w+)", line)
            if wm:
                who = wm.group(1)
            if "persona.turn.start" in line:
                starts[who or "?"] += 1
            elif "persona.turn.spoke" in line:
                spokes.append((hhmm, who or "?"))
            elif "persona/identity/set" in line and "command" in line:
                edits[who or "?"] += 1

    total_starts = sum(starts.values())
    print(f"═══ emergence meter · {os.path.basename(log)} since {args.since}Z ═══")
    print(f"channel:   {len(spokes)} spoke / {total_starts} started "
          f"({(100 * len(spokes) / total_starts):.1f}%)" if total_starts else "channel:   no turns")
    by_voice = defaultdict(int)
    for _, w in spokes:
        by_voice[w] += 1
    print("voices:   ", dict(by_voice) or "none")
    print("self-edits:", dict(edits) or "none")

    # peer chains: a spoke ≤10min after a DIFFERENT persona's spoke
    chains = 0
    for i, (t, w) in enumerate(spokes):
        for pt, pw in spokes[max(0, i - 6):i]:
            if pw != w:
                h1, m1 = map(int, t.split(":"))
                h2, m2 = map(int, pt.split(":"))
                if 0 < (h1 * 60 + m1) - (h2 * 60 + m2) <= 10:
                    chains += 1
                    break
    print(f"peer-chains: {chains} (spokes within 10min of another persona's spoke)")

    # board initiative via airc (best-effort; absent airc → skipped, stated)
    try:
        board = subprocess.run(["airc", "work", "board"], capture_output=True,
                               text=True, timeout=15).stdout
        claims = len(re.findall(r"claim=(?!-)", board))
        done = board.count("Done")
        print(f"board:     {claims} claimed, {done} done")
    except Exception as e:
        print(f"board:     unavailable ({e})")


if __name__ == "__main__":
    sys.exit(main())
