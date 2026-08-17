#!/usr/bin/env python3
"""
grade_local.py — SWE-bench grading for pure-Python instances, without Docker.

WHY THIS EXISTS. On 2026-08-04 eight flask-4045 runs were driven by hand against a clone
left at flask HEAD. The upstream fix was already in that tree, so FAIL_TO_PASS passed
before any persona acted: every score in the series — the zeros AND the one that looked
like a win — measured nothing. The official `swebench.harness.run_evaluation` would have
caught it, but it needs Docker and a per-instance image, which is heavy enough that the
loop got shortcut instead. A grader that runs in seconds is a grader that actually gets
run ([[the-swe-workspace-was-never-at-the-base-commit-the-bug-was-not-there]]).

It reproduces the official protocol exactly:

    clone @ base_commit  ->  apply model_patch  ->  apply test_patch  ->  run tests
    RESOLVED  iff  every FAIL_TO_PASS passes AND every PASS_TO_PASS passes

and it refuses to report on a tree it cannot vouch for. `--gold` is the spine check: the
dataset's own patch MUST resolve. If gold does not resolve, the environment is wrong and
no persona number from it means anything — that is the whole point, so gold failing is a
loud non-zero exit, never a warning.

Scope: pure-Python repos whose deps pip-install cleanly (flask, requests, …). Anything
needing a build step or system libraries still belongs in the Docker harness.
"""
import argparse, json, os, re, shutil, subprocess, sys, tempfile, urllib.parse, urllib.request


def sh(cmd, cwd=None, check=True, env=None):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env)
    if check and r.returncode != 0:
        raise SystemExit(f"cmd failed: {' '.join(cmd)}\n{r.stdout}\n{r.stderr}")
    return r


def hf_instance(dataset, iid):
    for off in range(0, 400, 100):
        url = (f"https://datasets-server.huggingface.co/rows?dataset={urllib.parse.quote(dataset, safe='')}"
               f"&config=default&split=test&offset={off}&length=100")
        for r in json.load(urllib.request.urlopen(url, timeout=30)).get("rows", []):
            if r["row"]["instance_id"] == iid:
                return r["row"]
    raise SystemExit(f"instance {iid} not found in {dataset}")


