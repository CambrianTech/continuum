//! SUPER-Masked — Tier-1 pick #3 (AI2, EMNLP'24, Apache-2.0). 152 checkpointed
//! sub-scenarios of "make a real research repo actually run" — the truest
//! env-wrangling signal available at CPU cost (maintainers: ~2-3¢/task on CPU by
//! design), and the one whose partial-credit design keeps small models off the floor.
//!
//! # Oracle fidelity (checked against upstream before building, per the landscape
//! doc's own uncertainty register)
//!
//! Upstream `super/evaluate_dataset.py` grades TWO ways:
//! 1. `evaluate(predicted, gold, float_epsilon=1e-2)` — recursive structural match:
//!    floats within ABSOLUTE 1e-2, strings exact after strip, dicts averaged over
//!    gold keys, lists zip-averaged. Ported VERBATIM into this adapter's harness
//!    (same epsilon, same recursion, same dict-averaging).
//! 2. `evaluate_checkpoints(landmarks, history)` — regexes over the agent's
//!    OBSERVATION history, fraction hit. That needs her execution transcript, which
//!    lives in the substrate's act receipts, not in a dod shell — **deferred, and
//!    said so in every receipt** rather than silently dropped: the harness prints
//!    `LANDMARKS: not-evaluated (answer-only v1)`. Wiring landmarks over her
//!    tool-receipt history is the named follow-up (the corpus already records it).
//!
//! # Adaptation honesty (Masked semantics)
//!
//! Upstream Masked runs `pre_execute_cells` in the env BEFORE the agent starts. We
//! do not fake that: the cells are STAGED as `prior_work.py` and the prompt tells
//! her plainly these are the prior steps to run/adapt — environment execution is
//! part of the exam here (the benchmark's own thesis). Scores are therefore
//! "SUPER-Masked, answer-graded, self-executed-prior-work" — comparable to
//! ourselves over time, labeled honestly against upstream numbers.

use base64::Engine as _;

use crate::cognition::eval::EvalTask;

/// The one grading harness — upstream `evaluate()` ported verbatim.
const HARNESS_PY: &str = r#"import json, pathlib, sys

def evaluate(predicted, gold, float_epsilon=1e-2):
    if type(gold) == int: gold = float(gold)
    if type(predicted) == int: predicted = float(predicted)
    if type(gold) != type(predicted): return 0.0
    if type(gold) == list:
        if len(gold) == 0: raise ValueError("Gold is empty")
        return sum([evaluate(p, g) for p, g in zip(predicted, gold)]) / len(gold)
    if type(gold) == dict:
        if len(gold) == 0: raise ValueError("Gold is empty")
        return sum([evaluate(predicted.get(gk, None), gv, float_epsilon=float_epsilon)
                    for gk, gv in gold.items()]) / len(gold)
    if type(gold) == str: return float(predicted.strip() == gold.strip())
    if type(gold) == float: return float(abs(predicted - gold) < float_epsilon)
    raise NotImplementedError

gold = json.loads(pathlib.Path("gold.json").read_text())
try:
    pred = json.loads(pathlib.Path("answer.json").read_text())
except FileNotFoundError:
    print("super harness: answer.json not written yet", file=sys.stderr); sys.exit(3)
except json.JSONDecodeError as e:
    print(f"super harness: answer.json is not valid JSON: {e}", file=sys.stderr); sys.exit(2)
score = evaluate(pred, gold)
print(f"ANSWER-SCORE: {score:.3f}")
print("LANDMARKS: not-evaluated (answer-only v1)")
sys.exit(0 if score >= 0.999 else 1)
"#;

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

/// Extract the notebook-cell contents from the row's `pre_execute_cells` (a list of
/// JSON-encoded `{content: ...}` strings) into one runnable prior-work script.
pub fn prior_work_script(pre_execute_cells: &[serde_json::Value]) -> String {
    let mut out = String::from("# Prior work from the task's masked scaffold — run/adapt as needed.\n");
    for (i, cell) in pre_execute_cells.iter().enumerate() {
        let content = cell
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(str::to_string))
            .unwrap_or_else(|| cell.to_string()); // a cell that is not the {content} shape is kept RAW — she sees the original bytes, never a silently dropped step
        out.push_str(&format!("\n# --- cell {} ---\n{}\n", i + 1, content));
    }
    out
}

