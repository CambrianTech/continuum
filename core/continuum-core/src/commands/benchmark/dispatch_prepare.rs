//! Preparing a dispatch: the run-room name, roster resolution, card selection and preparation (eval-set and gym cards), SWE environment prewarm, and the substrate-executor accessor on `BenchmarkDispatch`.

use crate::sdk_codegen::{CommandError};
use super::*;

/// The room a run lands in when the caller names none: `bench-<benchmark>-<epoch>`.
///
/// Named for the ACTIVITY and stamped so it is THIS run — the naming rule
/// `activity/spawn` documents, and the reason it matters here specifically: a room named
/// for a SUBSYSTEM (`#academy`, `#benchmarks`) never finishes, so it reads as a permanent
/// place and quietly becomes the room every run reuses forever. That is precisely the
/// 136-card pile this parameter exists to end.
///
/// Flattened with `-` rather than the `academy/bench/<run>` path form the design of record
/// uses, because airc channel names accept only `[a-z0-9_-]` (`ChannelName::new` rejects
/// `/`). The tree is a naming convention waiting on a channel-name grammar, not something
/// this function can invent unilaterally.
pub(super) fn default_run_room_name(benchmark: &str, epoch_secs: u64) -> String {
    let slug: String = benchmark
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("bench-{slug}-{epoch_secs}")
}

/// Resolve the citizens a directed dispatch addresses — GENERALIZED for any repo user's
/// roster, never our specific names. Pure over the snapshots so it is unit-testable
/// without a running airc daemon (a real `PersonaSlot` needs one); the wrapper in `run`
/// feeds `registry.resident_snapshot()` and `registry.roster_snapshot()` in.
///
/// - `requested` empty → the WHOLE RESIDENT roster (whoever THIS machine has in the room).
///   This is the "dispatch to my citizens, whoever they are" default: a fresh clone runs
///   `benchmark/dispatch --name=…` with no `--assignees` and it targets their own resident
///   citizens. Directed dispatch is what actuates (a silent card does not — measured
///   2026-08-07), so defaulting to the resident roster keeps the loop autonomous everywhere.
/// - `requested` non-empty → every name MUST resolve to a RESIDENT citizen; anything else
///   FAILS LOUD listing who is resident (never silently addresses a citizen who cannot
///   hear, and never silently skips SWE staging). Order is preserved for round-robin.
/// - nobody resident → `Denied`, and the message distinguishes "not spawned" (fix:
///   `persona/spawn`) from "spawned but not hosted yet" (fix: wait — see below).
///
/// `live` is the RESIDENT snapshot (service loop attached and running), NOT the registered
/// roster — `registered` is passed alongside it purely so a refusal can tell the operator
/// WHICH of the two states they are in. That distinction is the whole point:
///
/// - registered ∧ ¬resident → she exists but has no perception stream. Hosting is waiting
///   on something (usually a serving lane proving it can decode, #363). Work staged now is
///   posted into an empty room and never worked.
/// - ¬registered → nobody spawned her. The fix is `persona/spawn`, a different action.
///
/// Measured 2026-08-18: dispatching against the REGISTERED roster in the first state
/// reported `dispatched: 2, kickoffs: 2, kickoff_errors: []` and produced zero turns.
pub(super) fn resolve_dispatch_roster(
    live: &[(String, uuid::Uuid)],
    registered: &[(String, uuid::Uuid)],
    requested: &[String],
) -> Result<Vec<(String, uuid::Uuid)>, CommandError> {
    if live.is_empty() {
        if registered.is_empty() {
            return Err(CommandError::Denied(
                "no citizens are online to work the cards — spawn a persona (persona/spawn) \
                 first, then dispatch."
                    .to_string(),
            ));
        }
        let names: Vec<&str> = registered.iter().map(|(n, _)| n.as_str()).collect();
        return Err(CommandError::Denied(format!(
            "citizen(s) [{}] are registered but NOT RESIDENT — no service loop, so no \
             perception stream, so nothing dispatched here would be heard. Hosting is \
             normally waiting on the serving lane to prove it can decode; watch \
             `inference.lane_relaunch_retry` and `persona.inbound.subscribe_opened`, and \
             re-dispatch once `persona/roster` reports resident_count > 0. Staging a round \
             into this window posts cards nobody can see.",
            names.join(", "),
        )));
    }
    if requested.is_empty() {
        return Ok(live.to_vec());
    }
    let mut resolved = Vec::with_capacity(requested.len());
    let mut unknown = Vec::new();
    for name in requested {
        match live.iter().find(|(n, _)| n == name) {
            Some(pair) => resolved.push(pair.clone()),
            None => unknown.push(name.clone()),
        }
    }
    if !unknown.is_empty() {
        let online: Vec<&str> = live.iter().map(|(n, _)| n.as_str()).collect();
        // Name the registered-but-not-resident case separately: "not online" reads as a
        // typo, and sending an operator hunting for a misspelling when the real answer is
        // "she is here but not hosted yet" is the same lie in a smaller box.
        let not_resident: Vec<&str> = unknown
            .iter()
            .filter(|n| registered.iter().any(|(r, _)| r == *n))
            .map(|s| s.as_str())
            .collect();
        let residency_note = if not_resident.is_empty() {
            String::new()
        } else {
            format!(
                " NOTE: [{}] are registered but not resident — they exist, they just have \
                 no service loop yet (hosting is likely waiting on a serving lane). That \
                 is a wait, not a typo.",
                not_resident.join(", ")
            )
        };
        return Err(CommandError::Invalid(format!(
            "assignee(s) not resident: {}. Citizens resident right now: [{}]. Pass names \
             from that list, or omit --assignees to dispatch to all of them.{}",
            unknown.join(", "),
            online.join(", "),
            residency_note,
        )));
    }
    Ok(resolved)
}

    pub(crate) enum CardWork {
        /// Gym: write a solution file; DoD is a compile/test shell.
        Gym { solution_file: String },
        /// SWE: the pulled instance. The CLAIM stages it into the claimer's
        /// workspace/swe/<instance> (`card_staging`); a detached-solve round stages
        /// its directed assignee at dispatch through the same function and fires
        /// her solve (#346) — the loop closes with nobody in it.
        Swe {
            instance: Box<crate::cognition::swe_bench::SweInstance>,
        },
    }
    pub(crate) struct PreparedCard {
        pub(crate) title: String,
        pub(crate) body: String,
        pub(crate) work: CardWork,
        /// Gym-only: a task needing a workspace re-break the card can't orchestrate yet.
        pub(crate) needs_setup: bool,
        /// The gym task's workspace-preparation shell, staged into the ASSIGNEE's
        /// workspace at dispatch (same contract as SWE checkout staging below) --
        /// idempotent by adapter convention (mkdir -p + overwrite-decode).
        pub(crate) setup_shell: Option<String>,
    }

