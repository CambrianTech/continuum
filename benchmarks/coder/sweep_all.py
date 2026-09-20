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
import argparse, json, os, shutil, struct, subprocess, sys, time, urllib.request

def _resolve_cli():
    """Locate the continuum CLI.

    `uu` is THE official short alias (the double-U of contin-UU-m). `cu` is
    /usr/bin/cu (UUCP) on every Unix and was never ours — a default pointing at a
    `cu` binary resolved to a file that does not exist, so the harness failed at
    the first invocation instead of running. Prefer what is actually installed on
    PATH; fall back to the release build.
    """
    for name in ("uu", "continuum"):
        found = shutil.which(name)
        if found:
            return found
    return os.path.expanduser("~/.continuum/cache/cargo-target/release/continuum")


HERE = os.path.dirname(os.path.abspath(__file__))
MATRIX = os.path.join(HERE, "matrix.py")
LLAMA_SERVER = os.path.expanduser("~/.continuum/bin/llama-server")
OPENCODE_CFG = os.path.expanduser("~/.config/opencode/opencode.json")

# Memory-safe ceiling on served context. We serve each model at min(its REAL trained context,
# this cap) — NEVER a hardcoded-down number (that would handicap the opponent arm AND misreport
# the comparison; the core's OURS lane already serves at the model's real capability via
# serving_plan's fit_ctx.min(model.context_window)). 65536 is chosen so any model that natively
# supports ≥64K clears the Hermes CLI's hard 64K floor without over-allocating KV on a 128K model.
CTX_CAP = 65536
# Hermes CLI refuses any model whose per-slot runtime context is below this. A model whose REAL
# trained context is under it cannot be run through Hermes honestly (overflowing rope-scale to
# fake 64K degrades the model) — so its Hermes cell is an honest N/A, not a fake 0.
HERMES_MIN_CTX = 64000


def gguf_n_ctx_train(path):
    """Read a GGUF model's REAL trained context length from its metadata header (fast — reads
    only the KV header, not the tensor data). Returns the int, or None if not found. This is the
    single source of truth for 'how much context does this model actually support' — we never
    hardcode it."""
    T = {0: "B", 1: "b", 2: "H", 3: "h", 4: "I", 5: "i", 6: "f",
         7: "?", 10: "Q", 11: "q", 12: "d"}  # gguf value-type → struct fmt (scalars)
    SZ = {k: struct.calcsize(v) for k, v in T.items()}
    try:
        with open(path, "rb") as fh:
            if fh.read(4) != b"GGUF":
                return None
            struct.unpack("<I", fh.read(4))            # version
            fh.read(8)                                 # tensor_count (uint64)
            n_kv = struct.unpack("<Q", fh.read(8))[0]  # metadata_kv_count

            def rstr():
                n = struct.unpack("<Q", fh.read(8))[0]
                return fh.read(n).decode("utf-8", "replace")

            def skip_val(vt):
                if vt == 8:            # string
                    rstr()
                elif vt == 9:          # array: elem_type, count, elements
                    et = struct.unpack("<I", fh.read(4))[0]
                    cnt = struct.unpack("<Q", fh.read(8))[0]
                    for _ in range(cnt):
                        skip_val(et)
                else:
                    fh.read(SZ[vt])

            for _ in range(n_kv):
                key = rstr()
                vt = struct.unpack("<I", fh.read(4))[0]
                if key.endswith(".context_length"):
                    if vt in (4, 10, 5, 11):           # uint32/uint64/int32/int64
                        return struct.unpack("<" + T[vt], fh.read(SZ[vt]))[0]
                    skip_val(vt)
                    return None
                skip_val(vt)
    except Exception as e:
        print(f"[sweep] gguf_n_ctx_train({os.path.basename(path)}) failed: {e}", file=sys.stderr)
    return None


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


def serve(gguf, alias, port, ctx):
    """Start a scratch llama-server at `ctx` context; return the Popen once /v1/models answers.

    `ctx` is the model's REAL trained context (min'd with the memory cap) — computed by the
    caller from GGUF metadata, NEVER a hardcoded-down constant (that would handicap the opponent
    arm and misreport the comparison). `--parallel 1` so the whole `ctx` is ONE slot: parallel 2
    would halve the per-slot window (and drop a 64K model under the Hermes floor)."""
    url = f"http://127.0.0.1:{port}/v1"
    if endpoint_up(url):
        raise SystemExit(f"[sweep] port {port} already serving — refusing to double-bind")
    log = open(f"/tmp/sweep-serve-{port}.log", "w")
    proc = subprocess.Popen(
        [LLAMA_SERVER, "-m", gguf, "--alias", alias, "--host", "127.0.0.1",
         "--port", str(port), "-c", str(ctx), "--parallel", "1", "--cache-reuse", "256", "--jinja"],
        stdout=log, stderr=subprocess.STDOUT)
    for _ in range(180):  # up to 3 min to load
        if endpoint_up(url):
            return proc
        if proc.poll() is not None:
            raise RuntimeError(f"llama-server for {alias} died on startup (see /tmp/sweep-serve-{port}.log)")
        time.sleep(1)
    proc.terminate()
    raise RuntimeError(f"llama-server for {alias} never became ready on :{port}")


