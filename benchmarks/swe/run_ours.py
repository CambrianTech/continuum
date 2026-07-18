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
    ap.add_argument("--solver", choices=["gold", "ours"], default="gold")
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
