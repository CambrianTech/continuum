#!/usr/bin/env python3
"""agent/solve reliability battery — proof harness (rewritten post-crash).

Runs a fixed set of SWE-shape bug-fix tasks against ONE persona on a misfit
7B via agent/solve (detached), grades each by running the repo's own assert,
and reports pass-rate + a CONTAMINATION metric (files touched beyond the
task's target — the cross-task / stale-memory bleed signature).

Usage:
  bench.py fire    <persona_uuid> <label> [suppress]  # workspaces + fire all (concurrent)
  bench.py runseq  <persona_uuid> <label> [suppress]  # one task at a time + grade
  bench.py grade   <label> [wait_s]                   # poll ledgers, grade, print table
"""
import json, os, subprocess, sys, time, shutil

def _resolve_cli():
    """Locate the continuum CLI.

    `uu` is THE official short alias (the double-U of contin-UU-m). `uu` is
    /usr/bin/cu (UUCP) on every Unix and was never ours — a default pointing at a
    `uu` binary resolved to a file that does not exist, so the harness failed at
    the first invocation instead of running. Prefer what is actually installed on
    PATH; fall back to the release build.
    """
    for name in ("uu", "continuum"):
        found = shutil.which(name)
        if found:
            return found
    return os.path.expanduser("~/.continuum/cache/cargo-target/release/continuum")


HOME = os.path.expanduser("~")
UU = _resolve_cli()
MODEL = "bartowski/Qwen2.5-Coder-7B-Instruct-GGUF"
ROOT = os.path.dirname(os.path.abspath(__file__)) + "/ws"
PDIR = f"{HOME}/.continuum/progress"

TASKS = [
    ("multiply", "mathlib.py", "def multiply(a, b):\n    return a + b\n",
     "from mathlib import multiply; assert multiply(3,4)==12 and multiply(0,5)==0",
     "There is a bug in mathlib.py: multiply returns a+b but should return a*b. Fix it."),
    ("subtract", "calc.py", "def subtract(a, b):\n    return a + b\n",
     "from calc import subtract; assert subtract(10,3)==7 and subtract(5,5)==0",
     "There is a bug in calc.py: subtract returns a+b but should return a-b. Fix it."),
    ("maxof", "util.py", "def max_of(xs):\n    return min(xs)\n",
     "from util import max_of; assert max_of([1,9,3])==9 and max_of([-5,-1])==-1",
     "There is a bug in util.py: max_of returns the minimum but should return the maximum. Fix it."),
    ("iseven", "nums.py", "def is_even(n):\n    return n % 2 == 1\n",
     "from nums import is_even; assert is_even(4)==True and is_even(3)==False",
     "There is a bug in nums.py: is_even is inverted (returns True for odd). Fix it."),
    ("revlist", "seq.py", "def rev(xs):\n    return xs\n",
     "from seq import rev; assert rev([1,2,3])==[3,2,1]",
     "There is a bug in seq.py: rev returns the list unchanged but should return it reversed. Fix it."),
]