def point_harnesses(port, alias):
    """Repoint BOTH opponent CLIs at the served model on `port`, so each drives identical weights.
    opencode via its shim baseURL; Hermes via `hermes config set` (provider custom + base_url +
    default model + the 64K context override it demands)."""
    url = f"http://127.0.0.1:{port}/v1"
    # opencode — persistent shim baseURL
    d = json.load(open(OPENCODE_CFG))
    d["provider"]["local"]["options"]["baseURL"] = url
    json.dump(d, open(OPENCODE_CFG, "w"), indent=2)
    # hermes — persistent config
    for k, v in [("model.provider", "custom"), ("model.base_url", url),
                 ("model.default", alias),
                 ("model.context_length", "65536"), ("model.ollama_num_ctx", "65536")]:
        subprocess.run(["hermes", "config", "set", k, v], capture_output=True, text=True)
    # aider — takes its endpoint per-invocation; export it so the (subprocess) matrix + aider
    # harness inherit OPENAI_API_BASE without any persistent config file.
    os.environ["OPENAI_API_BASE"] = url
    os.environ["OPENAI_API_KEY"] = "sk-none"


def run_model(row, args):
    label = row["label"]
    served_here = None
    # The model NAME the RAW arm + harnesses address. Two legal shapes:
    #   - a locally-served row names it with `alias` (what `serve --alias` publishes); OR
    #   - a reuse-an-already-served-endpoint row (our flagship on :58057) names it with
    #     `raw_model` and has NO alias (nothing is served here to name).
    # Resolve once, fail loud if neither — never hard-index one shape and KeyError the other.
    model_name = row.get("alias") or row.get("raw_model")
    if not model_name:
        raise RuntimeError(
            f"{label}: fleet row has neither `alias` (local serve) nor `raw_model` "
            f"(reused endpoint) — cannot name the model for RAW/harness arms")
    # The model's REAL trained context (GGUF metadata) — the single source of truth for how
    # much context it supports. We serve at min(real, cap) — never a hardcoded-down number.
    real_ctx = gguf_n_ctx_train(row["gguf"]) if row.get("gguf") else None
    serve_ctx = min(real_ctx or CTX_CAP, CTX_CAP)
    # Hermes CLI refuses models below its 64K floor; a 32K-native model (e.g. Qwen2.5-Coder)
    # simply cannot be run through Hermes honestly, so its Hermes cell is an honest absence,
    # not a fake 0 or a degrading rope-overflow to fake 64K.
    hermes_ok = (real_ctx or 0) >= HERMES_MIN_CTX
    # opencode + aider run on any context; hermes only clears its own 64K floor.
    opponents = ["opencode", "aider"] + (["hermes"] if hermes_ok else [])
    if not hermes_ok:
        print(f"[sweep] {label}: Hermes SKIPPED — model is {real_ctx or '?'}-ctx, below Hermes's "
              f"{HERMES_MIN_CTX} floor (honest N/A, not a 0)", file=sys.stderr)
    # 1. ensure an endpoint for RAW + opencode
    if row.get("raw_endpoint") and endpoint_up(row["raw_endpoint"], model_name):
        endpoint = row["raw_endpoint"]
        port = int(endpoint.rsplit(":", 1)[1].split("/")[0])
        print(f"[sweep] {label}: reusing already-served {endpoint}", file=sys.stderr)
    else:
        port = row["serve_port"]
        print(f"[sweep] {label}: serving {model_name} on :{port} at ctx={serve_ctx} "
              f"(real n_ctx_train={real_ctx}) …", file=sys.stderr)
        served_here = serve(row["gguf"], model_name, port, serve_ctx)
        endpoint = f"http://127.0.0.1:{port}/v1"
    try:
        point_harnesses(port, model_name)
        # 2. one-row config for matrix.py: RAW(endpoint) + OURS(ephemeral) + opencode [+ hermes]
        cfg = [{
            "label": label,
            "base_model_id": row.get("base_model_id"),
            "raw_endpoint": endpoint,
            "raw_model": model_name,
            "opponents": opponents,
            "opencode_model": "local/qwen14b",  # opencode's fixed shim name; baseURL is what varies
        }]
        tmp = f"/tmp/sweep-row-{_slug(label)}.json"
        json.dump(cfg, open(tmp, "w"))
        cmd = [sys.executable, MATRIX, "--models", tmp, "--benchmark", args.benchmark,
               "--limit", str(args.limit), "--uu", args.uu,
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
    ap.add_argument("--uu", default=_resolve_cli())
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
