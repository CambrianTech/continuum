"""Everything the A/B uses, read from the live system (never hard-coded): the engine's own argv,
and the residents' measured act sizes from the core's delib.generate.cache probes."""
import json, os, re, statistics, subprocess, sys

def engine_argv():
    out = subprocess.run(["ps", "-axo", "command"], capture_output=True, text=True).stdout
    # Whichever engine slot serves (engine-b, engine-c, ...): the running lane's own argv.
    lines = [l for l in out.splitlines() if re.search(r"/engine-[a-z]+/llama-server(\s|$)", l)]
    if not lines:
        sys.exit("no running engine to read arguments from: run this while serving is up, then stop serving")
    return lines[0].split()

def probe_rows(probes=os.path.expanduser("~/.continuum/probes/continuum-probes.jsonl")):
    # The probe sink (JSONL, rotated .1..), where a node routes probes there (IntelMac).
    rows = []
    for p in [probes + f".{i}" for i in range(9, 0, -1)] + [probes]:
        if not os.path.exists(p):
            continue
        for line in open(p, errors="ignore"):
            if "delib.generate.cache" not in line:
                continue
            try:
                f = json.loads(line)["fields"]
                rows.append((int(f["input_tokens"]), int(f["output_tokens"])))
            except (ValueError, KeyError):
                pass
    return rows

def act_sizes(log=os.path.expanduser("~/.continuum/logs/continuum-core-server.log"), last=40):
    text = re.sub(r"\x1b\[[0-9;]*m", "", "".join(open(p, errors="ignore").read() for p in [log + f".{i}" for i in range(9, 0, -1)] + [log] if os.path.exists(p)))
    rows = [(int(m.group(1)), int(m.group(2))) for m in re.finditer(r"delib\.generate\.cache.*?input_tokens=(\d+).*?output_tokens=(\d+)", text)]
    # Some nodes route probes to the JSONL sink instead of the core log: read whichever has them.
    rows = rows or probe_rows()
    if len(rows) < 5:
        sys.exit(f"only {len(rows)} measured acts in the log; not enough to size the benchmark")
    rows = rows[-last:]
    return {"acts": len(rows), "ctx_tokens": int(statistics.median(r[0] for r in rows)), "out_tokens": int(statistics.median(r[1] for r in rows))}

if __name__ == "__main__":
    argv = engine_argv()
    sizes = act_sizes()
    json.dump({"argv": argv, **sizes}, open(sys.argv[1], "w"), indent=1)
    print(json.dumps({k: v for k, v in sizes.items()}), "argv items", len(argv))
