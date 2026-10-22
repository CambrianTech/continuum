use super::*;

impl BenchmarkDispatch {
    // ── RECIPE PATH: the whole experiment by name ──────────────────────
    // `--recipe X` loads a `benchmark_recipes` row (data, not code), pins
    // the model it names (fit-gated by serving/pin), awaits lane readiness,
    // then fires every dispatch entry through THIS SAME verb. The design
    // from the start: two primitives, commands composing commands.
    pub(super) async fn run_recipe(
        &self,
        ctx: &Ctx,
        p: BenchmarkDispatchParams,
        recipe_name: String,
    ) -> Result<BenchmarkDispatchResult, CommandError> {
        if p.name.is_some() || p.instances.is_some() {
            return Err(CommandError::Invalid(
                "pass either --recipe OR --name/--instances — the recipe row carries                      its own dispatches"
                    .into(),
            ));
        }
        let recipe = self
            .load_recipe(&recipe_name, &p.params.clone().unwrap_or_default()) // unwrap_or: absent params = empty map, placeholders then fail loud by name
            .await?;
        self.ensure_recipe_model(&recipe).await?;
        let mut agg: Option<BenchmarkDispatchResult> = None;
        for d in &recipe.dispatches {
            // ROLE → NAMES, at dispatch (recipe = content-type + RULES, #371):
            // `reviewers: N` resolves against the LIVE resident roster here, so
            // the recipe ships without citizen names. Merged with any explicit
            // `teammates`; a reviewer who lands as a card's assignee is skipped
            // per-card downstream (never their own reviewer). Short rosters
            // degrade loudly, never silently.
            let mut team: Vec<String> = d.teammates.clone();
            if let Some(n) = d.reviewers.filter(|n| *n > 0) {
                let picked: Vec<String> = match crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() {
                    Some(reg) => reg
                        .resident_snapshot()
                        .await
                        .into_iter()
                        .map(|(name, _)| name)
                        .filter(|name| !team.contains(name))
                        .take(n as usize)
                        .collect(),
                    None => Vec::new(),
                };
                if (picked.len() as u32) < n {
                    continuum_base::probe!(
                        class = "recipe.reviewers.short",
                        benchmark = %d.benchmark,
                        wanted = n as u64,
                        resolved = picked.len() as u64,
                        "recipe asked for more reviewers than the roster holds — team degrades, loudly"
                    );
                }
                team.extend(picked);
            }
            let sub = BenchmarkDispatchParams {
                name: Some(d.benchmark.clone()),
                recipe: None,
                doctrine: d.doctrine.clone().or_else(|| p.doctrine.clone()),
                review_gate: d.review_gate.or(p.review_gate),
                teammates: if team.is_empty() {
                    p.teammates.clone()
                } else {
                    Some(team)
                },
                instances: if d.instances.is_empty() {
                    None
                } else {
                    Some(d.instances.clone())
                },
                limit: d.limit.or(p.limit),
                ..p.clone()
            };
            let r = Box::pin(self.run(ctx, sub)).await?;
            agg = Some(match agg.take() {
                None => r,
                Some(mut a) => {
                    a.dispatched += r.dispatched;
                    a.card_ids.extend(r.card_ids);
                    a.skipped_needs_setup += r.skipped_needs_setup;
                    a.skipped_known_red += r.skipped_known_red;
                    a.skipped_already_on_board += r.skipped_already_on_board;
                    a.skipped_already_resolved += r.skipped_already_resolved;
                    a.pruned_duplicates += r.pruned_duplicates;
                    a.contended_tasks += r.contended_tasks;
                    a.kickoffs += r.kickoffs;
                    a.solves_fired += r.solves_fired;
                    a.kickoff_errors.extend(r.kickoff_errors);
                    a.benchmark = format!("recipe:{recipe_name}");
                    a
                }
            });
        }
        return agg.ok_or_else(|| {
            CommandError::Invalid(format!(
                "recipe '{recipe_name}' has no dispatches — author at least one"
            ))
        });
    }
}