/// What one dispatch SELECTS from a suite. Pure data, so the same selection can be
/// replayed: `(suite, seed, sample)` is the replication contract the score is
/// published with, and `instances` is the explicit alternative to it.
#[derive(Debug, Clone, Default)]
pub(crate) struct CardSelection {
    pub(crate) instances: Option<Vec<String>>,
    pub(crate) sample: Option<u32>,
    pub(crate) seed: Option<u64>,
    /// Keep only instances whose harness speaks this language (`rust`, `go`, `php`,
    /// `ruby`, `javascript`, `java`, `c`; `python` for the SWE-bench family) — the
    /// Multilingual dataset mixes nine, and a round is one language's board.
    /// `None`/empty = every language.
    pub(crate) language: Option<String>,
}

/// THE ONE WRITER of benchmark cards: task + oracle only, projected into the
/// title/body a citizen sees and the grader keys on (`[bench <suite>] <task>: …`).
/// No side effects — no room, no board, no env pre-warm — so `benchmark/import`
/// (the recipe path, S4) and `benchmark/dispatch` (the Rust path) post IDENTICAL
/// cards for the same selection by construction, not by a test that compares them.
pub(crate) async fn prepare_cards(
    spec: &BenchmarkSpec,
    selection: &CardSelection,
) -> Result<Vec<PreparedCard>, CommandError> {
    let prepared: Vec<PreparedCard> = if let Some(dataset) = spec.swe_dataset() {
        // Real-project tier: pull instances on demand (cached on first use), one
        // full-project card each. Reuses the SAME loader agent/solve grades against —
        // no second source of truth. THIS is what killed the "no runnable eval_set"
        // refusal that had blocked every SWE dispatch (Joel: "fix the goddamn thing").
        let mut instances = crate::cognition::swe_bench::load_dataset(dataset)
            .await
            .map_err(|e| {
                CommandError::Internal(format!("swe dataset '{dataset}' load failed: {e}"))
            })?;
        if let Some(lang) = selection.language.as_deref().filter(|l| !l.is_empty()) {
            let before = instances.len();
            instances.retain(|i| i.language() == lang);
            if instances.is_empty() {
                return Err(CommandError::Invalid(format!(
                    "no instance in '{dataset}' is `{lang}` (of {before}) — a language is \
                     what an instance's harness speaks: rust, go, php, ruby, javascript, \
                     java, c; python for the SWE-bench family"
                )));
            }
        }
        // Deterministic sample: (dataset, seed, n) → the same list on every
        // machine. Fisher-Yates over the dataset order with the shared LCG —
        // the command IS the replication recipe (no operator-side scripts).
        if let Some(n) = selection.sample.filter(|n| *n > 0) {
            if selection.instances.as_ref().is_some_and(|w| !w.is_empty()) {
                return Err(CommandError::Invalid(
                    "pass either `sample` (seeded random) or `instances` (explicit list),                          not both — they are competing selection recipes"
                        .into(),
                ));
            }
            let n = (n as usize).min(instances.len());
            let mut rng = crate::cognition::gym_rng::Lcg::new(selection.seed.unwrap_or(0)); // documented default seed: 0 is part of the replication contract, not a guess
            for i in 0..n {
                let j = i + rng.next(instances.len() - i);
                instances.swap(i, j);
            }
            instances.truncate(n);
            let list: Vec<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            tracing::info!(
                probe_class = "benchmark.dispatch.sample",
                dataset,
                n,
                seed = selection.seed.unwrap_or(0), // same documented default as the draw above
                instances = ?list,
                "seeded sample selected — publish (dataset, seed, n) with the score"
            );
        }
        // Caller-targeted instances win over dataset order — select by substring (so a
        // short id resolves) and preserve the CALLER's ordering, fail loud on a miss so a
        // typo never silently dispatches the wrong (or whole) set.
        if let Some(wanted) = selection.instances.as_ref().filter(|w| !w.is_empty()) {
            let mut picked: Vec<crate::cognition::swe_bench::SweInstance> = Vec::new();
            for want in wanted {
                match instances
                    .iter()
                    .position(|i| i.instance_id.contains(want.as_str()))
                {
                    Some(idx) => picked.push(instances.remove(idx)),
                    None => {
                        return Err(CommandError::Invalid(format!(
                            "no instance in '{dataset}' matches '{want}' — check the id (e.g. sympy__sympy-24152)"
                        )));
                    }
                }
            }
            instances = picked;
        }
        // ENV PRE-WARM (background): a cold native build (scikit's cython,
        // matplotlib's freetype) mid-round burns a solve attempt on an ENV
        // failure and reads as a model miss. Build every instance's env
        // AHEAD of the driver, in REVERSE card order so the warmer and the
        // solver approach from opposite ends (ensure_env itself holds the
        // per-instance lock, so even a meeting in the middle is safe).
        // Fire-and-forget: a prewarm failure is probed — the SAME failure
        // the solve would hit, surfaced hours earlier and attributable.
        instances
            .into_iter()
            .map(|i| PreparedCard {
                title: dispatch_card_title(spec.name, &i.instance_id, &i.problem_statement),
                body: dispatch_swe_card_body(spec.name, &i),
                needs_setup: false,
                setup_shell: None,
                work: CardWork::Swe {
                    instance: Box::new(i),
                },
            })
            .collect()
    } else {
        let reference = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' has no runnable eval_set yet — it is catalogued but its \
                 task collection hasn't been pulled/committed (see benchmark/list `runnable`)",
                spec.name
            ))
        })?;
        prepare_gym_cards(spec.name, reference)?
    };
    Ok(prepared)
}

