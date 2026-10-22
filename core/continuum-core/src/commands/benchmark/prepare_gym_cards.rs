use super::*;

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
                            continuum_base::probe!(
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
                    Ok(_) => continuum_base::probe!(
                        class = "benchmark.env.prewarmed",
                        instance = %inst.instance_id,
                        "env ready ahead of the driver"
                    ),
                    Err(e) => continuum_base::probe!(
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
                    continuum_base::probe!(
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
            continuum_base::probe!(
                class = "benchmark.recipe.awaiting_model",
                model = model_id,
                waited_ms = started.elapsed().as_millis() as u64,
                "recipe model not ready yet — still awaiting the lane swap"
            );
        }
    }
}
