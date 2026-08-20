#!/usr/bin/env python3
"""
run_project.py — ONE runner for the whole-app / project / website benchmark tier.

The pattern is proven in swe/run_ours.py: fetch an instance -> set up a real
workspace -> point a Continuum persona's HANDS at it (cognition/eval with
--workspace_root roots her file engine there) -> capture the artifact she produced
-> grade it -> append to the durable ledger. This generalizes that into an ADAPTER
registry so every benchmark is a small plug-in, not a fork.

An ADAPTER implements four hooks (see BenchmarkAdapter):
  fetch(instance)     -> a task dict (problem statement + whatever grading needs)
  setup(task, workdir)-> workspace_root (the dir the persona's hands root at)
  prompt(task)        -> the natural-language task the persona works on
  grade(task, ws)     -> {"passed": int, "total": int, ...} or {"needs": "..."} 

Run:  python3 benchmarks/project/run_project.py --benchmark <name> --instance <id>
List: python3 benchmarks/project/run_project.py --list

Adapters that need heavy infra (Docker, a live web server, app APIs, a GPU) declare
it in `.requires`; the runner reports it honestly instead of pretending — that infra
is the grid's job, and the adapter is ready the moment it's present.
"""
import argparse, datetime, json, os, platform, shutil, subprocess, sys, tempfile, time

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


HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
UU = _resolve_cli()
LEDGER = os.path.join(ROOT, "benchmarks", "RESULTS.jsonl")


def sh(cmd, cwd=None, check=True):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"cmd failed: {' '.join(cmd)}\n{r.stdout}\n{r.stderr}")
    return r


class BenchmarkAdapter:
    """One benchmark's fetch/setup/prompt/grade. Subclass + register."""
    name = "abstract"
    # Human note on any infra this adapter needs before it can grade for real.
    requires = None  # e.g. "Docker + the official swebench harness"

    def fetch(self, instance):        raise NotImplementedError
    def setup(self, task, workdir):   raise NotImplementedError  # -> workspace_root
    def prompt(self, task):           raise NotImplementedError  # -> str
    def grade(self, task, ws):        raise NotImplementedError  # -> dict


def resolve_persona():
    r = subprocess.run([UU, "cognition/personas"], capture_output=True, text=True)
    ps = (json.loads(r.stdout).get("personas") or []) if r.stdout.strip().startswith("{") else []
    if not ps:
        raise SystemExit("no resident persona (is the core booted?)")
    return ps[0]["persona_id"], ps[0].get("name")


def run_persona_on(workspace_root, prompt, note, max_acts=25):
    """Point the resident persona's hands at `workspace_root` and let her work `prompt`.
    Detached + polled — the same humane path SWE uses (she edits the tree in the
    background; grading is external so the eval's own dod is a no-op)."""
    pid, name = resolve_persona()
    print(f"[persona] {name} ({pid})")
    wd = os.path.dirname(workspace_root)
    task = {"id": note, "prompt": prompt, "dod_shell": "true", "lang": "rust"}
    tf = os.path.join(wd, "task.jsonl"); open(tf, "w").write(json.dumps(task) + "\n")
    led = os.path.expanduser(f"~/.continuum/progress/{pid}.jsonl")
    n0 = sum(1 for _ in open(led)) if os.path.exists(led) else 0
    cap = os.path.join(wd, "capture")
    print(f"[run] dispatching on workspace={workspace_root} (detached, max_acts={max_acts}, capture→{cap})")
    sh([UU, "cognition/eval", "--persona_id", pid, "--eval_set", tf,
        "--workspace_root", workspace_root, "--capture_dir", cap,
        "--max_acts", str(max_acts), "--note", note, "--detach", "true"], check=False)
    for _ in range(40):
        time.sleep(30)
        if os.path.exists(led):
            rows = [json.loads(l) for l in open(led) if l.strip()]
            if any(r.get("note") == note for r in rows[n0:]):
                print("[run] persona run landed"); return
    print("[run] WARN: run did not land in the ledger window (may still be working)")