/// The eval set a gym suite's cards resolve their oracle from, by the suite name a card title
/// carries: a registered suite's `eval_set`, or a coursework set's content-addressed file
/// (`cognition::coursework`). The ONE resolver import, preparation and the grader share, so a
/// coursework card is graded from its title exactly like a registered gym card. `None` = not a
/// gym suite this node can resolve.
pub(crate) fn eval_set_reference(suite: &str) -> Option<String> {
    if let Some(spec) = known_benchmarks().iter().find(|b| b.name == suite) {
        return spec.eval_set.map(str::to_string);
    }
    let home = crate::paths::continuum_home().ok()?;
    crate::cognition::coursework::set_path(&home, suite).map(|p| p.display().to_string())
}

/// One gym card per task of the eval set at `reference`, titled under `suite`: the gym arm of
/// [`prepare_cards`], shared with coursework so a lesson card is written by the same writer the
/// grader's parser reads.
pub(crate) fn prepare_gym_cards(suite: &str, reference: &str) -> Result<Vec<PreparedCard>, CommandError> {
    // Same fail-loud task loading as cognition/eval: the committed gym resolves
    // from the embedded registry, a malformed line names itself.
    let (origin, text) =
        crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty())
        .map(|(n, l)| {
            let mut t: crate::cognition::eval::EvalTask = serde_json::from_str(l).map_err(|e| {
                CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
            })?;
            // Title gist comes from the AUTHORED prompt: require_hands_for_code
            // prepends the same write-and-verify preamble to every code task, and a
            // board of 12 cards all titled "Implement the following, and VERIFY…"
            // is unscannable for the citizen AND breaks dispatch_card_key parsing.
            let headline = t.prompt.clone();
            // THE artifact rule — the same normalization cognition/eval applies at
            // load. Before this, the card body named NO file (the gym rows carry no
            // solution_file) while the grade read the derived one: the citizen was
            // graded against a path she was never told. One derivation, both readers.
            t.require_hands_for_code();
            let solution_file = t
                .solution_file
                .clone()
                .unwrap_or_else(|| format!("{}.rs", t.id));
            Ok(PreparedCard {
                title: dispatch_card_title(suite, &t.id, &headline),
                body: dispatch_card_body(suite, &t),
                needs_setup: t.setup_shell.is_some(),
                setup_shell: t.setup_shell.clone(),
                work: CardWork::Gym { solution_file },
            })
        })
        .collect::<Result<_, CommandError>>()
}

