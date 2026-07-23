#!/usr/bin/env python3
"""
run_ours.py — the Continuum SWE-bench runner (EDGE benchmark harness; scoring rides the
official `swebench` Docker harness, the persona side rides `cu` into the Rust core).

Per instance: clone repo @ base_commit → the SOLVER edits the working tree → `git diff` is the
model_patch → write predictions.jsonl → score with the official harness (the industry yardstick).

Solvers (apples-to-apples, same instances, same scorer):
  --solver gold     apply the dataset's gold patch (SPINE CHECK — must resolve; validates our
                    clone→edit→diff→predictions path before we trust any real solver's number)
  --solver ours     point a Continuum persona's hands at the clone (create-workspace) and run
                    her cognition on the problem_statement  [wired next]
"""
import argparse, json, os, subprocess, sys, tempfile, urllib.request

def hf_instance(dataset, iid):
    # scan the dataset in pages for the instance (datasets-server caps length=100)
    for off in range(0, 400, 100):
        url = (f"https://datasets-server.huggingface.co/rows?dataset={urllib.parse.quote(dataset,safe='')}"
               f"&config=default&split=test&offset={off}&length=100")
        rows = json.load(urllib.request.urlopen(url, timeout=30)).get("rows", [])
        for r in rows:
            if r["row"]["instance_id"] == iid:
                return r["row"]
    raise SystemExit(f"instance {iid} not found in {dataset}")

def sh(cmd, cwd=None, check=True):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if check and r.returncode != 0:
        raise SystemExit(f"cmd failed: {' '.join(cmd)}\n{r.stdout}\n{r.stderr}")
    return r