def run_persona_agent(workspace_root, prompt, note, max_acts=25,
                      base_model="unsloth/Devstral-Small-2507-GGUF"):
    """The proven whole-being driver (agent/solve: drive_to_settle + idiom-lifted
    tool parsing + workspace grounding — the 87-93% t1 path). Detached + polled
    via the run ledger (#86)."""
    pid, name = resolve_persona()
    print(f"[persona] {name} ({pid})")
    run_id = f"proj-{note}"
    ledger = os.path.expanduser(f"~/.continuum/progress/agent-solve-{run_id}.json")
    if os.path.exists(ledger):
        os.remove(ledger)
    print(f"[agent] dispatching {run_id} (workspace={workspace_root}, max_acts={max_acts})")
    sh([UU, "agent/solve", "--persona-id", pid, "--base-model-id", base_model,
        "--task", prompt, "--workspace", workspace_root, "--max-acts", str(max_acts),
        "--learn", "true", "--detach", "true", "--run-id", run_id], check=False)
    for _ in range(120):
        time.sleep(30)
        if os.path.exists(ledger):
            led = json.load(open(ledger))
            print(f"[agent] landed: acts={led.get('acts')} failed={led.get('failed')} "
                  f"error={led.get('error') or '-'}")
            return
    print("[agent] WARN: ledger did not land in the poll window (may still be working)")


def append_ledger(benchmark, model, arm, result):
    row = {"benchmark": benchmark, "model": model, "arm": arm,
           "score": result.get("passed"), "total": result.get("total"),
           "pass_rate": (result["passed"]/result["total"]) if result.get("total") else None,
           "mean_output_tokens": None, "excluded": bool(result.get("excluded")),
           "captured": datetime.date.today().isoformat(),
           "git_sha": sh(["git","rev-parse","--short","HEAD"], cwd=ROOT, check=False).stdout.strip() or "unknown",
           "machine": platform.node() or "unknown", "note": result.get("note")}
    try:
        with open(LEDGER, "a") as f: f.write(json.dumps(row) + "\n")
        print(f"[ledger] appended → {LEDGER}")
    except Exception as e:
        print(f"[ledger] WARN append failed: {e}")


# ─────────────────────────── adapters ───────────────────────────

class SweBenchAdapter(BenchmarkAdapter):
    """Real GitHub issues in real repos. Ports swe/run_ours.py's proven flow."""
    name = "swe-bench-lite"
    requires = "Docker + the official `swebench` harness for scoring (clone→edit→diff works without it)"
    dataset = "princeton-nlp/SWE-bench_Lite"

    def fetch(self, instance):
        import urllib.request, urllib.parse
        for off in range(0, 400, 100):
            url = (f"https://datasets-server.huggingface.co/rows?dataset="
                   f"{urllib.parse.quote(self.dataset, safe='')}&config=default&split=test&offset={off}&length=100")
            for r in json.load(urllib.request.urlopen(url, timeout=30)).get("rows", []):
                if r["row"]["instance_id"] == instance:
                    return r["row"]
        raise SystemExit(f"instance {instance} not in {self.dataset}")

    def setup(self, task, workdir):
        repo_dir = os.path.join(workdir, "repo")
        sh(["git", "clone", f"https://github.com/{task['repo']}.git", repo_dir])
        sh(["git", "checkout", "-q", task["base_commit"]], cwd=repo_dir)
        return repo_dir

    def prompt(self, task):
        return ("You are fixing a real bug in a git repository already rooted at your workspace. "
                "Use code/search + code/read to find the source, code/edit to fix it IN PLACE "
                "(edit existing files; do not create new top-level files). When done the working "
                f"tree holds your fix.\n\nISSUE:\n{task['problem_statement']}")

    def grade(self, task, ws):
        diff = sh(["git", "diff"], cwd=ws).stdout
        preds = os.path.join(os.path.dirname(ws), "preds.jsonl")
        open(preds, "w").write(json.dumps({"instance_id": task["instance_id"],
            "model_patch": diff, "model_name_or_path": "continuum"}) + "\n")
        return {"passed": None, "total": 1, "note": f"{len(diff)}b patch → {preds}; score with: "
                f"python -m swebench.harness.run_evaluation --dataset_name {self.dataset} "
                f"--predictions_path {preds} --instance_ids {task['instance_id']} --run_id ours"}


