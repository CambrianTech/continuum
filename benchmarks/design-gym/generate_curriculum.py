#!/usr/bin/env python3
"""
design-gym curriculum generator — procedural, seeded, INFINITE, zero hand-authoring.

The mutation trick: we compose a CLEAN page from a seeded design system (harmonized
palette with verified WCAG contrast, modular spacing/type scales), then inject ONE
named defect whose inverse we know exactly — so every training pair is validated by
construction, no teacher inference needed. The grader line in the prompt is computed
from the actual defect (real contrast ratios, real px), so the model learns
metric→fix, exactly the way the coder gene learns compiler-error→fix
(the metric IS the compiler).

Output: ShareGPT/mlx `{"messages":[...]}` JSONL (the same shape genome/teach emits
and `genome/job-create` consumes) + an eval split. Infinite via --seed/--count.

Prior art this composes with (fetched, not vendored): Aalto Interface Metrics for
harder pixel-level graders, WebSight for real-page mutation at scale. v0 keeps the
defect families that are pure computation — no browser needed to GENERATE, the
benchmark side (`benchmarks/project` website tier) is where launch+capture grade.

    python3 generate_curriculum.py --count 200 --seed 7 --out design-basics.jsonl
"""
import argparse
import colorsys
import json
import random


# ───────────────────────── WCAG contrast (the formula IS the grader) ─────────