# Tier 2 — harder SWE shapes: multi-file trace, class-invariant bug, base-case bug,
# implement-to-spec, and a multi-requirement normalizer. Tasks may seed EXTRA files
# (dict field #6); the instruction names the SYMPTOM, not always the buggy file.
TASKS2 = [
    ("geometry", "geometry.py",
     "import math\n\ndef circle_area(r):\n    return 2 * math.pi * r\n",
     "from geometry import circle_area; import math; assert abs(circle_area(2) - 4*math.pi) < 1e-9; "
     "from report import disk_report; assert disk_report(1) == 'area:3.14'",
     "disk_report in report.py prints the wrong area for every disk. The bug is somewhere "
     "in this project — trace it and fix the ROOT cause (do not patch around it in report.py).",
     {"report.py": "from geometry import circle_area\n\n"
                   "def disk_report(r):\n    return 'area:%.2f' % circle_area(r)\n"}),
    ("stack", "stack.py",
     "class Stack:\n    def __init__(self):\n        self._items = []\n\n"
     "    def push(self, x):\n        self._items.append(x)\n\n"
     "    def pop(self):\n        return self._items.pop(0)\n",
     "from stack import Stack; s = Stack(); s.push(1); s.push(2); s.push(3); "
     "assert s.pop() == 3 and s.pop() == 2 and s.pop() == 1",
     "Stack is meant to be LIFO but pops in FIFO order. Fix the bug.", None),
    ("fib", "fib.py",
     "def fib(n):\n    if n <= 1:\n        return 1\n    return fib(n-1) + fib(n-2)\n",
     "from fib import fib; assert fib(0) == 0 and fib(1) == 1 and fib(10) == 55",
     "fib(0) should be 0 and fib(10) should be 55, but the base case is wrong. Fix it.", None),
    ("slugify", "slug.py",
     "def slugify(s):\n    \"\"\"Lowercase, strip punctuation, join words with '-'.\n"
     "    slugify('Hello, World!') -> 'hello-world'\n    \"\"\"\n    pass\n",
     "from slug import slugify; assert slugify('Hello, World!') == 'hello-world'; "
     "assert slugify('  A  B  ') == 'a-b'",
     "slugify is unimplemented (returns None). Implement it per the docstring.", None),
    ("wordcount", "wc.py",
     "def count_words(text):\n    counts = {}\n    for w in text.split(','):\n"
     "        counts[w] = counts.get(w, 0) + 1\n    return counts\n",
     "from wc import count_words; assert count_words('the cat and the dog') == "
     "{'the': 2, 'cat': 1, 'and': 1, 'dog': 1}",
     "count_words should count whitespace-separated words, but it splits on commas. Fix it.",
     None),
]

TIERS = {"t1": TASKS, "t2": TASKS2}

def tier_tasks(tier):
    """Normalize both tier shapes to (name, file, src, check, instr, extra_files)."""
    out = []
    for t in TIERS[tier]:
        if len(t) == 5:
            name, fn, src, check, instr = t
            out.append((name, fn, src, check, instr, None))
        else:
            out.append(t)
    return out

def sh(*args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)

def rid(label, name):
    return f"{label}-{name}"

def seed_ws(label, name, fn, src, extra=None):
    ws = f"{ROOT}/{label}-{name}"
    if os.path.isdir(ws):
        shutil.rmtree(ws)
    os.makedirs(ws)
    sh("git", "-C", ws, "init", "-q")
    sh("git", "-C", ws, "config", "user.email", "t@t")
    sh("git", "-C", ws, "config", "user.name", "t")
    with open(f"{ws}/{fn}", "w") as f:
        f.write(src)
    for path, content in (extra or {}).items():
        full = os.path.join(ws, path)
        os.makedirs(os.path.dirname(full) or ws, exist_ok=True)
        with open(full, "w") as f:
            f.write(content)
    sh("git", "-C", ws, "add", "-A")
    sh("git", "-C", ws, "commit", "-qm", "seed")
    return ws

def fire_one(persona, model, label, name, fn, instr, ws, suppress, capture=None):
    led = f"{PDIR}/agent-solve-{rid(label,name)}.json"
    if os.path.exists(led):
        os.remove(led)
    task = (f"{instr} Work in your workspace: read the files, make the fix with "
            f"your tools, and run the code to confirm.")
    args = [UU, "agent/solve", "--persona-id", persona, "--base-model-id", model,
            "--task", task, "--workspace", ws, "--max-acts", "10",
            "--detach", "true", "--run-id", rid(label, name),
            # work IS training: the experience (never the solution) reaches her
            # living mind, so batteries generate the consolidations that dream
            # supersession feeds on (#221 slice 3).
            "--learn", "true"]
    if suppress:
        args += ["--suppress_recall", "true"]
    if capture:
        args += ["--capture_dir", capture]
    sh(*args)
    return led

def runseq(persona, label, tier="t1", model=MODEL, suppress=False,
           per_task_timeout=420, capture=None):
    for name, fn, src, _check, instr, extra in tier_tasks(tier):
        ws = seed_ws(label, name, fn, src, extra)
        led = fire_one(persona, model, label, name, fn, instr, ws, suppress, capture)
        t0 = time.time()
        while not os.path.exists(led) and time.time() - t0 < per_task_timeout:
            time.sleep(5)
        st = "done" if os.path.exists(led) else "TIMEOUT"
        print(f"  {name}: {st} ({int(time.time()-t0)}s)", flush=True)
    print(f"sequential run complete for {label}", flush=True)

def load_ledger(label, name):
    led = f"{PDIR}/agent-solve-{rid(label,name)}.json"
    if not os.path.exists(led):
        return None
    try:
        with open(led) as f:
            return json.load(f)
    except Exception:
        return None