/// Warm the SWE envs a dispatch is about to hand out — dispatch's side effect, kept
/// out of [`prepare_cards`] so the import verb stays pure. Best-effort by design.
pub(super) fn prewarm_swe_envs(prepared: &[PreparedCard]) {
    let warm: Vec<crate::cognition::swe_bench::SweInstance> = prepared
        .iter()
        .filter_map(|pc| match &pc.work {
            CardWork::Swe { instance } => Some((**instance).clone()),
            CardWork::Gym { .. } => None,
        })
        .collect();
    if warm.is_empty() {
        return;
    }
    tokio::spawn(async move {
        for inst in warm.into_iter().rev() {
                let checkout =
                    match crate::cognition::swe_bench::ensure_grade_checkout(&inst).await {
                        Ok(dir) => dir,
                        Err(e) => {
                            crate::probe!(
                                class = "benchmark.env.prewarm_failed",
                                instance = %inst.instance_id,
                                stage = "checkout",
                                error = %e,
                                "env pre-warm could not stage a checkout — the solve \
                                 will hit this same wall; this is an ENV failure, not \
                                 a model result"
                            );
                            continue;
                        }
                    };
                match crate::cognition::swe_bench::ensure_env(&inst, &checkout).await {
                    Ok(_) => crate::probe!(
                        class = "benchmark.env.prewarmed",
                        instance = %inst.instance_id,
                        "env ready ahead of the driver"
                    ),
                    Err(e) => crate::probe!(
                        class = "benchmark.env.prewarm_failed",
                        instance = %inst.instance_id,
                        stage = "env",
                        error = %e,
                        "env pre-warm FAILED — the solve will hit this same wall; \
                         an ENV failure, never a model result"
                    ),
                }
        }
    });
}

impl BenchmarkDispatch {
    /// The substrate executor, or a loud error naming the wiring gap — never a
    /// silent no-op (the slot is installed by `install_executor_on_all` at boot).
    pub(super) fn executor(
        &self,
    ) -> Result<std::sync::Arc<crate::runtime::command_executor::CommandExecutor>, CommandError>
    {
        self.executor_slot.get().cloned().ok_or_else(|| {
            CommandError::Internal(
                "command executor not installed on benchmark/dispatch — boot wiring gap".into(),
            )
        })
    }