def _chan(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

def luminance(hexcol):
    h = hexcol.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return 0.2126 * _chan(r) + 0.7152 * _chan(g) + 0.0722 * _chan(b)

def contrast(a, b):
    la, lb = luminance(a), luminance(b)
    hi, lo = max(la, lb), min(la, lb)
    return (hi + 0.05) / (lo + 0.05)

def hsl_hex(h, s, l):
    r, g, b = colorsys.hls_to_rgb((h % 360) / 360.0, l, s)
    return "#{:02x}{:02x}{:02x}".format(int(r * 255), int(g * 255), int(b * 255))


# ───────────────────────── seeded clean-page composer ────────────────────────

FIELDS = ["bakery", "florist", "bike shop", "coffee roaster", "bookstore", "climbing gym",
          "pottery studio", "juice bar", "record store", "tailor", "apiary", "arcade"]
NAMES = ["Harbor", "Cedar", "Northside", "Golden", "Willow", "Summit", "Copper",
         "Lantern", "Meridian", "Bluebird", "Foxglove", "Anchor"]

def design_system(rng):
    hue = rng.randrange(0, 360)
    accent_hue = (hue + rng.choice([30, 150, 180, 210])) % 360
    light = rng.random() < 0.7
    bg = hsl_hex(hue, 0.15, 0.96) if light else hsl_hex(hue, 0.25, 0.12)
    # walk lightness until body text verifiably clears 4.5:1 on bg
    l = 0.25 if light else 0.85
    text = hsl_hex(hue, 0.10, l)
    step = -0.02 if light else 0.02
    while contrast(text, bg) < 4.6 and 0.02 < l < 0.98:
        l += step
        text = hsl_hex(hue, 0.10, l)
    accent = hsl_hex(accent_hue, 0.55, 0.42 if light else 0.62)
    space = rng.choice([6, 8])          # base spacing unit
    ratio = rng.choice([1.25, 1.333])   # modular type scale
    base_font = rng.choice([16, 17, 18])
    return {"hue": hue, "bg": bg, "text": text, "accent": accent,
            "space": space, "ratio": ratio, "base_font": base_font,
            "name": f"{rng.choice(NAMES)} {rng.choice(FIELDS).title()}"}

def compose_css(d):
    s, r, f = d["space"], d["ratio"], d["base_font"]
    return f"""* {{ box-sizing: border-box; margin: 0; }}
:root {{
  --bg: {d['bg']};
  --text: {d['text']};
  --accent: {d['accent']};
  --space-1: {s}px;
  --space-2: {s * 2}px;
  --space-3: {s * 4}px;
}}
body {{
  background: var(--bg);
  color: var(--text);
  font-family: system-ui, sans-serif;
  font-size: {f}px;
  line-height: 1.6;
}}
header {{
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--space-2) var(--space-3);
}}
nav a {{
  color: var(--text);
  margin-left: var(--space-2);
  text-decoration: none;
}}
nav a:hover {{ color: var(--accent); }}
.hero {{
  padding: var(--space-3);
  text-align: center;
}}
.hero h1 {{ font-size: {round(f * r * r)}px; }}
.hero p {{ font-size: {round(f * r)}px; max-width: 60ch; margin: var(--space-2) auto; }}
.cards {{
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: var(--space-2);
  padding: var(--space-3);
}}
.card {{
  background: color-mix(in srgb, var(--bg) 85%, var(--accent));
  border-radius: 8px;
  padding: var(--space-2);
}}
footer {{
  padding: var(--space-2) var(--space-3);
  opacity: 0.8;
}}
"""

def compose_html(d):
    n = d["name"]
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{n}</title>
<link rel="stylesheet" href="styles.css">
</head>
<body>
<header><strong>{n}</strong><nav><a href="index.html">Home</a><a href="about.html">About</a><a href="contact.html">Contact</a></nav></header>
<section class="hero"><h1>{n}</h1><p>Neighborhood {n.split()[-1].lower()} — open daily, made with care.</p></section>
<section class="cards"><div class="card"><h2>What we do</h2><p>Quality first.</p></div><div class="card"><h2>Visit</h2><p>12 Main St.</p></div><div class="card"><h2>Hours</h2><p>7am–6pm.</p></div></section>
<footer><small>© {n}</small></footer>
</body>
</html>
"""


# ───────────────────────── named mutations (defect + computed feedback) ──────
# Each returns (mutated_css, grader_line). The FIX is the original css — known by
# construction. Feedback quotes REAL computed numbers, so metric→fix is learnable.

def m_contrast_kill(css, d, rng):
    bad_l = 0.62 if luminance(d["bg"]) > 0.5 else 0.35
    bad = hsl_hex(d["hue"], 0.08, bad_l)
    ratio = contrast(bad, d["bg"])
    return css.replace(f"--text: {d['text']};", f"--text: {bad};"), (
        f"[contrast] body text {bad} on background {d['bg']} measures {ratio:.1f}:1 — "
        f"WCAG requires >= 4.5:1 for body text. Restore a text color in the same hue "
        f"family that clears 4.5:1.")

def m_fontsize_floor(css, d, rng):
    tiny = rng.choice([9, 10, 11])
    return css.replace(f"font-size: {d['base_font']}px;", f"font-size: {tiny}px;"), (
        f"[type] body font-size is {tiny}px — below the 16px readability floor for "
        f"body text. Restore a base size >= 16px.")

def m_overflow_force(css, d, rng):
    w = rng.choice([1400, 1600, 1900])
    return css.replace(".card {", f".card {{\n  width: {w}px;"), (
        f"[overflow] .card has a fixed width of {w}px inside a responsive grid — the "
        f"page scrolls horizontally on every viewport narrower than {w}px. Remove the "
        f"fixed width and let the grid size the cards.")

def m_misalign(css, d, rng):
    off = rng.choice([13, 17, 23, 31])
    return css.replace(".hero {", f".hero {{\n  margin-left: {off}px;"), (
        f"[alignment] .hero is offset {off}px from the page's left alignment line — "
        f"every other section sits on the shared edge. Remove the stray offset so "
        f"sections align.")

def m_palette_clash(css, d, rng):
    clash = hsl_hex((d["hue"] + rng.choice([72, 96, 264])) % 360, 0.85, 0.5)
    return css.replace(f"--accent: {d['accent']};", f"--accent: {clash};"), (
        f"[palette] the accent {clash} sits outside the page's color harmony (base hue "
        f"{d['hue']}°, saturation ~55%) — it reads as a clash at 85% saturation. Bring "
        f"the accent back into harmony with the base hue (a 30°/150°/180°/210° "
        f"relationship at moderate saturation).")

def m_lineheight_crush(css, d, rng):
    lh = rng.choice([0.9, 1.0, 1.05])
    return css.replace("line-height: 1.6;", f"line-height: {lh};"), (
        f"[type] body line-height is {lh} — lines of body text collide; readable body "
        f"copy needs ~1.4-1.7. Restore a line-height in that range.")

MUTATIONS = [m_contrast_kill, m_fontsize_floor, m_overflow_force,
             m_misalign, m_palette_clash, m_lineheight_crush]


# ───────────────────────── pair emission ─────────────────────────────────────

def make_pair(rng):
    d = design_system(rng)
    css = compose_css(d)
    html = compose_html(d)
    mutated, grader = rng.choice(MUTATIONS)(css, d, rng)
    if mutated == css:  # substitution missed → composer/mutation drift; fail loud
        raise RuntimeError("mutation was a no-op — composer and mutation drifted apart")
    user = (
        "You are fixing the visual design of a website. Here are the files.\n\n"
        f"index.html:\n```html\n{html}```\n\nstyles.css:\n```css\n{mutated}```\n\n"
        f"The design grader reports exactly one defect:\n{grader}\n\n"
        "Return the corrected styles.css (the complete file, nothing else) that fixes "
        "the reported defect while preserving the rest of the design.")
    assistant = f"```css\n{css}```"
    return {"messages": [{"role": "user", "content": user},
                         {"role": "assistant", "content": assistant}]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--out", default="design-basics.jsonl")
    ap.add_argument("--eval-out", default=None,
                    help="held-out eval split path (defaults to <out>.eval.jsonl)")
    ap.add_argument("--eval-frac", type=float, default=0.1)
    ap.add_argument("--install", default=None, metavar="DATASET_NAME",
                    help="install as ~/.continuum/datasets/<name>/ (train.jsonl + "
                         "eval.jsonl + manifest.json — the shape genome/job-create "
                         "consumes via datasetName). The mechanical path; never cp by hand.")
    args = ap.parse_args()

    rng = random.Random(args.seed)
    rows = [make_pair(rng) for _ in range(args.count)]
    n_eval = max(1, int(len(rows) * args.eval_frac))
    eval_rows, train_rows = rows[:n_eval], rows[n_eval:]

    if args.install:
        import datetime
        import os
        root = os.path.expanduser(f"~/.continuum/datasets/{args.install}")
        os.makedirs(root, exist_ok=True)
        with open(os.path.join(root, "train.jsonl"), "w") as f:
            for r in train_rows:
                f.write(json.dumps(r) + "\n")
        with open(os.path.join(root, "eval.jsonl"), "w") as f:
            for r in eval_rows:
                f.write(json.dumps(r) + "\n")
        manifest = {"name": args.install, "version": "1.0",
                    "total_examples": len(rows), "train_examples": len(train_rows),
                    "eval_examples": len(eval_rows), "train_path": "train.jsonl",
                    "eval_path": "eval.jsonl",
                    "imported_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "generator": {"script": "benchmarks/design-gym/generate_curriculum.py",
                                  "seed": args.seed, "count": args.count}}
        with open(os.path.join(root, "manifest.json"), "w") as f:
            json.dump(manifest, f, indent=1)
        print(f"installed dataset '{args.install}' -> {root} "
              f"(train={len(train_rows)}, eval={len(eval_rows)})")
    else:
        eval_path = args.eval_out or f"{args.out}.eval.jsonl"
        with open(args.out, "w") as f:
            for r in train_rows:
                f.write(json.dumps(r) + "\n")
        with open(eval_path, "w") as f:
            for r in eval_rows:
                f.write(json.dumps(r) + "\n")
        print(f"train={len(train_rows)} -> {args.out}   eval={len(eval_rows)} -> {eval_path}")

    by = {}
    for r in rows:
        tag = r["messages"][0]["content"].split("[")[1].split("]")[0]
        by[tag] = by.get(tag, 0) + 1
    print("defect mix:", json.dumps(by))


if __name__ == "__main__":
    main()
