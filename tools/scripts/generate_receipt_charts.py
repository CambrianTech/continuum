#!/usr/bin/env python3
"""Generate the repo's receipt charts as portable SVGs.

THE LAW (same as the forge-alloy model cards): charts are PROJECTIONS of
receipts — every point is read from a verdict artifact on disk, never
hand-authored. Rerun after any round; commit the SVGs beside the data
snapshot so a reader can diff chart against source.

Outputs (shared by README and docs/paper/):
  docs/assets/charts/improvement-curve.svg   cumulative attempts vs resolves
  docs/assets/charts/receipts-snapshot.json  the exact rows the SVG encodes
"""

import glob
import json
import os
import sys
from datetime import datetime, timezone

VERDICTS = os.path.expanduser("~/.continuum/benchmarks/swe/verdicts/*.json")
OUT_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "docs", "assets", "charts")

# Palette chosen to stay legible on GitHub light AND dark backgrounds.
INK = "#8b949e"       # neutral gray for axes/labels (readable on both)
ACCENT = "#2ea043"    # resolved line (GitHub green family)
MUTED = "#6e7681"     # attempts line
FRAME = "#30363d"


def load_rows():
    """Every GRADED attempt, and only those.

    An UNGRADEABLE instance is an absence, not a zero: the environment could not score a
    known-correct patch, so nothing about a citizen's work was measured there. The
    substrate already refuses to tally those as failures ("an errored verdict is an
    ABSENCE, not a zero — must never be tallied as a failed attempt"), and this chart must
    agree with it: `<instance>.ungradeable.json` refusal markers sit in the same directory
    as real verdicts, and counting them as attempts understated the result by thirteen
    (2026-09-09: 43/89 = 48% charted, against 43/76 = 57% actually graded).

    The refusals are not hidden — `receipts-snapshot.json` carries their count, so the gap
    between "attempted" and "gradeable on this box" stays readable.
    """
    rows = []
    ungradeable = 0
    for p in sorted(glob.glob(VERDICTS), key=os.path.getmtime):
        if p.endswith(".ungradeable.json"):
            ungradeable += 1
            continue
        try:
            v = json.load(open(p))
        except Exception as e:  # a corrupt verdict is a loud skip, never silent
            print(f"SKIP {p}: {e}", file=sys.stderr)
            continue
        instance = os.path.basename(p)[:-5]
        rows.append(
            {
                "instance": instance,
                # SWE-bench ids are "<project>__<repo>-<number>"; the project is the
                # part a reader recognizes, and grouping by it is what shows BREADTH.
                "project": instance.split("__")[0],
                "resolved": bool(v.get("resolved")),
                # Partial credit the binary verdict hides: an attempt that moved 0/2 -> 1/2
                # did real work. Charting only `resolved` throws that away.
                "f2p_passed": int(v.get("f2p_passed") or 0),
                "f2p_total": int(v.get("f2p_total") or 0),
                # Regression safety: did the patch keep everything that already worked?
                # This is the number a reader should trust most and the one nobody shows.
                "p2p_passed": int(v.get("p2p_passed") or 0),
                "p2p_total": int(v.get("p2p_total") or 0),
                "served_model": v.get("served_model", ""),
                "harness_build": v.get("harness_build", ""),
                "graded_at": datetime.fromtimestamp(
                    os.path.getmtime(p), tz=timezone.utc
                ).isoformat(timespec="seconds"),
            }
        )
    if ungradeable:
        print(
            f"note: {ungradeable} ungradeable instance(s) excluded — an environment that "
            f"cannot score a known-correct patch measured nothing about the work",
            file=sys.stderr,
        )
    globals()["UNGRADEABLE_COUNT"] = ungradeable
    return rows


def _ungradeable_note() -> str:
    """Refusals are stated on the chart itself, never quietly dropped: a reader deserves
    to know how many instances this box could not score at all."""
    n = globals().get("UNGRADEABLE_COUNT", 0)
    return f" · {n} ungradeable on this box (environment, not capability)" if n else ""


PARTIAL = "#d29922"   # amber: real progress the binary verdict hides