class Commit0Adapter(BenchmarkAdapter):
    """Build an entire small library from a spec + its unit tests. Local-runnable
    (pytest), no Docker — the most achievable whole-project benchmark to develop
    against first. `instance` = a git repo url of a stubbed library w/ tests."""
    name = "commit0"
    requires = "the target library repo (its own pytest suite is the grader) + its pip deps"

    def fetch(self, instance):
        return {"instance_id": instance, "repo_url": instance}

    def setup(self, task, workdir):
        repo_dir = os.path.join(workdir, "lib")
        sh(["git", "clone", task["repo_url"], repo_dir])
        return repo_dir

    def prompt(self, task):
        return ("This is a Python library with a specification and a pytest test suite but "
                "UNIMPLEMENTED functions (they raise NotImplementedError or are stubs). "
                "Implement them so the tests pass. Use code/read to understand the spec + tests, "
                "code/edit to fill in the implementations, and `code/shell: python -m pytest -q` to "
                "check your progress. Iterate until the suite is green.")

    def grade(self, task, ws):
        r = subprocess.run(["python", "-m", "pytest", "-q", "--tb=no"], cwd=ws, capture_output=True, text=True)
        out = (r.stdout + r.stderr)
        import re
        m = re.search(r"(\d+) passed", out); passed = int(m.group(1)) if m else 0
        f = re.search(r"(\d+) failed", out); failed = int(f.group(1)) if f else 0
        total = passed + failed
        return {"passed": passed, "total": total or 1,
                "note": f"pytest: {passed} passed, {failed} failed"}