/// Project one Masked row onto the gym rails. Pure over the row.
pub fn to_eval_task(idx: usize, row: &serde_json::Value) -> Result<EvalTask, String> {
    let s = |k: &str| {
        row.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("super row {idx}: missing field '{k}'"))
    };
    let task_id = s("task_id")?;
    let repo = s("github_repo")?;
    let commit = s("git_commit")?;
    let query = s("query")?;
    let answer = s("answer")?;
    // Gold must parse as JSON now — a malformed gold graded later would produce a
    // permanent infra-zero wearing a capability-zero's face.
    serde_json::from_str::<serde_json::Value>(&answer)
        .map_err(|e| format!("super {task_id}: gold answer is not JSON: {e}"))?;
    let cells: Vec<serde_json::Value> = row
        .get("pre_execute_cells")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default(); // absent pre_execute_cells means an EMPTY scaffold (a real Masked state), not an error
    let dir = format!("super/{task_id}");
    Ok(EvalTask {
        id: format!("super-{task_id}"),
        prompt: format!(
            "[SUPER-Masked · {task_id}] A research-repo task. The repo is {repo} at commit \
             {commit} — clone it into your workspace ({dir}/repo) and work there. \
             `{dir}/prior_work.py` holds the scaffold's prior steps: run/adapt them first \
             (installing what they need — env-wrangling is part of this exam), then complete \
             the task below. Write your final answer as JSON to `{dir}/answer.json` in \
             EXACTLY the format the task specifies.\n\n{query}",
        ),
        dod_shell: Some(format!("cd {dir} && python3 grade.py")),
        solution_file: Some(format!("{dir}/answer.json")),
        setup_shell: Some(format!(
            "mkdir -p {dir} && printf '%s' '{gold}' | base64 -d > {dir}/gold.json && \
             printf '%s' '{grade}' | base64 -d > {dir}/grade.py && \
             printf '%s' '{prior}' | base64 -d > {dir}/prior_work.py",
            gold = b64(&answer),
            grade = b64(HARNESS_PY),
            prior = b64(&prior_work_script(&cells)),
        )),
        lang: Some("python".to_string()),
        ..Default::default()
    })
}

/// Fetch the Masked split and write the converted gym.
pub async fn materialize_gym(limit: Option<usize>) -> Result<(std::path::PathBuf, usize), String> {
    let mut lines: Vec<String> = Vec::new();
    let mut idx = 0usize;
    crate::cognition::swe_bench::stream_hf_rows("allenai/super", "Masked", "all_examples", |row| {
        if let Some(cap) = limit {
            if lines.len() >= cap {
                return Ok(());
            }
        }
        let task = to_eval_task(idx, row)?;
        idx += 1;
        lines.push(serde_json::to_string(&task).map_err(|e| format!("super serialize: {e}"))?);
        Ok(())
    })
    .await?;
    if lines.is_empty() {
        return Err("super: zero rows streamed — dataset unreachable or renamed".into());
    }
    let dir = crate::cognition::gym::gym_cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join("super-masked.jsonl");
    let tmp = path.with_extension("jsonl.tmp");
    std::fs::write(&tmp, lines.join("\n") + "\n").map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    Ok((path, lines.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> serde_json::Value {
        serde_json::json!({
            "task_id": "colbert_cpu",
            "github_repo": "https://github.com/stanford-futuredata/ColBERT",
            "git_commit": "852271661b22567e3720f2dd56b6d503613a3228",
            "query": "Train the model. Report as {\"last_loss\": 0.0}",
            "answer": "{\"last_loss\": 1.53119}",
            "pre_execute_cells": ["{\"content\": \"!git clone x\\n!pip install -e .\"}"],
            "landmarks": ["Training started"],
        })
    }

    // what this catches: gold-answer integrity at CONVERSION time. A gold that is
    // not JSON would grade every attempt zero forever, as an infra fault wearing a
    // capability face — refuse it while the row still names itself.
    #[test]
    fn a_non_json_gold_is_refused_at_conversion() {
        let mut r = row();
        r["answer"] = serde_json::json!("not json at all {");
        let err = to_eval_task(0, &r).unwrap_err();
        assert!(err.contains("colbert_cpu"), "{err}");
        assert!(err.contains("not JSON"), "{err}");
    }

    // what this catches: the wiring triangle (prompt path == solution_file == the
    // file the staged grader reads) plus the honesty markers: the harness must
    // carry upstream's 1e-2 epsilon AND declare landmarks not-evaluated — silently
    // dropping half the official oracle is how an adapted score becomes a lie.
    #[test]
    fn wiring_is_consistent_and_the_oracle_subset_is_declared() {
        let t = to_eval_task(0, &row()).unwrap();
        assert_eq!(t.solution_file.as_deref(), Some("super/colbert_cpu/answer.json"));
        assert!(t.prompt.contains("super/colbert_cpu/answer.json"));
        assert!(t.dod_shell.as_deref().unwrap().contains("grade.py"));
        assert!(HARNESS_PY.contains("float_epsilon=1e-2"), "upstream epsilon verbatim");
        assert!(HARNESS_PY.contains("LANDMARKS: not-evaluated"), "the deferred half must announce itself");
        // Prior-work cells decode into a runnable script, cell content extracted.
        let cells = row()["pre_execute_cells"].as_array().unwrap().clone();
        let script = prior_work_script(&cells);
        assert!(script.contains("!git clone x"), "{script}");
    }
}
