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
    rows = []
    for p in sorted(glob.glob(VERDICTS), key=os.path.getmtime):
        try:
            v = json.load(open(p))
        except Exception as e:  # a corrupt verdict is a loud skip, never silent
            print(f"SKIP {p}: {e}", file=sys.stderr)
            continue
        rows.append(
            {
                "instance": os.path.basename(p)[:-5],
                "resolved": bool(v.get("resolved")),
                "served_model": v.get("served_model", ""),
                "harness_build": v.get("harness_build", ""),
                "graded_at": datetime.fromtimestamp(
                    os.path.getmtime(p), tz=timezone.utc
                ).isoformat(timespec="seconds"),
            }
        )
    return rows


def improvement_curve_svg(rows, w=720, h=360):
    pad_l, pad_r, pad_t, pad_b = 56, 16, 44, 40
    plot_w, plot_h = w - pad_l - pad_r, h - pad_t - pad_b
    n = len(rows)
    if n == 0:
        raise SystemExit("no verdicts on disk — nothing to chart")
    cum_attempts = list(range(1, n + 1))
    cum_resolved = []
    r = 0
    for row in rows:
        r += 1 if row["resolved"] else 0
        cum_resolved.append(r)
    y_max = n

    def x(i):
        return pad_l + plot_w * i / max(n - 1, 1)

    def y(v):
        return pad_t + plot_h * (1 - v / y_max)

    def polyline(vals, color, width):
        pts = " ".join(f"{x(i):.1f},{y(v):.1f}" for i, v in enumerate(vals))
        return (
            f'<polyline fill="none" stroke="{color}" stroke-width="{width}" '
            f'stroke-linejoin="round" stroke-linecap="round" points="{pts}"/>'
        )

    rate = cum_resolved[-1] / n
    gridlines = []
    for frac in (0.25, 0.5, 0.75, 1.0):
        gy = y(y_max * frac)
        gridlines.append(
            f'<line x1="{pad_l}" y1="{gy:.1f}" x2="{w - pad_r}" y2="{gy:.1f}" '
            f'stroke="{FRAME}" stroke-width="1" stroke-dasharray="3,5"/>'
            f'<text x="{pad_l - 8}" y="{gy + 4:.1f}" text-anchor="end" '
            f'font-size="12" fill="{INK}">{int(y_max * frac)}</text>'
        )

    end_x, end_y = x(n - 1), y(cum_resolved[-1])
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="ui-sans-serif, system-ui, sans-serif">
  <title>SWE-bench verdicts: cumulative attempts vs resolved (receipts-generated)</title>
  <text x="{pad_l}" y="24" font-size="16" font-weight="600" fill="{INK}">SWE-bench receipts — cumulative graded attempts vs resolved</text>
  <text x="{pad_l}" y="{h - 10}" font-size="11" fill="{MUTED}">generated from verdict artifacts; every point cites a JSON on disk · resolved {cum_resolved[-1]}/{n} ({rate:.0%})</text>
  {''.join(gridlines)}
  {polyline(cum_attempts, MUTED, 2)}
  {polyline(cum_resolved, ACCENT, 3)}
  <circle cx="{end_x:.1f}" cy="{end_y:.1f}" r="4.5" fill="{ACCENT}"/>
  <text x="{end_x - 6:.1f}" y="{end_y - 10:.1f}" text-anchor="end" font-size="13" font-weight="600" fill="{ACCENT}">{cum_resolved[-1]} resolved</text>
  <text x="{x(n - 1) - 6:.1f}" y="{y(cum_attempts[-1]) + 16:.1f}" text-anchor="end" font-size="12" fill="{MUTED}">{n} graded attempts</text>
</svg>
"""
    return svg


def main():
    rows = load_rows()
    os.makedirs(OUT_DIR, exist_ok=True)
    with open(os.path.join(OUT_DIR, "receipts-snapshot.json"), "w") as f:
        json.dump(rows, f, indent=2)
    with open(os.path.join(OUT_DIR, "improvement-curve.svg"), "w") as f:
        f.write(improvement_curve_svg(rows))
    resolved = sum(1 for r in rows if r["resolved"])
    print(f"charts written: {len(rows)} verdicts, {resolved} resolved ({resolved/len(rows):.0%})")
    print(f"  -> {os.path.join(OUT_DIR, 'improvement-curve.svg')}")


if __name__ == "__main__":
    main()