def grade(label, tier="t1", wait_s=0, quiet=False):
    tasks = tier_tasks(tier)
    deadline = time.time() + wait_s
    while True:
        done = sum(1 for n, *_ in tasks if load_ledger(label, n) is not None)
        if done == len(tasks) or time.time() > deadline:
            break
        time.sleep(5)
    rows, npass, ncontam, nerr = [], 0, 0, 0
    for name, fn, _src, check, _instr, extra in tasks:
        ws = f"{ROOT}/{label}-{name}"
        led = load_ledger(label, name)
        acts = led.get("acts") if led else None
        failed = led.get("failed") if led else None
        err = (led or {}).get("error", "")
        files = (led or {}).get("files_changed", []) or []
        legit = {fn} | set((extra or {}).keys())
        stray = [f for f in files if f not in legit]
        if stray:
            ncontam += 1
        if failed:
            nerr += 1
        r = sh("python3", "-c", check, cwd=ws) if os.path.isdir(ws) else None
        ok = (r is not None and r.returncode == 0)
        if ok:
            npass += 1
        rows.append((name, ok, acts, files, stray, failed, err))
    if not quiet:
        print(f"\n=== BATTERY: {label} (tier {tier}) ===")
        print(f"{'task':10} {'pass':6} {'acts':5} {'files_changed':26} {'notes'}")
        for name, ok, acts, files, stray, failed, err in rows:
            tag = "PASS" if ok else ("INFRA" if failed else "FAIL")
            fstr = ",".join(files)[:24] if files else "(none)"
            note = f"BLEED:{stray}" if stray else (err[:50] if failed else "clean")
            print(f"{name:10} {tag:6} {str(acts):5} {fstr:26} {note}")
        print("-" * 60)
        print(f"PASS RATE: {npass}/{len(tasks)}   contaminated: {ncontam}   infra-errors: {nerr}")
    return {"label": label, "tier": tier, "npass": npass, "total": len(tasks),
            "contaminated": ncontam, "infra": nerr,
            "rows": [{"task": n, "pass": ok, "acts": a, "infra": bool(f)}
                     for n, ok, a, _fs, _st, f, _e in rows]}