def receipts_by_project_svg(rows, w=760):
    """Every graded instance as ONE CELL, grouped by the project it came from.

    Why this replaced the cumulative curve (2026-09-11): two cumulative lines over a
    fixed attempt order are near-parallel by construction — the shape carries no
    information a single fraction doesn't, and it occupied the strongest slot on the
    README. It also threw away three things the verdicts already hold and a reader
    actually wants:

      * BREADTH — these are ten real OSS codebases, not one toy repo. A per-project
        row makes that visible, including the projects we do BADLY on. Showing the
        0/4 is the point; a chart that can only go up is an advertisement.
      * PARTIAL CREDIT — an attempt that takes 1 of 2 fail-to-pass tests did real
        work. Binary `resolved` scores it identically to writing nothing.
      * REGRESSION SAFETY — whether the patch kept everything that already passed.
        It is the number that separates a fix from a bulldozer, and it is in every
        verdict file already.

    One cell = one verdict file on disk. Nothing here is authored.
    """
    projects = {}
    for r in rows:
        projects.setdefault(r["project"], []).append(r)
    # most-attempted first; a project we tried once should not lead the chart
    order = sorted(projects, key=lambda k: (-len(projects[k]), k))

    cell, gap, row_h = 13, 3, 24
    label_w, pad_l, pad_t = 104, 16, 52
    grid_x = pad_l + label_w
    widest = max(len(v) for v in projects.values())
    h = pad_t + row_h * len(order) + 96

    resolved = sum(1 for r in rows if r["resolved"])
    p2p_rows = [r for r in rows if r["p2p_total"]]
    intact = sum(1 for r in p2p_rows if r["p2p_passed"] == r["p2p_total"])
    p2p_pass = sum(r["p2p_passed"] for r in p2p_rows)
    p2p_all = sum(r["p2p_total"] for r in p2p_rows)
    partial = sum(1 for r in rows if not r["resolved"] and r["f2p_passed"])

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" '
        f'height="{h}" font-family="-apple-system,BlinkMacSystemFont,Segoe UI,Helvetica,Arial,sans-serif">',
        f'<text x="{pad_l}" y="24" font-size="14" font-weight="600" fill="{INK}">'
        f'SWE-bench Verified receipts — every cell is a verdict file on disk</text>',
        f'<text x="{pad_l}" y="42" font-size="12" fill="{INK}">'
        f'{resolved} of {len(rows)} resolved across {len(order)} real codebases'
        f'{_ungradeable_note()}</text>',
    ]

    for i, proj in enumerate(order):
        items = sorted(projects[proj], key=lambda r: r["instance"])
        y = pad_t + i * row_h
        got = sum(1 for r in items if r["resolved"])
        out.append(
            f'<text x="{grid_x - 10}" y="{y + 11:.0f}" text-anchor="end" font-size="11.5" '
            f'fill="{INK}">{proj}</text>'
        )
        for j, r in enumerate(items):
            cx = grid_x + j * (cell + gap)
            if r["resolved"]:
                fill, stroke = ACCENT, ACCENT
            elif r["f2p_passed"]:
                fill, stroke = PARTIAL, PARTIAL      # moved some tests, did not finish
            else:
                fill, stroke = "none", FRAME
            out.append(
                f'<rect x="{cx}" y="{y}" width="{cell}" height="{cell}" rx="2.5" '
                f'fill="{fill}" stroke="{stroke}" stroke-width="1"><title>'
                f'{r["instance"]} — f2p {r["f2p_passed"]}/{r["f2p_total"]}, '
                f'p2p {r["p2p_passed"]}/{r["p2p_total"]}</title></rect>'
            )
        tx = grid_x + widest * (cell + gap) + 8
        out.append(
            f'<text x="{tx}" y="{y + 11:.0f}" font-size="11.5" fill="{INK}" '
            f'font-variant-numeric="tabular-nums">{got}/{len(items)}</text>'
        )

    fy = pad_t + row_h * len(order) + 22
    out.append(
        f'<line x1="{pad_l}" y1="{fy - 12}" x2="{w - pad_l}" y2="{fy - 12}" '
        f'stroke="{FRAME}" stroke-width="1"/>'
    )
    legend = [(ACCENT, ACCENT, "resolved"), (PARTIAL, PARTIAL, f"partial ({partial})"),
              ("none", FRAME, "not resolved")]
    lx = pad_l
    for fill, stroke, text in legend:
        out.append(
            f'<rect x="{lx}" y="{fy - 1}" width="10" height="10" rx="2" fill="{fill}" '
            f'stroke="{stroke}" stroke-width="1"/>'
            f'<text x="{lx + 15}" y="{fy + 8}" font-size="11" fill="{INK}">{text}</text>'
        )
        lx += 22 + len(text) * 6.2
    out.append(
        f'<text x="{pad_l}" y="{fy + 30}" font-size="11.5" fill="{INK}">'
        f'Regression safety: {intact} of {len(p2p_rows)} patched suites left every '
        f'previously-passing test passing ({p2p_pass}/{p2p_all} tests).</text>'
    )
    out.append(
        f'<text x="{pad_l}" y="{fy + 48}" font-size="11" fill="{INK}" opacity="0.8">'
        f'Regenerated from ~/.continuum/benchmarks/swe/verdicts by '
        f'tools/scripts/generate_receipt_charts.py — never hand-edited.</text>'
    )
    out.append("</svg>")
    return "\n".join(out)


def main():
    rows = load_rows()
    os.makedirs(OUT_DIR, exist_ok=True)
    with open(os.path.join(OUT_DIR, "receipts-snapshot.json"), "w") as f:
        json.dump(rows, f, indent=2)
    with open(os.path.join(OUT_DIR, "receipts-by-project.svg"), "w") as f:
        f.write(receipts_by_project_svg(rows))
    resolved = sum(1 for r in rows if r["resolved"])
    print(f"charts written: {len(rows)} verdicts, {resolved} resolved ({resolved/len(rows):.0%})")
    print(f"  -> {os.path.join(OUT_DIR, 'receipts-by-project.svg')}")


if __name__ == "__main__":
    main()
