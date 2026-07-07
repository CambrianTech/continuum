#!/usr/bin/env python3
"""
catalog.py — the benchmark CATALOG. Manage benchmark collections the way we manage the model
catalog and databases: LIST what's known, PULL (download + cache) the large ones on demand,
RESOLVE a name → a local dataset. Public and reproducible — anyone gets the same benchmark by
the same name, and (like models) big collections are fetched once and cached, not vendored.

Design mirrors the model catalog on purpose:
  • declarative registry of KNOWN benchmarks (name → source, grader, size) — one entry to add one;
  • a shared cache under ~/.continuum/benchmarks/ (like ~/.continuum model dirs);
  • fetch-on-demand + fail-loud (never a silent stub);
  • a `grader` per benchmark (how a solution is scored) so the matrix/opponent runner knows how
    to judge it — `rust` (compile+run, live today) or `python`/others as the catalog grows.

This is the management layer the reproducible matrix reads from. Later a Rust `benchmark/*`
command can wrap the SAME registry so a PERSONA can call a competition by name.

CLI:
  python3 benchmarks/catalog.py list
  python3 benchmarks/catalog.py pull humaneval
  python3 benchmarks/catalog.py resolve humaneval-rs
"""
import gzip
import io
import json
import os
import sys
import urllib.request

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.expanduser("~/.continuum/benchmarks")

# The known benchmark collections. `grader`: how a solution is scored. `kind`: where it lives.
#   local — vendored in-repo (small, canonical).
#   url   — fetched + cached on demand (may be .gz).
# Add a benchmark = add one entry. Add a respected collection (SWE-bench, LiveCodeBench, …) as a
# `url`/`hf` entry with its grader; big ones cache like a model, never bloat the repo.
KNOWN = {
    "humaneval-rs": {
        "desc": "HumanEval ported to Rust — 164 tasks, graded by rustc compile+run.",
        "kind": "local",
        "path": "docs/genome/humaneval-rs.jsonl",
        "grader": "rust",
        "tasks": 164,
    },
    "humaneval": {
        "desc": "OpenAI HumanEval (Python) — the original, 164 tasks. Grader: python exec.",
        "kind": "url",
        "url": "https://github.com/openai/human-eval/raw/master/data/HumanEval.jsonl.gz",
        "gz": True,
        "grader": "python",
        "tasks": 164,
    },
    "mbpp": {
        "desc": "MBPP (Mostly Basic Python Problems) — ~974 tasks. Grader: python exec.",
        "kind": "url",
        "url": "https://raw.githubusercontent.com/google-research/google-research/master/mbpp/mbpp.jsonl",
        "grader": "python",
        "tasks": 974,
    },
}


def _cache_path(name):
    return os.path.join(CACHE, f"{name}.jsonl")


def pull(name):
    """Fetch + cache a benchmark. Local ones resolve in-repo (nothing to fetch)."""
    if name not in KNOWN:
        raise SystemExit(f"unknown benchmark '{name}'. Known: {', '.join(sorted(KNOWN))}")
    spec = KNOWN[name]
    if spec["kind"] == "local":
        p = os.path.join(REPO, spec["path"])
        if not os.path.isfile(p):
            raise SystemExit(f"vendored benchmark '{name}' missing at {p}")
        return p
    # url
    dst = _cache_path(name)
    if os.path.isfile(dst) and os.path.getsize(dst) > 0:
        return dst
    os.makedirs(CACHE, exist_ok=True)
    print(f"pulling {name} from {spec['url']} …", file=sys.stderr)
    raw = urllib.request.urlopen(spec["url"], timeout=120).read()
    if spec.get("gz"):
        raw = gzip.GzipFile(fileobj=io.BytesIO(raw)).read()
    with open(dst, "wb") as f:
        f.write(raw)
    n = sum(1 for _ in open(dst))
    print(f"cached {name}: {n} lines → {dst}", file=sys.stderr)
    return dst


def resolve(name):
    """Name → local dataset path (pulling + caching if needed). The one call the matrix uses."""
    return pull(name)


def grader_of(name):
    return KNOWN.get(name, {}).get("grader", "unknown")


def list_():
    rows = ["| name | grader | tasks | cached | description |", "|---|---|---|---|---|"]
    for name, s in sorted(KNOWN.items()):
        if s["kind"] == "local":
            cached = "in-repo" if os.path.isfile(os.path.join(REPO, s["path"])) else "MISSING"
        else:
            cached = "yes" if os.path.isfile(_cache_path(name)) else "no (pull)"
        rows.append(f"| {name} | {s['grader']} | {s['tasks']} | {cached} | {s['desc']} |")
    return "\n".join(rows)


def main():
    if len(sys.argv) < 2:
        print("usage: catalog.py {list | pull <name> | resolve <name>}", file=sys.stderr)
        raise SystemExit(2)
    cmd = sys.argv[1]
    if cmd == "list":
        print(list_())
    elif cmd in ("pull", "resolve") and len(sys.argv) == 3:
        print(resolve(sys.argv[2]) if cmd == "resolve" else pull(sys.argv[2]))
    else:
        print("usage: catalog.py {list | pull <name> | resolve <name>}", file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    main()