def clone_at(repo, base_commit, dest):
    sh(["git", "clone", f"https://github.com/{repo}.git", dest])
    sh(["git", "checkout", "-q", base_commit], cwd=dest)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", default="princeton-nlp/SWE-bench_Lite")
    ap.add_argument("--instance", required=True)
    ap.add_argument("--solver", choices=["gold", "ours", "agent", "team"], default="gold")
    ap.add_argument("--base-model", default="unsloth/Devstral-Small-2507-GGUF",
                    help="agent solver: the base model the persona is measured on")
    ap.add_argument("--max-acts", type=int, default=40)
    ap.add_argument("--workdir", default=None)
    args = ap.parse_args()

    inst = hf_instance(args.dataset, args.instance)
    wd = args.workdir or tempfile.mkdtemp(prefix="swe-ours-")
    repo_dir = os.path.join(wd, "repo")
    print(f"[clone] {inst['repo']} @ {inst['base_commit'][:10]} -> {repo_dir}")
    clone_at(inst["repo"], inst["base_commit"], repo_dir)

    if args.solver == "gold":
        # simulate a CORRECT solver edit by applying the gold patch to the working tree
        p = os.path.join(wd, "gold.patch"); open(p, "w").write(inst["patch"])
        sh(["git", "apply", p], cwd=repo_dir)
    elif args.solver in ("agent", "team"):
        # AGENT: the whole-being agent/solve battery harness (drive_to_settle + idiom-lifted
        # tool parsing + workspace grounding) — the path that scores 87-93% on the t1 battery,
        # vs the older cognition/eval exam framing that looped ([[eval-is-an-exam-not-a-life]]).
        import time
        CU = next(p for p in (
            os.path.expanduser("~/.continuum/cache/cargo-target/release/cu"),
            os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu"),
        ) if os.path.exists(p))
        pr = subprocess.run([CU, "cognition/personas"], capture_output=True, text=True)
        personas = (json.loads(pr.stdout).get("personas") or []) if pr.stdout.strip().startswith("{") else []
        if not personas:
            raise SystemExit("no resident persona (core booted?) — cannot run --solver agent")
        pid = personas[0]["persona_id"]
        print(f"[persona] {personas[0].get('name')} ({pid})")
        run_id = f"swe-{args.instance}"
        task = (
            "You are ALREADY in the task's workspace: a real git repository with a real bug. "
            "Do not create a new workspace and do not create new top-level files — find the "
            "existing source with code/search and code/read, and fix it IN PLACE with code/edit. "
            "Run checks with code/shell if useful. The fix must land in the existing files.\n\n"
            f"ISSUE:\n{inst['problem_statement']}"
        )
        ledger = os.path.expanduser(f"~/.continuum/progress/agent-solve-{run_id}.json")
        if os.path.exists(ledger):
            os.remove(ledger)
        print(f"[agent] dispatching {run_id} (workspace={repo_dir}, max_acts={args.max_acts}, detached)")
        sh([CU, "agent/solve", "--persona-id", pid, "--base-model-id", args.base_model,
            "--task", task, "--workspace", repo_dir, "--max-acts", str(args.max_acts),
            "--detach", "true", "--run-id", run_id], check=False)
        # fire-and-poll (#86): the drive outlives any socket timeout; the ledger is the result
        for _ in range(120):
            time.sleep(30)
            if os.path.exists(ledger):
                led = json.load(open(ledger))
                print(f"[agent] landed: acts={led.get('acts')} failed={led.get('failed')} "
                      f"error={led.get('error') or '-'}")
                break
        else:
            print("[agent] WARN: ledger did not land in 60min window (may still be running)")
        if args.solver == "team" and len(personas) > 1:
            # TEAMWORK LEG: an INDEPENDENT persona reviews the working tree before
            # scoring — the reviewer catches exactly the solo failure classes the
            # first official cell exposed (duplicate/misplaced edit application,
            # never running the repo's own tests). Different persona = different
            # memory + genome; this is the team-vs-solo arm, never a self-review.
            reviewer = personas[1]
            rid = reviewer["persona_id"]
            print(f"[team] reviewer: {reviewer.get('name')} ({rid})")
            review_run = f"swe-{args.instance}-review"
            review_ledger = os.path.expanduser(f"~/.continuum/progress/agent-solve-{review_run}.json")
            if os.path.exists(review_ledger):
                os.remove(review_ledger)
            diff_now = sh(["git", "diff"], cwd=repo_dir).stdout
            review_task = (
                "You are the CODE REVIEWER for a teammate's bug-fix in this git repository. "
                "Their change is the current uncommitted diff (run `git diff` with code/shell "
                "to see it). Review it against these failure classes and FIX what you find, "
                "in place:\n"
                "1. DUPLICATE or misplaced applications of the same logical change — keep "
                "exactly one, at the correct site, remove the rest.\n"
                "2. Broken/leftover code the edit introduced (a stray duplicated function or "
                "class member, unbalanced blocks).\n"
                "3. Then RUN the repository's own test suite with code/shell (e.g. "
                "`python -m pytest -x -q` or the tests for the touched file) and fix "
                "failures your teammate's change caused.\n"
                "Do NOT redesign the fix or expand scope — polish it until the tests pass.\n\n"
                f"The bug being fixed:\n{inst['problem_statement'][:1500]}"
            )
            sh([CU, "agent/solve", "--persona-id", rid, "--base-model-id", args.base_model,
                "--task", review_task, "--workspace", repo_dir,
                "--max-acts", str(args.max_acts), "--detach", "true",
                "--run-id", review_run], check=False)
            for _ in range(120):
                time.sleep(30)
                if os.path.exists(review_ledger):
                    led = json.load(open(review_ledger))
                    print(f"[team] review landed: acts={led.get('acts')} "
                          f"failed={led.get('failed')} error={led.get('error') or '-'}")
                    break
            else:
                print("[team] WARN: review did not land in window")
            diff_after = sh(["git", "diff"], cwd=repo_dir).stdout
            print(f"[team] diff {len(diff_now)}B -> {len(diff_after)}B after review")
    else:
        # OURS: point a Continuum persona at the clone and run her cognition on the issue.
        # Her file engine is rooted at the clone DETERMINISTICALLY by cognition/eval's
        # --workspace_root seam (#49) — it invokes code/create-workspace through her own
        # identity-bearing executor before her cycle, so we no longer rely on the MODEL
        # choosing to call create-workspace itself (that was the 0-byte-diff failure mode).
        # Grading is EXTERNAL (the git diff → official harness), so the eval's own dod is a
        # no-op; we only need her to ACT on the working tree.
        import time
        CU=os.path.expanduser("~/.continuum/cache/cargo-target/debug/cu")
        # resolve the resident persona LIVE from the booted core — never a baked UUID
        # (only exists on one machine; breaks every other install).
        pr=subprocess.run([CU,"cognition/personas"],capture_output=True,text=True)
        personas=(json.loads(pr.stdout).get("personas") or []) if pr.stdout.strip().startswith("{") else []
        if not personas: raise SystemExit("no resident persona (core booted?) — cannot run --solver ours")
        ASHA=personas[0]["persona_id"]; print(f"[persona] {personas[0].get('name')} ({ASHA})")
        note=f"swe-{args.instance}"
        task={"id":args.instance,
              "prompt":(f"You are fixing a real bug in a git repository. Your workspace is ALREADY rooted at "
                        f"the repo — just use your tools on it directly. Use code/search and code/read to find "
                        f"the relevant source, and code/edit to fix it IN PLACE (do NOT create new top-level "
                        f"files; edit the existing source). Run checks with code/shell if useful. When done, the "
                        f"working tree should contain your fix.\n\nISSUE:\n{inst['problem_statement']}"),
              "dod_shell":"true","lang":"python"}  # SWE-bench Lite repos are Python — wrong lang primes her to search **/*.rs → 0 matches → search loop (glass-boxed)
        tf=os.path.join(wd,"task.jsonl"); open(tf,"w").write(json.dumps(task)+"\n")
        led=os.path.expanduser(f"~/.continuum/progress/{ASHA}.jsonl")
        n0=sum(1 for _ in open(led)) if os.path.exists(led) else 0
        cap=os.path.join(wd,"capture")  # glass-box: her per-tick bids+DECISION land in <cap>/<persona>.jsonl
        print(f"[ours] dispatching persona on {args.instance} (workspace rooted at clone, capture→{cap}, detached, max_acts=25)")
        sh([CU,"cognition/eval","--persona_id",ASHA,"--eval_set",tf,"--workspace_root",repo_dir,
            "--capture_dir",cap,"--max_acts","25","--note",note,"--detach","true"],check=False)
        # poll the ledger for this run to land (she is editing the clone in the background)
        for _ in range(40):
            time.sleep(30)
            if os.path.exists(led):
                rows=[json.loads(l) for l in open(led) if l.strip()]
                if any(r.get("note")==note for r in rows[n0:]): print("[ours] persona run landed"); break
        else:
            print("[ours] WARN: run did not land in ledger window (may still be editing)")

    # the model_patch is whatever the solver changed in the working tree
    diff = sh(["git", "diff"], cwd=repo_dir).stdout
    print(f"[diff] {len(diff)} bytes changed")
    preds = os.path.join(wd, "preds.jsonl")
    open(preds, "w").write(json.dumps({
        "instance_id": args.instance,
        "model_patch": diff,
        "model_name_or_path": f"continuum-{args.solver}",
    }) + "\n")
    print(f"[preds] {preds}")
    print(f"NEXT: score with the official harness:\n  python -m swebench.harness.run_evaluation "
          f"--dataset_name {args.dataset} --predictions_path {preds} --max_workers 1 "
          f"--run_id ours-{args.solver} --instance_ids {args.instance}")
    print(preds)  # last line = preds path for the caller

if __name__ == "__main__":
    import urllib.parse
    main()