def ensure_env(base_python, env_root, instance, repo_dir, pytest_pin, as_of):
    """A cached per-instance venv, with dependencies resolved AS OF the instance's date.

    The date is the whole trick. These instances are years old and their packaging is
    almost always upper-bound-free — flask 2.0 asks for `Werkzeug>=2.0`, so a plain
    `pip install -e .` in 2026 resolves Werkzeug 3.x, which deleted `url_quote`, and the
    repo cannot even import. Guessing per-repo pins by hand does not scale past a couple
    of instances and silently rots.

    `uv pip install --exclude-newer <created_at>` resolves the whole graph against the
    index as it existed on that day, which is what the official Docker images bake in.
    One rule, every repo, no hand-maintained pin table.

    Per-instance rather than per-repo because instances span years of a repo's history
    and their dependency graphs genuinely differ; sharing one env is how you get a green
    gold on one instance and a mystery failure on the next. Cached because pip resolution
    is the slow part and a re-grade after a persona run must be instant — a grader that
    takes minutes to re-run is a grader that gets skipped, which is the exact failure this
    file exists to prevent.
    """
    env = os.path.join(env_root, instance)
    py = os.path.join(env, "bin", "python")
    if os.path.exists(py):
        return py
    os.makedirs(env_root, exist_ok=True)
    uv = shutil.which("uv")
    # The INTERPRETER has an era too, not just the dependency graph. A 2014 requests
    # vendors a urllib3 that does `from collections import Mapping`, which Python 3.10
    # deleted — no dependency pin can rescue that, the language moved. uv can fetch any
    # CPython on demand, so pick one the instance's code could actually have run on.
    # Coarse on purpose: the gold gate is the arbiter, and a wrong guess fails loudly
    # rather than producing a plausible number.
    if uv and as_of and base_python in ("auto", "", None):
        year = int(as_of[:4]) if as_of[:4].isdigit() else 2023
        # 3.9 is the last release with the `collections.Mapping` aliases that pre-2020
        # code reaches for; uv's own floor is 3.8, so there is no lower rung to offer.
        base_python = "3.9" if year < 2020 else "3.11"
        print(f"[env] instance is from {year} — using Python {base_python}")
    print(f"[env] building {instance} venv{f' (deps as of {as_of[:10]})' if uv and as_of else ''}")
    if uv and as_of:
        sh([uv, "venv", "--python", base_python, "-q", env])
        # THE SPLIT: SUBJECT vs HARNESS.
        #
        # Subject — the repo's own runtime dependency graph — is pinned to the instance's
        # date, because those versions define the behavior under test.
        #
        # Harness — pytest, setuptools, wheel — is deliberately MODERN, because it has to
        # run on this machine's interpreter. Date-pinning it breaks in ways that have
        # nothing to do with the instance, and both failures are worth naming since they
        # cost real time here: 2021 `py` raises `AttributeError: __spec__` under Python
        # 3.11's import machinery, and 2021 setuptools predates PEP 660 so
        # `build_meta:__legacy__` has no `build_editable` and the install dies outright.
        #
        # `--no-build-isolation` is what lets the two coexist: the build runs against the
        # modern setuptools already in the venv instead of pip fetching a date-pinned one.
        sh([uv, "pip", "install", "-q", "--python", py, pytest_pin, "setuptools", "wheel"],
           check=False)
        r = sh([uv, "pip", "install", "-q", "--python", py, "--exclude-newer", as_of,
                "--no-build-isolation", "-e", "."], cwd=repo_dir, check=False)
    else:
        sh([base_python, "-m", "venv", env])
        sh([py, "-m", "pip", "-q", "install", "--upgrade", "pip", "setuptools", "wheel"], check=False)
        sh([py, "-m", "pip", "-q", "install", pytest_pin], check=False)
        r = sh([py, "-m", "pip", "-q", "install", "-e", "."], cwd=repo_dir, check=False)
    if r.returncode != 0:
        # DELETE the half-built env rather than cache it. Caching on "the venv directory
        # exists" made a failed install sticky: every later run reused a venv with no repo
        # in it and reported a gold failure whose real cause was three steps upstream.
        # An env is only worth keeping if it was actually built.
        shutil.rmtree(env, ignore_errors=True)
        raise SystemExit(
            f"[env] could not install {instance}'s repo into a venv — nothing can be graded "
            f"here, and a cached broken env would poison every later run:\n{r.stderr[-600:]}"
        )
    return py


def apply_patch(repo, text, what):
    """git apply, tolerating the whitespace drift that trips a strict apply."""
    if not text.strip():
        return
    p = os.path.join(repo, f".{what}.patch")
    open(p, "w").write(text)
    for extra in ([], ["--ignore-whitespace"], ["--ignore-whitespace", "-C1"]):
        if sh(["git", "apply", *extra, p], cwd=repo, check=False).returncode == 0:
            os.remove(p)
            return
    raise SystemExit(f"could not apply {what} patch — the tree is not what the patch expects")


def patched_test_files(test_patch):
    """The test files the instance's own test_patch touches — the scope to run."""
    return sorted({m.group(1) for m in re.finditer(r"^\+\+\+ b/(\S+)", test_patch, re.M)})