def matrix(persona, outdir, models, tiers, reps):
    """THE MATRIX: whole-being persona x model x tier x rep, strictly sequential
    (one mind, one task at a time). Emits scoreboard.json + scoreboard.md with
    provenance. INFRA rows are counted separately from FAIL — machine noise
    never masquerades as mind performance."""
    os.makedirs(outdir, exist_ok=True)
    sha = sh("git", "-C", os.path.expanduser("~/Development/continuum"),
             "rev-parse", "--short", "HEAD").stdout.strip()
    started = time.strftime("%Y-%m-%d %H:%M:%S")
    arms = []
    # Run-unique label stamp — WITHOUT it, two matrix runs share labels and the
    # second SILENTLY DELETES the first's ledgers as it fires (found by the
    # forensic pass's missing-ledger signature on its first-ever execution).
    stamp = time.strftime("%H%M%S")
    for model in models:
        mslug = model.split("/")[-1][:24].lower().replace(".", "").replace("_", "-")
        for tier in tiers:
            for rep in range(1, reps + 1):
                label = f"mx{stamp}-{mslug}-{tier}-r{rep}"
                print(f"\n### ARM {label} (model={model} tier={tier} rep={rep}) ###",
                      flush=True)
                runseq(persona, label, tier=tier, model=model,
                       capture=f"{outdir}/captures")
                res = grade(label, tier=tier)
                res.update({"model": model, "rep": rep})
                arms.append(res)
                with open(f"{outdir}/scoreboard.json", "w") as f:
                    json.dump({"started": started, "commit": sha,
                               "persona": persona, "arms": arms}, f, indent=2)
    # Markdown scoreboard: mean pass-rate per (model, tier) over CLEAN runs.
    lines = [
        "# agent/solve matrix — whole-being persona, memory ON, never stripped",
        f"\nstarted: {started} · commit: `{sha}` · persona: `{persona}` · reps: {reps}",
        "\nINFRA = lane/pressure faults (machine noise), scored separately — a run",
        "with any INFRA rows is reported but excluded from the mind's mean.",
        "\n| model | tier | clean runs | mean pass | per-run | contaminated | infra rows |",
        "|---|---|---|---|---|---|---|",
    ]
    for model in models:
        for tier in tiers:
            sel = [a for a in arms if a["model"] == model and a["tier"] == tier]
            clean = [a for a in sel if a["infra"] == 0]
            per = ", ".join(f"{a['npass']}/{a['total']}" for a in sel)
            mean = (sum(a["npass"] / a["total"] for a in clean) / len(clean)
                    if clean else None)
            lines.append(
                f"| {model.split('/')[-1]} | {tier} | {len(clean)}/{len(sel)} | "
                f"{('%.0f%%' % (mean * 100)) if mean is not None else 'VOID'} | {per} | "
                f"{sum(a['contaminated'] for a in sel)} | {sum(a['infra'] for a in sel)} |")
    with open(f"{outdir}/scoreboard.md", "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines), flush=True)
    print(f"\nscoreboard written to {outdir}/scoreboard.md", flush=True)

    # Append aggregated rows to the CANONICAL evidence ledger (benchmarks/RESULTS.jsonl)
    # — one row per (model, tier) over the INFRA-clean reps, same schema the README
    # renderer consumes ("edit the data, re-render", never hand-edited claims). The
    # whole-being arm is OURS; opponent harnesses (hermes/aider/opencode) join later as
    # sibling arms on the SAME tasks. All-reps-infra → excluded:true (VOID, never 0%).
    import platform
    ledger = os.path.expanduser("~/Development/continuum/benchmarks/RESULTS.jsonl")
    stamped = time.strftime("%Y-%m-%d")
    with open(ledger, "a") as f:
        for model in models:
            for tier in tiers:
                sel = [a for a in arms if a["model"] == model and a["tier"] == tier]
                clean = [a for a in sel if a["infra"] == 0]
                score = sum(a["npass"] for a in clean)
                total = sum(a["total"] for a in clean)
                row = {
                    "benchmark": f"agent-solve-{tier}",
                    "model": model.split("/")[-1].replace("-Instruct-GGUF", ""),
                    "arm": "OURS",
                    "score": score, "total": total,
                    "pass_rate": (score / total) if total else None,
                    "mean_output_tokens": None,
                    "excluded": not clean,
                    "captured": stamped, "git_sha": sha,
                    "machine": platform.node(),
                    "note": f"whole-being agent/solve battery, {len(clean)}/{len(sel)} clean reps, memory ON, never stripped",
                }
                f.write(json.dumps(row) + "\n")
    print(f"appended {len(models) * len(tiers)} rows to {ledger}", flush=True)
    forensic(outdir)  # the standing order: every run ends with a forensic pass

import re

# Structural-defect signatures, each learned from a REAL glass-boxed failure
# (2026-07-22 session). The forensic pass is a completeness critic: it does not
# judge, it FLAGS with evidence — every finding names the mechanism class the
# operator (or a persona) should chase, and links the raw artifact.
CALL_SHAPE = re.compile(r"\b[a-z_/]{3,}\(\s*\{")           # edit_file({ / code/read({
RECEIPT_SHAPE = re.compile(r"\[Action #\d+\]|^Result:|\"stdout\"", re.M)

def forensic(outdir):
    """Post-run forensic sweep: mine every arm's ledgers + workspaces + captures
    for STRUCTURAL issues (not scores). Writes <outdir>/forensics.md."""
    with open(f"{outdir}/scoreboard.json") as f:
        board = json.load(f)
    findings = []  # (severity, klass, arm_label, task, evidence)
    for arm in board["arms"]:
        label, tier = arm["label"], arm["tier"]
        for name, fn, _src, _check, _instr, extra in tier_tasks(tier):
            led = load_ledger(label, name)
            if not led:
                findings.append(("HIGH", "missing-ledger", label, name, "no ledger file"))
                continue
            if led.get("failed"):
                findings.append(("INFO", "infra", label, name, (led.get("error") or "")[:120]))
                continue
            acts = led.get("acts") or 0
            spoken = led.get("spoken") or ""
            patch = led.get("patch") or ""
            files = led.get("files_changed") or []
            legit = {fn} | set((extra or {}).keys())
            target_ext = os.path.splitext(fn)[1]
            # 1. Silent settle: she gave up without acting or speaking — the
            #    highest-priority unexplained class (needs the capture).
            if acts <= 1 and not patch and not spoken.strip():
                findings.append(("HIGH", "silent-settle", label, name,
                                 f"acts={acts}, no patch, no spoken"))
            # 2. Unlifted call shapes: she EMITTED tool-call-looking text but no
            #    file changed — a parser/idiom gap candidate (the #219 family).
            if not patch and CALL_SHAPE.search(spoken):
                snippet = CALL_SHAPE.search(spoken).group(0)
                findings.append(("HIGH", "unlifted-call-shape", label, name,
                                 f"speech contains '{snippet}...' but no patch"))
            # 3. Fabricated receipts: transcript-mimicry / invented results.
            if RECEIPT_SHAPE.search(spoken) and acts <= 1:
                findings.append(("HIGH", "fabricated-receipts", label, name,
                                 "receipt-shaped text with <=1 real act"))
            # 4. Language drift: wrote a different language than the task's —
            #    the stale-memory bias signature (Rust into python repos).
            drift = [f for f in files if os.path.splitext(f)[1] not in ("", target_ext)]
            if drift:
                findings.append(("HIGH", "language-drift", label, name,
                                 f"target is {target_ext}, wrote {drift}"))
            # 5. Off-target writes (same language, wrong/new files).
            stray = [f for f in files if f not in legit and f not in drift]
            if stray:
                findings.append(("MED", "off-target-write", label, name, f"{stray}"))
            # 6. Symptom patch: multi-file task where ONLY the symptom file moved.
            if extra and files and fn not in files and any(f in extra for f in files):
                findings.append(("MED", "symptom-patch", label, name,
                                 f"edited {files}, root-cause file {fn} untouched"))
            # 7. Zero-length drives that PASSED (suspicious: check the seed).
            if arm["rows"] and acts == 0 and any(
                r["task"] == name and r["pass"] for r in arm["rows"]
            ):
                findings.append(("HIGH", "pass-without-acting", label, name,
                                 "graded PASS with acts=0 — audit the seed/grader"))
    # Captures inventory (deep tick-level mapping is the next iteration).
    capdir = f"{outdir}/captures"
    caps = []
    if os.path.isdir(capdir):
        for c in os.listdir(capdir):
            p = os.path.join(capdir, c)
            caps.append(f"{c}: {os.path.getsize(p)//1024}KB, "
                        f"{sum(1 for _ in open(p, errors='ignore'))} ticks")
    sev_rank = {"HIGH": 0, "MED": 1, "INFO": 2}
    findings.sort(key=lambda x: sev_rank.get(x[0], 3))
    lines = ["# Forensic pass — structural issues, not scores",
             f"\nrun: {board.get('started')} · commit: `{board.get('commit')}` · "
             f"{len(board['arms'])} arms · {len(findings)} findings\n"]
    by_class = {}
    for sev, klass, label, task, ev in findings:
        by_class.setdefault(klass, []).append((sev, label, task, ev))
    for klass, rows in by_class.items():
        lines.append(f"\n## {klass} ({len(rows)})")
        for sev, label, task, ev in rows:
            lines.append(f"- **{sev}** `{label}/{task}` — {ev}")
    if caps:
        lines.append("\n## captures on disk (tick-level evidence)")
        lines += [f"- {c}" for c in caps]
    report = "\n".join(lines) + "\n"
    with open(f"{outdir}/forensics.md", "w") as f:
        f.write(report)
    print(report, flush=True)
    return findings

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "help"
    if cmd == "runseq":
        tier = sys.argv[4] if len(sys.argv) > 4 else "t1"
        runseq(sys.argv[2], sys.argv[3], tier=tier)
        grade(sys.argv[3], tier=tier, wait_s=0)
    elif cmd == "grade":
        grade(sys.argv[2], tier=sys.argv[3] if len(sys.argv) > 3 else "t1",
              wait_s=int(sys.argv[4]) if len(sys.argv) > 4 else 0)
    elif cmd == "forensic":
        forensic(sys.argv[2])
    elif cmd == "matrix":
        # matrix <persona> <outdir> [reps] [model1,model2,...]
        reps = int(sys.argv[4]) if len(sys.argv) > 4 else 2
        models = sys.argv[5].split(",") if len(sys.argv) > 5 else [MODEL]
        matrix(sys.argv[2], sys.argv[3], models, ["t1", "t2"], reps)
    else:
        print(__doc__)
