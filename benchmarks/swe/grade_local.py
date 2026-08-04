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
import argparse, json, os, subprocess, sys, tempfile, urllib.parse, urllib.request


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


def run_tests(repo, venv_py, tests):
    """Each test id run individually: one collection error must not mask the others."""
    env = dict(os.environ, PYTHONPATH=os.path.join(repo, "src"), PYTHONDONTWRITEBYTECODE="1")
    out = {}
    for t in tests:
        r = sh([venv_py, "-m", "pytest", t, "-q", "--no-header", "-p", "no:cacheprovider"],
               cwd=repo, check=False, env=env)
        out[t] = (r.returncode == 0, (r.stdout + r.stderr)[-400:])
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
    ap.add_argument("--venv", required=True, help="python interpreter with pytest + the repo's deps")
    ap.add_argument("--workdir")
    args = ap.parse_args()

    if not args.gold and not args.predictions:
        raise SystemExit("need --predictions <preds.jsonl> or --gold")

    inst = hf_instance(args.dataset, args.instance)
    f2p = json.loads(inst["FAIL_TO_PASS"]) if isinstance(inst["FAIL_TO_PASS"], str) else inst["FAIL_TO_PASS"]
    p2p = json.loads(inst["PASS_TO_PASS"]) if isinstance(inst["PASS_TO_PASS"], str) else inst["PASS_TO_PASS"]

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

    # THE GATE. On the pristine tree + test_patch, FAIL_TO_PASS must FAIL — that failure IS
    # the bug. If it passes here, the checkout does not contain the bug and nothing measured
    # against it can distinguish a fix from a no-op.
    apply_patch(repo, inst["test_patch"], "test")
    pre = run_tests(repo, args.venv, f2p)
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

    f2p_res = run_tests(repo, args.venv, f2p)
    p2p_res = run_tests(repo, args.venv, p2p[:40])  # cap: p2p can be hundreds; 40 catches breakage

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