class WebsiteAdapter(BenchmarkAdapter):
    """Launch an ENTIRE website and fix its appearance — the whole-site tier.

    She builds a real multi-page site in the workspace, LAUNCHES it (code/shell),
    self-verifies with curl, and CAPTURES screenshots via `interface/capture`
    (the build→run→see loop; the PNGs land in the run record for the appearance
    review). Grading is structural + liveness: every required page serves 200,
    required elements present, nav links resolve, a real shared stylesheet is
    linked. Appearance scoring by a vision describer is the named next leg —
    the screenshots this loop records are its training corpus.
    """
    name = "website"
    requires = "a headless browser for interface/capture (Chrome); nothing else"

    SPECS = {
        "bakery": {
            "port": 8734,
            "brief": ("a small-business website for 'Millbrook Bakery' — a neighborhood "
                      "bakery. Pages: index.html (hero with the bakery name, hours, a "
                      "featured-items section), about.html (story + team), contact.html "
                      "(a contact form with name/email/message fields using required "
                      "attributes, plus address and phone). All pages share ONE "
                      "stylesheet styles.css (real styling: readable typography, a "
                      "color scheme, responsive nav that links all three pages)."),
            "pages": {
                "index.html":   ["millbrook", "styles.css", "about.html", "contact.html"],
                "about.html":   ["styles.css", "index.html"],
                "contact.html": ["<form", "required", "styles.css"],
            },
            "css_min_bytes": 400,
        },
        "portfolio": {
            "port": 8735,
            "brief": ("a personal portfolio site for a freelance photographer named "
                      "Rivera. Pages: index.html (hero + a gallery grid of at least 6 "
                      "figure/img placeholders with captions), services.html (pricing "
                      "table), contact.html (form with validation attributes). One "
                      "shared styles.css with a responsive grid for the gallery."),
            "pages": {
                "index.html":    ["rivera", "styles.css", "<figure", "services.html"],
                "services.html": ["styles.css", "<table"],
                "contact.html":  ["<form", "required", "styles.css"],
            },
            "css_min_bytes": 400,
        },
    }

    def fetch(self, instance):
        spec = self.SPECS.get(instance)
        if not spec:
            raise SystemExit(f"unknown website instance '{instance}' — have: {', '.join(self.SPECS)}")
        return {"id": instance, **spec}

    def setup(self, task, workdir):
        ws = os.path.join(workdir, "site")
        os.makedirs(ws, exist_ok=True)
        sh(["git", "init", "-q"], cwd=ws, check=False)
        return ws

    def prompt(self, task):
        p = task["port"]
        return (
            f"Build and LAUNCH a complete website: {task['brief']}\n\n"
            f"Work in the current workspace. When the files are written:\n"
            f"1. Launch it: code/shell `nohup python3 -m http.server {p} > server.log 2>&1 &`\n"
            f"2. Verify every page: code/shell `curl -s http://127.0.0.1:{p}/index.html | head -5` "
            f"(and about/services/contact pages).\n"
            f"3. SEE it: call interface/capture with target \"web\" and url "
            f"\"http://127.0.0.1:{p}\" — look at the result and fix anything broken "
            f"(missing styles, broken nav, unreadable text), then re-verify.\n"
            f"Leave the server RUNNING when you finish — a site that is not serving "
            f"is not launched."
        )

    def grade(self, task, ws):
        import urllib.request
        p, results, notes = task["port"], [], []
        live = True
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{p}/", timeout=5)
        except Exception:
            live = False
            notes.append("server NOT left running (launch leg failed)")
            # Serve the tree ourselves to grade the CONTENT leg honestly.
            subprocess.Popen(["python3", "-m", "http.server", str(p)], cwd=ws,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(1.5)
        for page, needles in task["pages"].items():
            try:
                html = urllib.request.urlopen(
                    f"http://127.0.0.1:{p}/{page}", timeout=5).read().decode("utf-8", "replace").lower()
                missing = [n for n in needles if n.lower() not in html]
                ok = not missing
                if missing:
                    notes.append(f"{page}: missing {missing}")
            except Exception as e:
                ok = False
                notes.append(f"{page}: {e}")
            results.append(ok)
        css = os.path.join(ws, "styles.css")
        css_ok = os.path.exists(css) and os.path.getsize(css) >= task["css_min_bytes"]
        if not css_ok:
            notes.append("styles.css missing or trivially small")
        results.append(css_ok)
        results.append(live)  # the LAUNCH leg is scored
        # cleanup: reap whatever python http.server holds the port (verified by comm)
        try:
            pid = subprocess.run(["lsof", "-ti", f"tcp:{p}", "-sTCP:LISTEN"],
                                 capture_output=True, text=True).stdout.strip().splitlines()
            for x in pid:
                comm = subprocess.run(["ps", "-p", x, "-o", "comm="],
                                      capture_output=True, text=True).stdout.strip()
                if "python" in comm.lower():
                    subprocess.run(["kill", "-9", x], capture_output=True)
        except Exception:
            pass
        return {"passed": sum(results), "total": len(results),
                "note": "; ".join(notes) or "all pages serve with required structure; server left running"}


ADAPTERS = {a.name: a for a in [SweBenchAdapter(), Commit0Adapter(), WebsiteAdapter()]}


def main():
    ap = argparse.ArgumentParser(description="Run a Continuum persona against a project-tier benchmark.")
    ap.add_argument("--benchmark", help="adapter name (see --list)")
    ap.add_argument("--instance", help="instance id / repo url")
    ap.add_argument("--workdir", default=None)
    ap.add_argument("--max-acts", type=int, default=25)
    ap.add_argument("--solver", choices=["agent", "eval"], default="agent",
                    help="agent = the proven agent/solve whole-being driver; eval = legacy cognition/eval")
    ap.add_argument("--list", action="store_true")
    args = ap.parse_args()

    if args.list or not args.benchmark:
        print("Project-benchmark adapters:")
        for n, a in ADAPTERS.items():
            print(f"  {n:16} requires: {a.requires or 'nothing extra'}")
        print("\nCatalog (uu benchmark/list) has more targets; each becomes an adapter here.")
        return

    adapter = ADAPTERS.get(args.benchmark)
    if not adapter:
        raise SystemExit(f"no adapter for '{args.benchmark}'. Have: {', '.join(ADAPTERS)}")
    if not args.instance:
        raise SystemExit("--instance required (id or repo url)")

    wd = args.workdir or tempfile.mkdtemp(prefix=f"proj-{args.benchmark}-")
    print(f"[{args.benchmark}] instance={args.instance} workdir={wd}")
    task = adapter.fetch(args.instance)
    ws = adapter.setup(task, wd)
    driver = run_persona_agent if args.solver == "agent" else run_persona_on
    driver(ws, adapter.prompt(task), f"{args.benchmark}-{args.instance}", args.max_acts)
    result = adapter.grade(task, ws)
    print(f"[grade] {json.dumps(result)}")
    append_ledger(args.benchmark, "resident-persona", "OURS", result)


if __name__ == "__main__":
    main()