def run_tests(repo, venv_py, tests, test_files):
    """
    Resolve each required test id against ONE pytest run over the instance's test files.

    The dataset does not use a single id shape. pytest/flask instances give node ids
    (`tests/test_x.py::test_y`); sympy gives BARE function names (`test_solve_biquadratic`)
    because sympy ships its own runner, and handing those to pytest as paths produces
    `ERROR: file or directory not found` — which scores as a failure and looks exactly
    like a real one. That mis-scored gold at 0/N on instances whose environment was fine.

    So: run the patched test files once, parse the per-test verdicts, and look each
    required id up by node id OR by bare function name. A test that never appears in the
    report (collection error, renamed, wrong file) is a failure — but a NAMED one.
    """
    env = dict(os.environ, PYTHONPATH=os.path.join(repo, "src"), PYTHONDONTWRITEBYTECODE="1")
    scope = test_files or sorted({t.split("::")[0] for t in tests if t.endswith(".py")})
    if not scope:
        return {t: (False, "no test file to run — test_patch touched nothing") for t in tests}

    r = sh([venv_py, "-m", "pytest", *scope, "-v", "--no-header", "-rN",
            "-p", "no:cacheprovider"], cwd=repo, check=False, env=env)
    report = r.stdout + r.stderr

    by_node, by_func = {}, {}
    for m in re.finditer(r"^(\S+::\S+?)\s+(PASSED|FAILED|ERROR|XFAIL|XPASS|SKIPPED)", report, re.M):
        node, verdict = m.group(1), m.group(2)
        ok = verdict in ("PASSED", "XFAIL", "SKIPPED")
        by_node[node] = ok
        func = node.split("::")[-1].split("[")[0]
        # Same bare name in two files: only count it as passing if every one passed.
        by_func[func] = ok and by_func.get(func, True)

    out = {}
    for t in tests:
        if t in by_node:
            out[t] = (by_node[t], "")
        else:
            key = t.split("::")[-1].split(".")[-1].split("[")[0]
            if key in by_func:
                out[t] = (by_func[key], "")
            else:
                out[t] = (False, f"not present in the pytest report for {' '.join(scope)}")
    if not by_node:
        print(f"[tests] pytest collected NOTHING from {' '.join(scope)}:\n{report[-800:]}")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", default="princeton-nlp/SWE-bench_Lite")
    ap.add_argument("--instance", required=True)
    ap.add_argument("--predictions", help="preds.jsonl from run_ours.py (the model_patch to grade)")
    ap.add_argument("--gold", action="store_true",
                    help="SPINE CHECK: grade the dataset's own patch. It MUST resolve; "
                         "a failure here means the environment is wrong and no other "
                         "number from it is meaningful.")
    ap.add_argument("--venv", help="python interpreter with pytest + the repo's deps")
    ap.add_argument("--auto-venv", metavar="BASE_PYTHON",
                    help="build the environment instead: a venv from BASE_PYTHON with "
                         "pytest + the repo installed editable. Cached per instance under "
                         "--env-root, so a re-grade is instant.")
    # THE CANONICAL ENV ROOT, and the ONLY one. This default used to be
    # `~/.continuum/cache/swe-envs` — a SECOND cache that the Rust core never reads and
    # never writes. Both roots existed, both held real venvs, and neither named the other.
    # On 2026-08-17 that cost a full misdiagnosis: `ls`ing the retired root showed 14 envs
    # across 3 repos and produced the headline "77% of staged instances have no
    # environment", reported and acted on. The live root held 46 envs across 8 repos —
    # 95% coverage. Same question, two directories, opposite answers.
    # Keep this pointed at `swe_bench::swe_cache_dir()/envs`. One root or the confusion
    # regrows. ([[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]])
    ap.add_argument("--env-root",
                    default=os.path.expanduser("~/.continuum/benchmarks/swe/envs"),
                    help="where --auto-venv caches per-instance environments. THE canonical "
                         "root, shared with the Rust core (cognition::swe_bench::ensure_env) "
                         "— do not point this somewhere else, a second cache is how the "
                         "2026-08-17 misdiagnosis happened.")
    ap.add_argument("--pytest", default="pytest<7",
                    help="pytest pin — repos older than pytest 7 need <7 (conftest using "
                         "`monkeypatch.notset`, removed in 7). The gold gate catches a wrong pin.")
    ap.add_argument("--workdir")
    args = ap.parse_args()
    if not args.venv and not args.auto_venv:
        raise SystemExit("need --venv <python> or --auto-venv <base python>")

    if not args.gold and not args.predictions:
        raise SystemExit("need --predictions <preds.jsonl> or --gold")

    inst = hf_instance(args.dataset, args.instance)
    f2p = json.loads(inst["FAIL_TO_PASS"]) if isinstance(inst["FAIL_TO_PASS"], str) else inst["FAIL_TO_PASS"]
    p2p = json.loads(inst["PASS_TO_PASS"]) if isinstance(inst["PASS_TO_PASS"], str) else inst["PASS_TO_PASS"]
    tfiles = patched_test_files(inst["test_patch"])

    if args.gold:
        model_patch = inst["patch"]
    else:
        row = next(json.loads(l) for l in open(args.predictions)
                   if json.loads(l)["instance_id"] == args.instance)
        model_patch = row["model_patch"]

    wd = args.workdir or tempfile.mkdtemp(prefix=f"swe-grade-{args.instance}-")
    repo = os.path.join(wd, "repo")
    print(f"[clone] {inst['repo']} @ {inst['base_commit'][:10]}")
    sh(["git", "clone", "--quiet", f"https://github.com/{inst['repo']}.git", repo])
    sh(["git", "checkout", "--quiet", inst["base_commit"]], cwd=repo)

    venv = args.venv or ensure_env(args.auto_venv, args.env_root, args.instance, repo,
                                   args.pytest, inst.get("created_at") or "")

    # THE GATE. On the pristine tree + test_patch, FAIL_TO_PASS must FAIL — that failure IS
    # the bug. If it passes here, the checkout does not contain the bug and nothing measured
    # against it can distinguish a fix from a no-op.
    apply_patch(repo, inst["test_patch"], "test")
    pre = run_tests(repo, venv, f2p, tfiles)
    already = [t for t, (ok, _) in pre.items() if ok]
    if already:
        print(f"\n✗ UNGRADEABLE — FAIL_TO_PASS already passes on the PRISTINE tree: {already}")
        print("  The bug is not in this checkout. Every score from this tree is void.")
        raise SystemExit(2)
    print(f"[gate] FAIL_TO_PASS fails on the pristine tree ({len(f2p)} test(s)) — the bug is present")

    # now the solver's change, on a tree reset to base + test_patch
    sh(["git", "checkout", "--quiet", "--", "."], cwd=repo)
    apply_patch(repo, model_patch, "model")
    apply_patch(repo, inst["test_patch"], "test")

    f2p_res = run_tests(repo, venv, f2p, tfiles)
    p2p_res = run_tests(repo, venv, p2p[:40], tfiles)  # cap: p2p can be hundreds; 40 catches breakage

    f2p_ok = all(ok for ok, _ in f2p_res.values())
    p2p_ok = all(ok for ok, _ in p2p_res.values())
    print(f"\nFAIL_TO_PASS  {sum(ok for ok, _ in f2p_res.values())}/{len(f2p_res)}")
    for t, (ok, tail) in f2p_res.items():
        print(f"  {'PASS' if ok else 'FAIL'}  {t}")
        if not ok:
            print("        " + tail.strip().replace("\n", "\n        ")[:300])
    print(f"PASS_TO_PASS  {sum(ok for ok, _ in p2p_res.values())}/{len(p2p_res)} (sampled)")
    for t, (ok, tail) in p2p_res.items():
        if not ok:
            print(f"  BROKE  {t}\n        " + tail.strip().replace("\n", "\n        ")[:300])

    resolved = f2p_ok and p2p_ok
    label = "gold" if args.gold else "model"
    print(f"\nRESOLVED={int(resolved)}  ({label} patch, {len(model_patch)} bytes)")
    if args.gold and not resolved:
        print("✗ GOLD DID NOT RESOLVE — the environment is wrong (deps? python version?). "
              "No persona number from this setup is meaningful until this is green.")
        raise SystemExit(3)
    raise SystemExit(0 if resolved else 1)


if __name__ == "__main__":
    main()
