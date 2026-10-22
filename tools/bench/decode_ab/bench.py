"""Decode under a concurrent prefill, per batch size, at the residents' measured act sizes.
Usage: python3 -I bench.py derived.json <ubatch> <out.jsonl>   (serving stopped; derive.py ran while it was up)
Nothing is hard-coded: engine argv, context depth and output length all come from derived.json."""
import glob, json, os, subprocess, sys, threading, time, urllib.request, uuid

D = json.load(open(sys.argv[1])); UB = sys.argv[2]; OUT = sys.argv[3]

def free_port():
    import socket
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0)); return s.getsockname()[1]
PORT = free_port()

def argv_for(ubatch):
    a, out, skip = D["argv"], [], 0
    for i, x in enumerate(a):
        if skip: skip -= 1; continue
        if x in ("--port", "--ubatch-size", "-ub", "--batch-size", "-b"): skip = 1; continue
        out.append(x)
    return out + ["--port", str(PORT), "--batch-size", ubatch, "--ubatch-size", ubatch]

def call(path, body, timeout=1800):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=timeout))

def corpus():
    parts = []
    for p in sorted(glob.glob(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../core/continuum-core/src/**/*.rs"), recursive=True)):
        parts.append(open(p, errors="ignore").read())
        if sum(map(len, parts)) > D["ctx_tokens"] * 6: break
    return "".join(parts)

def prompt_of(tokens, seed):
    # Exactly `tokens` tokens of real source, cut by the server's own tokenizer, unique per seed.
    ids = call("/tokenize", {"content": f"[{seed}]\n" + corpus()})["tokens"][:tokens]
    return call("/detokenize", {"tokens": ids})["content"]

srv = subprocess.Popen(argv_for(UB), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    for _ in range(900):
        try:
            if json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2)).get("status") == "ok": break
        except Exception: pass
        time.sleep(1)
    ctx, out_n = D["ctx_tokens"], D["out_tokens"]
    decode_prompt = prompt_of(ctx, uuid.uuid4())
    dec = lambda: {"prompt": decode_prompt, "n_predict": out_n, "cache_prompt": True, "temperature": 0.7, "ignore_eos": True, "id_slot": 0}
    pre = lambda: {"prompt": prompt_of(ctx, uuid.uuid4()), "n_predict": 1, "cache_prompt": False, "id_slot": 1}
    r = {"ubatch": int(UB), "ctx_tokens": ctx, "out_tokens": out_n}
    t = call("/completion", dec())["timings"]; r["cold_prefill_tps"] = t["prompt_per_second"]  # fills slot 0 at depth
    t = call("/completion", dec())["timings"]; r["solo_decode_tps"] = t["predicted_per_second"]; r["solo_decode_uncached_n"] = t["prompt_n"]
    res = {}
    th = threading.Thread(target=lambda: res.__setitem__("a", call("/completion", dec())["timings"]))
    body = pre()
    th.start(); time.sleep(1)
    tb = call("/completion", body)["timings"]; th.join()
    r["contended_decode_tps"] = res["a"]["predicted_per_second"]; r["contended_prefill_tps"] = tb["prompt_per_second"]
    r["at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    print(json.dumps(r)); open(OUT, "a").write(json.dumps(r) + "\n")
finally:
    srv.terminate()
    try: srv.wait(60)
    except Exception: srv.kill()
