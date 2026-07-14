#!/usr/bin/env python3
"""
resolve_fleet.py — make the opponent fleet PORTABLE.

The kicking machine must reproduce on a stranger's laptop, not just ours. The
committed fleet config names each opponent by its authoritative Hugging Face
source (`gguf_repo` + `gguf_file`); this resolves each to a concrete local path,
downloading it into the caller's own HF cache if absent. No absolute paths from
one operator's machine leak into the run.

Reads a fleet json (list of rows). For each row, in order of preference:
  1. `gguf` is set AND exists on disk           -> use it verbatim (fast path / offline)
  2. `gguf_repo` + `gguf_file` set               -> hf_hub_download into the HF cache
  3. neither resolvable                          -> row is dropped with a loud note

Writes the RESOLVED fleet (every kept row has a real local `gguf`) to --out, and
prints a one-line status per model to stderr. Exit non-zero only if ZERO models
resolved (nothing to run) — a single missing opponent is a note, not a wall.

Usage:
  python3 resolve_fleet.py --in models-fleet.json --out /tmp/fleet-resolved.json
"""
import argparse, json, os, sys


def resolve_one(row):
    """Return a benchmarkable row, or None if unresolvable.

    A row is benchmarkable if it can be REACHED, which is EITHER:
      - an already-serving endpoint (`raw_endpoint`) — nothing to download/serve; OR
      - a servable GGUF (local path, or an HF `gguf_repo`+`gguf_file` to fetch).
    """
    label = row.get("label", "?")
    # (0) already served (e.g. the live core's own lane): no GGUF needed. The sweep
    # reuses the endpoint for RAW and stands up its OWN ephemeral lane for OURS via
    # base_model_id — dropping it would silently exclude our flagship from the board.
    if row.get("raw_endpoint") and not (row.get("gguf") or row.get("gguf_repo")):
        print(f"  [served]   {label}: {row['raw_endpoint']} (already up — no local serve)",
              file=sys.stderr)
        return dict(row)
    # (1) an existing absolute path wins — offline / same-machine fast path.
    gguf = row.get("gguf")
    if gguf and os.path.exists(gguf):
        print(f"  [have]     {label}: {gguf}", file=sys.stderr)
        return {**row, "gguf": gguf}
    # (2) portable: pull the exact quant from its authoritative HF repo.
    repo, fname = row.get("gguf_repo"), row.get("gguf_file")
    if repo and fname:
        try:
            from huggingface_hub import hf_hub_download
        except ImportError:
            print("  [error]    huggingface_hub not installed — `pip install huggingface_hub`",
                  file=sys.stderr)
            return None
        try:
            print(f"  [resolve]  {label}: {repo}/{fname} (downloading if absent)…", file=sys.stderr)
            local = hf_hub_download(repo_id=repo, filename=fname)
            print(f"  [ok]       {label}: {local}", file=sys.stderr)
            return {**row, "gguf": local}
        except Exception as e:  # network / gated / typo — a note, never a crash.
            print(f"  [skip]     {label}: could not fetch {repo}/{fname}: {e}", file=sys.stderr)
            return None
    print(f"  [skip]     {label}: no local `gguf` and no `gguf_repo`+`gguf_file` to resolve",
          file=sys.stderr)
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--in", dest="inp", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    rows = json.load(open(args.inp))
    print(f"resolving {len(rows)} opponent(s) from {args.inp}:", file=sys.stderr)
    resolved = [r for r in (resolve_one(row) for row in rows) if r]

    json.dump(resolved, open(args.out, "w"), indent=2)
    print(f"resolved {len(resolved)}/{len(rows)} → {args.out}", file=sys.stderr)
    if not resolved:
        print("NO opponents resolved — nothing to benchmark. Fix gguf_repo/gguf_file.",
              file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