    /// Load a recipe row from the `benchmark_recipes` collection via `data/list`
    /// — the recipe is DATA authored with `data/create`, never code.
    pub(super) async fn load_recipe(
        &self,
        name: &str,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<BenchmarkRecipe, CommandError> {
        let exec = self.executor()?;
        let out = exec
            .execute(
                "data/list",
                serde_json::json!({
                    "collection": "benchmark_recipes",
                    "filter": { "name": name },
                    "limit": 1,
                }),
            )
            .await
            .map_err(|e| CommandError::Internal(format!("data/list failed: {e}")))?;
        let crate::runtime::CommandResult::Json(v) = out else {
            return Err(CommandError::Internal(
                "data/list returned a non-JSON result".into(),
            ));
        };
        let item = v
            .get("items")
            .and_then(|i| i.as_array())
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "no recipe named '{name}' in `benchmark_recipes` — author one with \
                     data/create (fields: name, description, model_id?, dispatches: \
                     [{{benchmark, instances[], limit?}}])"
                ))
            })?;
        // The ORM returns the entity ENVELOPE ({collection, id, data: {…}});
        // the recipe fields live under `data`. Tolerate both shapes so a raw
        // row (tests, future stores) parses identically.
        let item = item.get("data").cloned().unwrap_or(item); // unwrap_or: raw (non-enveloped) rows parse as themselves
        let item = Self::instantiate_recipe(item, params)?;
        serde_json::from_value::<BenchmarkRecipe>(item).map_err(|e| {
            CommandError::Invalid(format!(
                "recipe '{name}' exists but does not parse as a BenchmarkRecipe: {e}"
            ))
        })
    }

    /// TEMPLATE → INSTANCE: substitute `{key}` placeholders in every string of
    /// the recipe row from the caller's params (the recipe-doctrine split — a
    /// recipe is a reusable template; the invocation supplies the specifics).
    /// Pure so it is unit-testable; unresolved placeholders are a loud error
    /// naming the key, never a silently-literal "{model}" reaching dispatch.
    pub(super) fn instantiate_recipe(
        mut row: serde_json::Value,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<serde_json::Value, CommandError> {
        fn walk(
            v: &mut serde_json::Value,
            params: &std::collections::BTreeMap<String, String>,
            missing: &mut Vec<String>,
        ) {
            match v {
                serde_json::Value::String(s) => {
                    if s.contains('{') {
                        let mut out = s.clone();
                        for (k, val) in params {
                            out = out.replace(&format!("{{{k}}}"), val);
                        }
                        if let (Some(a), Some(b)) = (out.find('{'), out.find('}')) {
                            if a < b {
                                missing.push(out[a + 1..b].to_string());
                            }
                        }
                        *s = out;
                    }
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(|x| walk(x, params, missing)),
                serde_json::Value::Object(o) => {
                    o.values_mut().for_each(|x| walk(x, params, missing))
                }
                _ => {}
            }
        }
        let mut missing = Vec::new();
        walk(&mut row, params, &mut missing);
        if missing.is_empty() {
            Ok(row)
        } else {
            missing.sort();
            missing.dedup();
            Err(CommandError::Invalid(format!(
                "recipe placeholders unresolved: {{{}}} — pass them via --params",
                missing.join("}, {")
            )))
        }
    }

    /// Bring the lane to the recipe's model BEFORE any card fires: pin via the
    /// `serving/pin` COMMAND (its fit-gate refuses loud), then await readiness
    /// on the daemon's own snapshot — bounded, probed, never a silent hang.
    /// The lane's readiness smoke probe doubles as the first-request warmup
    /// models with a serving contract require.
    pub(super) async fn ensure_recipe_model(&self, recipe: &BenchmarkRecipe) -> Result<(), CommandError> {
        let Some(model_id) = recipe.model_id.as_deref() else {
            return Ok(()); // recipe runs on whatever is live
        };
        let live = crate::inference::llama_server::current_serving();
        if live.ready && live.active_model.as_deref() == Some(model_id) {
            return Ok(());
        }
        let exec = self.executor()?;
        exec.execute("serving/pin", serde_json::json!({ "model_id": model_id }))
            .await
            .map_err(|e| {
                CommandError::Denied(format!(
                    "recipe names model '{model_id}' but serving/pin refused: {e}"
                ))
            })?;
        // Await the swap. Generous bound: a cold multi-shard load is minutes.
        const RECIPE_SERVE_DEADLINE: std::time::Duration =
            std::time::Duration::from_secs(15 * 60);
        let started = std::time::Instant::now();
        loop {
            if let Some(s) = crate::inference::llama_server::await_ready_serving(
                std::time::Duration::from_secs(30),
            )
            .await
            {
                if s.ready && s.active_model.as_deref() == Some(model_id) {
                    crate::probe!(
                        class = "benchmark.recipe.model_ready",
                        model = model_id,
                        waited_ms = started.elapsed().as_millis() as u64,
                        "recipe's model is serving — dispatch proceeds"
                    );
                    return Ok(());
                }
            }
            if started.elapsed() > RECIPE_SERVE_DEADLINE {
                return Err(CommandError::Internal(format!(
                    "pinned '{model_id}' but it did not become the ready served model within \
                     {}s — check serving/status and the lane log",
                    RECIPE_SERVE_DEADLINE.as_secs()
                )));
            }
            crate::probe!(
                class = "benchmark.recipe.awaiting_model",
                model = model_id,
                waited_ms = started.elapsed().as_millis() as u64,
                "recipe model not ready yet — still awaiting the lane swap"
            );
        }
    }
}
