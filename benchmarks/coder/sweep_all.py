#!/usr/bin/env python3
"""
sweep_all.py — the MANY-MODELS benchmark orchestrator.

The definitive proof is not one model — it's the SAME claim holding across a fleet:
for each local model, our loop (OURS) beats the standard local agentic harness
(opencode) on identical weights, and stands next to competing local models (Hermes).

64GB can't hold every model at once, so this walks the fleet SEQUENTIALLY:

  for each model:
    1. serve its GGUF on a scratch llama-server (unless already served)
    2. point opencode's shim at that endpoint
    3. run RAW (one-shot) + OURS (Continuum ephemeral lane) + opencode (harness)
       via matrix.py — which appends every cell to the committed RESULTS.jsonl
    4. tear the scratch server down before the next model loads

Each model's three arms grade the SAME tasks with the SAME rustc compile+run grader,
so every delta is an honest system effect, not a model-fit confound. Nothing here can
be gamed by prose.

Config row (benchmarks/coder/models-fleet.json):
  {
    "label": "Qwen2.5-Coder-3B",
    "gguf": "/abs/path/....gguf",        # served on a scratch port for RAW+opencode
    "alias": "Qwen2.5-Coder-3B-Instruct-Q4_K_M.gguf",  # name RAW/opencode expect
    "base_model_id": "continuum-ai/qwen2.5-coder-3b-instruct-GGUF",  # OURS ephemeral lane
    "serve_port": 8091,                  # scratch port (reused per model; only one at a time)
    "raw_endpoint": "http://127.0.0.1:8093/v1"  # OPTIONAL: already-served → skip serve/teardown
  }

Usage:
  python3 benchmarks/coder/sweep_all.py --models benchmarks/coder/models-fleet.json \
      --benchmark humaneval-rs --limit 40 [--wait-pid 56248]
"""
import argparse, json, os, subprocess, sys, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
MATRIX = os.path.join(HERE, "matrix.py")
LLAMA_SERVER = os.path.expanduser("~/.continuum/bin/llama-server")
OPENCODE_CFG = os.path.expanduser("~/.config/opencode/opencode.json")


def _slug(s):
    return "".join(c if c.isalnum() else "-" for c in s.lower())


def endpoint_up(url, model=None):
    try:
        with urllib.request.urlopen(url.rstrip("/") + "/models", timeout=4) as r:
            data = json.loads(r.read())
            ids = [m["id"] for m in data.get("data", [])]
            return (model in ids) if model else bool(ids)
    except Exception:
        return False


def serve(gguf, alias, port, ctx=16384):
    """Start a scratch llama-server; return the Popen once /v1/models answers."""
    url = f"http://127.0.0.1:{port}/v1"
    if endpoint_up(url):
        raise SystemExit(f"[sweep] port {port} already serving — refusing to double-bind")
    log = open(f"/tmp/sweep-serve-{port}.log", "w")
    proc = subprocess.Popen(
        [LLAMA_SERVER, "-m", gguf, "--alias", alias, "--host", "127.0.0.1",
         "--port", str(port), "-c", str(ctx), "--parallel", "2", "--cache-reuse", "256", "--jinja"],
        stdout=log, stderr=subprocess.STDOUT)
    for _ in range(180):  # up to 3 min to load
        if endpoint_up(url):
            return proc
        if proc.poll() is not None:
            raise RuntimeError(f"llama-server for {alias} died on startup (see /tmp/sweep-serve-{port}.log)")
        time.sleep(1)
    proc.terminate()
    raise RuntimeError(f"llama-server for {alias} never became ready on :{port}")


def point_opencode(port):
    """Repoint opencode's local shim baseURL at the given port (for the opencode arm)."""
    d = json.load(open(OPENCODE_CFG))
    d["provider"]["local"]["options"]["baseURL"] = f"http://127.0.0.1:{port}/v1"
    json.dump(d, open(OPENCODE_CFG, "w"), indent=2)


def run_model(row, args):
    label = row["label"]
    served_here = None
    # 1. ensure an endpoint for RAW + opencode
    if row.get("raw_endpoint") and endpoint_up(row["raw_endpoint"], row.get("alias")):
        endpoint = row["raw_endpoint"]
        port = int(endpoint.rsplit(":", 1)[1].split("/")[0])
        print(f"[sweep] {label}: reusing already-served {endpoint}", file=sys.stderr)
    else:
        port = row["serve_port"]
        print(f"[sweep] {label}: serving {row['alias']} on :{port} …", file=sys.stderr)
        served_here = serve(row["gguf"], row["alias"], port)
        endpoint = f"http://127.0.0.1:{port}/v1"
    try:
        point_opencode(port)
        # 2. one-row config for matrix.py: RAW(endpoint) + OURS(ephemeral) + opencode(harness)
        cfg = [{
            "label": label,
            "base_model_id": row.get("base_model_id"),
            "raw_endpoint": endpoint,
            "raw_model": row["alias"],
            "opponent": "opencode",
            "opencode_model": "local/qwen14b",  # opencode's fixed model name; baseURL is what varies
        }]
        tmp = f"/tmp/sweep-row-{_slug(label)}.json"
        json.dump(cfg, open(tmp, "w"))
        cmd = [sys.executable, MATRIX, "--models", tmp, "--benchmark", args.benchmark,
               "--limit", str(args.limit), "--cu", args.cu,
               "--out", f"/tmp/matrix-{_slug(label)}.md"]
        print(f"[sweep] {label}: RAW+OURS+opencode × {args.limit} tasks …", file=sys.stderr)
        subprocess.run(cmd)  # appends every cell to RESULTS.jsonl itself
    finally:
        if served_here is not None:
            served_here.terminate()
            try:
                served_here.wait(timeout=20)
            except Exception:
                served_here.kill()
            print(f"[sweep] {label}: scratch server on :{port} torn down", file=sys.stderr)


def main():
    ap = argparse.ArgumentParser(description="Sequential serve→sweep→teardown across a model fleet.")
    ap.add_argument("--models", required=True)
    ap.add_argument("--benchmark", default="humaneval-rs")
    ap.add_argument("--limit", type=int, default=40)
    ap.add_argument("--cu", default=os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu"))
    ap.add_argument("--wait-pid", type=int, default=None,
                    help="idle-wait for this pid (a prior sweep) to exit before starting")
    args = ap.parse_args()

    if args.wait_pid:
        print(f"[sweep] waiting for pid {args.wait_pid} to finish before starting…", file=sys.stderr)
        while True:
            try:
                os.kill(args.wait_pid, 0)
            except OSError:
                break
            time.sleep(30)
        print(f"[sweep] pid {args.wait_pid} done — starting fleet sweep", file=sys.stderr)

    rows = json.load(open(args.models))
    for row in rows:
        try:
            run_model(row, args)
        except Exception as e:
            # one bad model must never kill the fleet — record + continue (matrix cells self-flag EXCL)
            print(f"[sweep] {row['label']}: FAILED ({e}) — continuing to next model", file=sys.stderr)
    print("[sweep] fleet sweep complete — re-render with benchmarks/render_results.py", file=sys.stderr)


if __name__ == "__main__":
    main()
