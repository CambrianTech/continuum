#![cfg(test)]

    // Every faculty's generation and render evidence returns through the real
    // cycle. The arbiter now forwards every bid, so this test does not exercise
    // dropped-bid behavior; collection still reads all_bids, not the broadcast.
    #[tokio::test]
    async fn the_cycle_returns_every_facultys_generation_and_render_receipts() {
        use crate::ai::provenance::GenerationReceipt;

        let mut loud = Contribution::context(FacultyId::Recall, "loud", 0.9, "wins attention");
        loud.receipts = vec![GenerationReceipt::faulted("req-loud", "served after retry")];
        let mut quiet = Contribution::context(FacultyId::WorldModel, "quiet", 0.1, "loses attention");
        quiet.receipts = vec![GenerationReceipt::faulted("req-quiet", "lane refused the model")];
        // A previous successful attempt can have rendered before this faculty's
        // later attempt faulted. Neither fact may disappear with its losing bid.
        let rendered = crate::cognition::render_receipt::RenderReceipt {
            owner: uuid::Uuid::new_v4(),
            incarnation: uuid::Uuid::new_v4(),
            request: "req-quiet-earlier-served".to_owned(),
            view: continuum_context::view::Receipt { epoch: 1, items: Vec::new() },
        };
        quiet.render_receipts.push(rendered.clone());

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(loud)),
            Arc::new(FixedFaculty(quiet)),
        ];
        let ws = cycle(faculties).run("burst").await;
        assert_eq!(ws.broadcast.len(), 2, "attention forwards every bid");
        assert_eq!(ws.render_receipts, vec![rendered]);

        let ids: Vec<&str> = ws
            .generation_receipts()
            .map(|r| r.submitted_request_id.as_str())
            .collect();
        assert!(
            ids.contains(&"req-quiet"),
            "the quieter faculty's receipt survives — provenance is not subject to \
             attention; got {ids:?}"
        );
        assert!(
            ids.contains(&"req-loud"),
            "the surviving faculty's receipt is kept too; got {ids:?}"
        );
    }
    use super::*;

    mod causality {
        use super::*;

        // what this catches: a `Cause` variant gaining a root it has no right to.
        // `Stimulus` is the ONLY antecedent — if `Ambient` or `Synthetic` ever
        // returned an id, the graph would grow `CausedBy` edges pointing at engrams
        // nothing admitted, which is worse than an unrooted chain: a fabricated cause
        // reads as evidence. `Cause::root` is the single place that decision lives, so
        // this pins it there.
        #[test]
        fn only_a_real_stimulus_is_ever_an_antecedent() {
            let id = Uuid::new_v4();
            assert_eq!(Cause::Stimulus(id).root(), Some(id));
            assert_eq!(
                Cause::Ambient.root(),
                None,
                "an idle tick has no admitted input — never invent one"
            );
            assert_eq!(
                Cause::Synthetic.root(),
                None,
                "a fixture has no live antecedent — never invent one"
            );
        }

        // what this catches: the SILENT default coming back. `cause` was once
        // `Option<Uuid>` defaulting to `None` at construction, which made "nothing
        // caused this" and "the author forgot" the same value — so the one live site
        // I wired by hand looked identical to the three I hadn't. Making it a required
        // constructor argument is the whole fix; this test fails to compile (not
        // merely fails) if someone reintroduces a defaulted builder, and asserts the
        // fixture constructors stay honestly Synthetic rather than quietly Ambient,
        // which would pollute the live ambient-rate measurement with test noise.
        #[test]
        fn a_burst_cannot_be_built_without_saying_what_caused_it() {
            let room = crate::identity::ActivityRoom::mint();
            let id = Uuid::new_v4();

            let live = Burst::from_turns_at(room, Vec::new(), Some(1), Cause::Stimulus(id));
            assert_eq!(live.cause.root(), Some(id));

            assert_eq!(
                Burst::from_turns(room, Vec::new()).cause,
                Cause::Synthetic,
                "the no-cause constructor is for fixtures and must say so"
            );
            assert_eq!(
                Burst::from("a raw string".to_string()).cause,
                Cause::Synthetic,
                "a raw string arrives through no channel at all"
            );
        }

        // what this catches: probe/receipt vocabulary drifting away from the variants,
        // which would silently break any grouping done on `cause=` rows.
        #[test]
        fn every_cause_reports_a_distinct_stable_tag() {
            let tags = [
                Cause::Stimulus(Uuid::new_v4()).as_str(),
                Cause::Ambient.as_str(),
                Cause::Synthetic.as_str(),
            ];
            assert_eq!(tags, ["stimulus", "ambient", "synthetic"]);
        }
    }

    /// A canned faculty for tests — fixed contribution + salience.
    struct FixedFaculty(Contribution);
    #[async_trait]
    impl Faculty for FixedFaculty {
        fn id(&self) -> FacultyId {
            self.0.faculty.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            Some(self.0.clone())
        }
    }

    /// A faculty that abstains this tick.
    struct AbstainFaculty(FacultyId);
    #[async_trait]
    impl Faculty for AbstainFaculty {
        fn id(&self) -> FacultyId {
            self.0.clone()
        }
        async fn contribute(&self, _ws: &Workspace) -> Option<Contribution> {
            None
        }
    }

    /// A deliberation faculty that CONDITIONS its decision on the assembled
    /// broadcast: it speaks (referencing what it saw) only if recall surfaced
    /// something; otherwise it passes. This is the probe for staged assembly —
    /// under the old single-pass join_all it would always see an empty broadcast
    /// and could never be informed by recall.
    struct ConditionalDeliberation;
    #[async_trait]
    impl Faculty for ConditionalDeliberation {
        fn id(&self) -> FacultyId {
            FacultyId::Deliberation
        }
        fn reacts_to_broadcast(&self) -> bool {
            true
        }
        async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
            // Look at the assembled context that won attention in phase 1.
            let recalled = ws.broadcast.iter().find(|c| c.faculty == FacultyId::Recall);
            match recalled {
                Some(mem) => Some(Contribution::verdict(
                    Decision::Speak {
                        text: format!("informed by recall: {}", mem.content),
                    },
                    0.9,
                    "conditioned the reply on the recalled context",
                )),
                None => Some(Contribution::verdict(
                    Decision::pass(),
                    0.5,
                    "blind — no context in the broadcast",
                )),
            }
        }
    }

    fn cycle(faculties: Vec<Arc<dyn Faculty>>) -> WorkspaceCycle {
        WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter))
    }

    // what this catches: the cycle's genome page-in/page-out round-trips through the
    // shared handle — page_in publishes a gene the next generation reads, page_out
    // reverts to the clean base. This is the A/B's lever (eval pages a gene in/out
    // around two passes); if it didn't round-trip, the candidate and base arms
    // would measure the same genome and every lift would read as zero.
    #[test]
    fn genome_page_in_and_out_round_trips() {
        let c = cycle(vec![]);
        assert!(
            c.genome().is_empty(),
            "a fresh cycle starts on the base model"
        );

        c.page_in(vec![ActiveAdapterRequest {
            name: "coder-0p5b".to_string(),
            path: "/genes/coder.gguf".to_string(),
            domain: String::new(),
            scale: 0.8,
        }]);
        let paged = c.genome();
        assert_eq!(paged.len(), 1, "the gene is now the active genome");
        assert_eq!(paged[0].name, "coder-0p5b");
        assert_eq!(paged[0].scale, 0.8);

        c.page_out();
        assert!(c.genome().is_empty(), "page_out reverts to the clean base");
    }

    // what this catches: the #186 acting tap lights the Act axis on the live cognition
    // compass THROUGH the cycle's public API — the exact path `act_observe::apply_act`
    // drives when a real tool batch executes. A resting cycle's compass is dark; a
    // `note_acting()` lights ONLY Act (idx 3 in the Focus/Reason/Recall/Act order the
    // radiator samples). If the tap regressed (wrong axis, or the pulse field dropped)
    // the tile's Act corner would never light while she runs tools.
    #[test]
    fn acting_lights_only_the_act_axis_on_the_compass() {
        let c = cycle(vec![]);
        assert_eq!(
            c.faculty_pulse().levels(),
            [0, 0, 0, 0],
            "a resting cycle's compass is dark"
        );
        c.note_acting();
        let levels = c.faculty_pulse().levels();
        assert!(levels[3] > 0, "acting lights the Act axis");
        assert_eq!(levels[0] + levels[1] + levels[2], 0, "and nothing else");
    }

    // what this catches: `rebind_model` writes THROUGH to the SAME shared binding
    // handle the deliberation faculty reads — the exact seam the served-model
    // re-home reconciler drives (ipc/mod.rs `re_home_all`). A live persona's model
    // page must swap the {adapter, model, context_window} triple the faculty will
    // budget its NEXT turn against; if the cycle held a separate handle from the
    // faculty, the swap would land on nothing and every re-home would be a silent
    // no-op (the multi-model sweep would keep answering as the boot-bound brain).
    // Also asserts the no-faculty cycle's `rebind_model` is a benign no-op (a
    // pure-cognition / test cycle shared no binding in).
    #[test]
    fn rebind_model_writes_through_the_shared_binding() {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        use crate::cognition::llm_deliberation_faculty::{model_binding, ModelBinding};

        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        // The handle the faculty would hold; the cycle shares the SAME Arc.
        let handle = model_binding(Arc::clone(&adapter), None, 4096);
        let c = cycle(vec![]).with_model_binding(Arc::clone(&handle));

        // Baseline: what boot bound.
        assert_eq!(handle.load().context_window, 4096);
        // A binding always names its model: `None` resolves to the adapter's served model at
        // bind time (one owner of the identity; every rate lookup is keyed by it).
        assert_eq!(handle.load().model.as_deref(), Some(adapter.default_model()));

        // A served-model change re-homes onto a new model + served window.
        c.rebind_model(ModelBinding {
            adapter: Arc::clone(&adapter),
            model: Some("qwen-coder-14b".to_string()),
            context_window: 8192,
        });

        // The faculty (holding the SAME handle) now sees the new binding.
        assert_eq!(
            handle.load().context_window,
            8192,
            "rebind must swap the served window through the shared handle"
        );
        assert_eq!(
            handle.load().model.as_deref(),
            Some("qwen-coder-14b"),
            "rebind must swap the model id through the shared handle"
        );

        // A cycle that shared no binding in (no deliberation faculty) must not
        // panic on re-home — it has nothing to rebind.
        let bare = cycle(vec![]);
        bare.rebind_model(ModelBinding {
            adapter,
            model: Some("whatever".to_string()),
            context_window: 2048,
        });
    }

    // what this catches: the "20k ctx, no model name" live-tile regression. On a
    // normal boot the upstart binds `model: None` (the shared served adapter
    // carries the active model as its DEFAULT), and the re-home reconciler adopts
    // the first served model as baseline WITHOUT a rebind — so the binding's
    // explicit id stays `None` forever on a stable host. `model_loadout` must
    // surface the EFFECTIVE model (explicit id, else the adapter's default — the
    // id the next generation actually requests) or every live roster LOADOUT
    // renders ctx-only with no model name. An explicit rebind still wins.
    #[test]
    fn model_loadout_surfaces_the_adapters_default_when_binding_model_is_none() {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        use crate::cognition::llm_deliberation_faculty::{model_binding, ModelBinding};

        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let default_id = adapter.default_model().to_string();
        assert!(
            !default_id.is_empty(),
            "fixture adapter must carry a default model"
        );

        // Boot shape: binding with NO explicit model (the live upstart path).
        let handle = model_binding(Arc::clone(&adapter), None, 20_224);
        let c = cycle(vec![]).with_model_binding(handle);
        let (model, ctx) = c.model_loadout().expect("a bound cycle reports a loadout");
        assert_eq!(
            model.as_deref(),
            Some(default_id.as_str()),
            "a None binding id resolves to the adapter's default — the served model"
        );
        assert_eq!(ctx, 20_224);

        // An explicit re-home id wins over the adapter default.
        c.rebind_model(ModelBinding {
            adapter,
            model: Some("qwen-coder-14b".to_string()),
            context_window: 8_192,
        });
        let (model, _) = c.model_loadout().expect("still bound");
        assert_eq!(model.as_deref(), Some("qwen-coder-14b"));

        // No binding at all (no deliberation faculty) → no loadout, honest-absent.
        assert!(cycle(vec![]).model_loadout().is_none());
    }

    // what this catches: every finding is stamped with the cycle it was computed
    // against (the cbar frameIndex) — the decoupling precondition. Without the
    // stamp a late/deferred finding can't know how stale it is, so the arbiter
    // can't merge or reproject it correctly. Asserts a fresh construction is
    // UNSTAMPED, both phases inherit `ws.cycle`, and the counter advances per tick
    // (so two findings from different ticks are never confused for one moment).
    #[tokio::test]
    async fn contributions_carry_the_cycle_they_were_born_in() {
        // A fresh contribution is UNSTAMPED until a live cycle stamps it.
        let raw = Contribution::context(FacultyId::Recall, "engram", 0.8, "r");
        assert_eq!(raw.cycle, CycleId::UNSTAMPED);

        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "engram: prior context",
                0.8,
                "perception finding",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let c = cycle(faculties);

        // First tick → CycleId(1); both the perception (Recall) and deliberation
        // (verdict) findings must carry it.
        let ws1 = c.run("burst one").await;
        assert_eq!(
            ws1.cycle,
            CycleId(1),
            "first live cycle is 1, not the 0 sentinel"
        );
        assert!(
            ws1.broadcast.len() >= 2,
            "both faculties contributed this tick"
        );
        for bid in &ws1.broadcast {
            assert_eq!(
                bid.cycle,
                CycleId(1),
                "finding from {:?} must be stamped with the cycle it reasoned against",
                bid.faculty
            );
        }

        // Second tick → the counter advances to CycleId(2): findings from
        // different ticks can never be mistaken for the same moment.
        let ws2 = c.run("burst two").await;
        assert_eq!(ws2.cycle, CycleId(2), "the service tick advances each run");
        for bid in &ws2.broadcast {
            assert_eq!(bid.cycle, CycleId(2));
        }
    }

    // what this catches: the factory's faculty-isolation seam (#14 ReplayFaculty).
    // `replay(ws, Some(id))` must re-run EXACTLY that faculty against the given
    // workspace, return its contribution, and stamp a timing — never leak other
    // faculties; `replay(ws, None)` runs them all. If isolation leaks or timing is
    // dropped, the glass box can't answer "what did recall surface for this burst?"
    // in isolation, and the harness degrades back to replaying only the final call.
    #[tokio::test]
    async fn replay_isolates_one_faculty_and_times_it() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "engram: the auth migration broke the deploy",
                0.8,
                "relevant past engram",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let c = cycle(faculties);
        let ws = Workspace::new("teammate: the deploy is red, what happened?");

        // isolate recall: exactly one bid, its engram content, a real timing.
        let recall_only = c.replay(&ws, Some(&FacultyId::Recall)).await;
        assert_eq!(recall_only.len(), 1, "only the requested faculty runs");
        assert_eq!(recall_only[0].faculty, FacultyId::Recall);
        assert!(recall_only[0]
            .contribution
            .as_ref()
            .expect("recall bid present")
            .content
            .contains("auth migration"));

        // no filter: every faculty re-runs over the same workspace.
        let all = c.replay(&ws, None).await;
        assert_eq!(all.len(), 2, "None replays every faculty");
    }

    // what this catches: `from_kebab` is the SINGLE inverse of `as_str` — if a
    // new FacultyId variant is added to `as_str` but not `from_kebab`, its tag
    // would silently degrade to `Custom`, splitting the one-place mapping. This
    // round-trip over every non-Custom variant fails loud on that drift.
    #[test]
    fn faculty_id_kebab_round_trips_every_variant() {
        for id in [
            FacultyId::Recall,
            FacultyId::WorldModel,
            FacultyId::Affect,
            FacultyId::Volition,
            FacultyId::Deliberation,
            FacultyId::Salience,
        ] {
            let tag = id.as_str();
            assert_eq!(
                FacultyId::from_kebab(tag),
                id,
                "tag '{tag}' must round-trip; from_kebab drifted from as_str"
            );
        }
        // Custom tags pass through untouched (sentinel-forged faculties).
        assert_eq!(
            FacultyId::from_kebab("sentinel-x"),
            FacultyId::Custom("sentinel-x".to_string())
        );
    }

    // what this catches: attention is a competition over ML-derived salience —
    // the highest-salience faculty bids win the bounded workspace. This is the
    // ML-integration replacement for calculate_priority's hand-weights.
    #[tokio::test]
    async fn arbiter_forwards_every_bid_highest_salience_first() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "low",
                0.2,
                "weak recall",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "high",
                0.9,
                "strong signal",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Affect,
                "mid",
                0.5,
                "some arousal",
            ))),
        ];
        let ws = cycle(faculties).run("the consolidated thread").await;
        let order: Vec<&str> = ws.broadcast.iter().map(|c| c.content.as_str()).collect();
        assert_eq!(order, ["high", "mid", "low"], "every bid, highest ML salience first: the view sizes each");
    }

    // what this catches: ACTING-ORGANISM step 1 — the mind can express "I want to
    // act on the world" as a first-class verdict. A deliberation faculty emits
    // Decision::Act{calls,intent}; the arbiter routes it like any decision, and
    // decision() returns it carrying the calls + the mind's narrated intent. This
    // is the vocabulary the genome learns to use (the disposition is trained, never
    // hardcoded); the executor that runs the calls and re-enters the result as a
    // memory is steps 3–4. Act is peer to Speak, not a special case.
    #[tokio::test]
    async fn deliberation_can_emit_an_act_decision() {
        let call = ToolCall {
            id: "toolu_run_1".to_string(),
            name: "code/run".to_string(),
            input: serde_json::json!({ "lang": "rust", "code": "fn main() { println!(\"{}\", (0..5).sum::<i32>()); }" }),
        };
        let act = Decision::Act {
            calls: vec![call.clone()],
            intent: "run my solution to see what it actually prints".to_string(),
        };
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            act,
            0.95,
            "the model emitted a tool call",
        )))];
        let ws = cycle(faculties).run("peer: does your sum work?").await;
        match ws.decision() {
            Some(Decision::Act { calls, intent }) => {
                assert_eq!(calls.as_slice(), std::slice::from_ref(&call));
                assert!(
                    intent.contains("run my solution"),
                    "the mind's narrated intent rides on the Act decision"
                );
            }
            other => panic!("expected an Act verdict to route through the arbiter, got {other:?}"),
        }
        // verdict() surfaces the intent as the contribution content (audited like
        // any bid) while the calls live on the decision.
        assert_eq!(
            ws.broadcast[0].content,
            "run my solution to see what it actually prints"
        );
    }

    // what this catches: EQUAL CITIZENS — a persona-sent message with high
    // relevance beats a human-sent one with low relevance. Salience (ML) decides,
    // never the sender's rank. This is the death of the Human=1.0/Persona=0.3
    // caste: there is nowhere in this core to encode it.
    #[tokio::test]
    async fn salience_decides_not_sender_caste() {
        // Two world-model bids; the "from a persona" one is more relevant.
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "human said hi (low value)",
                0.3,
                "pleasantry",
            ))),
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "peer persona flagged a real blocker (high value)",
                0.85,
                "actionable",
            ))),
        ];
        let ws = cycle(faculties).run("burst").await;
        assert_eq!(ws.broadcast.len(), 2);
        assert!(
            ws.broadcast[0].content.contains("peer persona"),
            "the relevant peer-persona content leads on salience, not the human's rank"
        );
    }

    // what this catches: the participation Decision is the OUTPUT of the
    // deliberation faculty's thought, surfaced from the workspace — not a gate.
    // Speak / RaiseUnprompted / Pass are all first-class.
    #[tokio::test]
    async fn decision_is_the_deliberation_faculty_output() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "context",
                0.4,
                "recalled",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::RaiseUnprompted {
                    text: "blocker on the deploy".into(),
                },
                0.95,
                "high epistemic value — no one raised it",
            ))),
        ];
        let ws = cycle(faculties).run("coordination thread").await;
        match ws.decision() {
            Some(Decision::RaiseUnprompted { text }) => {
                assert_eq!(text, "blocker on the deploy")
            }
            other => panic!("expected unprompted initiative, got {other:?}"),
        }
    }

    // what this catches: silence (Pass) is a first-class judgment, and abstaining
    // faculties simply don't contribute — no panic, no gate.
    #[tokio::test]
    async fn pass_and_abstain_are_first_class() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(AbstainFaculty(FacultyId::Recall)),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::pass(),
                0.6,
                "nothing worth adding",
            ))),
        ];
        let ws = cycle(faculties).run("idle chatter").await;
        assert_eq!(
            ws.broadcast.len(),
            1,
            "the abstaining faculty added nothing"
        );
        assert_eq!(ws.decision(), Some(&Decision::pass()));
    }

    // what this catches: a FAILED model call (a deliberation FAULT) is surfaced by
    // `deliberation_fault()` and is NEVER read as a verdict — `decision()` stays
    // `None`, so the settle step routes it to `InferenceFailed`, never a serene
    // `Passed`. This is the swept-model regression: reassign changed the served
    // model, the faculty still requested the old one, the lane refused, and the Err
    // used to masquerade as chosen silence ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn deliberation_fault_is_surfaced_and_is_not_a_decision() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "context",
                0.4,
                "recalled",
            ))),
            Arc::new(FixedFaculty(Contribution::deliberation_fault(
                "model 'X' is not the active served model (serving: 'Y')",
            ))),
        ];
        let ws = cycle(faculties).run("swept to an unhosted model").await;
        // The fault is readable and names the cause verbatim…
        assert_eq!(
            ws.deliberation_fault(),
            Some("model 'X' is not the active served model (serving: 'Y')"),
        );
        // …and it is NOT a decision, so it can never collapse into Pass/Speak/Act.
        assert_eq!(
            ws.decision(),
            None,
            "a fault is not a verdict — settle_step must see None here and surface InferenceFailed"
        );
    }

    // what this catches (card fe8554d2): the generation an act carries is the one that made its
    // decision: the winning verdict's own last SERVED receipt, past a retry's fault. Never
    // another faculty's generation, though the turn-wide receipts list it later
    // (phase-then-faculty order, not chronology).
    #[tokio::test]
    async fn the_decision_generation_is_the_winning_verdicts_served_receipt() {
        use crate::ai::provenance::{GenerationOutcome, GenerationReceipt};
        let receipt = |id: &str, outcome: GenerationOutcome| GenerationReceipt {
            submitted_request_id: id.into(),
            outcome,
            capture: None,
            genes: Vec::new(),
            gene_scales: Vec::new(),
            allocation: None,
        };
        let served = || GenerationOutcome::Served { model: "m".into(), provider: "p".into(), provider_request_id: None };
        let faulted = || GenerationOutcome::Faulted { detail: "lane refused".into(), model: None, provider: None };
        let mut verdict = Contribution::verdict(Decision::Speak { text: "done".into() }, 0.8, "decided");
        verdict.receipts = vec![receipt("decided", served()), receipt("retry-fault", faulted())];
        verdict.receipts.swap(0, 1); // the fault came first, the retry served
        let mut context = Contribution::context(FacultyId::Recall, "context", 0.4, "recalled");
        context.receipts = vec![receipt("other-faculty", served())];
        let faculties: Vec<Arc<dyn Faculty>> =
            vec![Arc::new(FixedFaculty(verdict)), Arc::new(FixedFaculty(context))];
        let ws = cycle(faculties).run("decide").await;
        assert_eq!(ws.decision_generation().as_deref(), Some("decided"));

        let mut all_faulted = Contribution::verdict(Decision::Speak { text: "x".into() }, 0.8, "faulted");
        all_faulted.receipts = vec![receipt("only-fault", faulted())];
        let ws = cycle(vec![Arc::new(FixedFaculty(all_faulted))]).run("decide").await;
        assert_eq!(ws.decision_generation(), None, "a decision with no served generation carries none");
    }

    // what this catches: one cycle runs over a CONSOLIDATED burst (the whole
    // world-state at once), not per-event — the efficiency spine.
    #[tokio::test]
    async fn runs_once_over_a_consolidated_burst() {
        let consolidated = "msg1\nmsg2\nmsg3\nmsg4 (many events, one unit)";
        let faculties: Vec<Arc<dyn Faculty>> = vec![Arc::new(FixedFaculty(Contribution::verdict(
            Decision::Speak {
                text: "one reply to the whole thread".into(),
            },
            0.8,
            "caught up on the backlog",
        )))];
        let ws = cycle(faculties).run(consolidated).await;
        assert!(ws.world_state.contains("many events, one unit"));
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    /// In-memory capture sink — the test-bench / replay primitive.
    #[derive(Default)]
    struct RecordingSink(std::sync::Mutex<Vec<WorkspaceTrace>>);
    impl WorkspaceCaptureSink for RecordingSink {
        fn record(&self, trace: &WorkspaceTrace) {
            self.0.lock().unwrap().push(trace.clone());
        }
    }

    // what this catches: every tick is replayable — the trace captures the FULL
    // competition incl. the LOSER bid (the bit you need to debug "why didn't it
    // win?"), what won, and the decision. This is the glass box that makes
    // working on the mind debuggable + test-benchable, not guesswork.
    #[tokio::test]
    async fn capture_records_the_full_tick_including_losers() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "loser bid",
                0.1,
                "weak — should lose attention",
            ))),
            Arc::new(FixedFaculty(Contribution::verdict(
                Decision::Speak {
                    text: "winner".into(),
                },
                0.9,
                "high value",
            ))),
        ];
        // Every bid is broadcast, and the trace keeps every bid with its salience.
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter))
            .with_capture(sink.clone())
            .run("thread")
            .await;

        let traces = sink.0.lock().unwrap();
        assert_eq!(traces.len(), 1, "one tick recorded");
        let t = &traces[0];
        assert_eq!(t.bids.len(), 2, "trace keeps ALL bids — winner AND loser");
        assert_eq!(t.broadcast.len(), 2, "attention forwards every bid");
        assert!(
            t.bids.iter().any(|b| b.content == "loser bid"),
            "the loser bid + its salience + reasoning are replayable for debugging"
        );
        assert_eq!(
            t.decision,
            Some(Decision::Speak {
                text: "winner".into()
            })
        );
    }

    // what this catches: STAGED ASSEMBLY — the deliberation faculty conditions its
    // Decision on what recall surfaced in phase 1. This is the coherence fix: "pull
    // relevant memory, THEN decide." Under the old single-pass join_all the decider
    // bid over an EMPTY broadcast and could never be informed by recall.
    #[tokio::test]
    async fn deliberation_sees_the_recall_that_won_phase_one() {
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            // Phase 1 (perception): recall surfaces a memory with strong salience.
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled the open blocker",
            ))),
            // Phase 2 (deliberation): reacts to the assembled broadcast.
            Arc::new(ConditionalDeliberation),
        ];
        let ws = cycle(faculties).run("what's the status?").await;
        match ws.decision() {
            Some(Decision::Speak { text }) => assert!(
                text.contains("deploy pipeline is red"),
                "the decider must condition on recall it saw, got: {text}"
            ),
            other => {
                panic!("expected an informed Speak, got {other:?} — decider was blind to recall")
            }
        }
    }

    // what this catches: the trace exposes the ASSEMBLED CONTEXT the decider saw
    // (context_broadcast) separately from the final broadcast — the glass-box
    // answer to "what RAG did the mind reason over?" — and the deliberation output
    // is appended, not competing for the bounded context spotlight.
    #[tokio::test]
    async fn trace_separates_assembled_context_from_deliberation_output() {
        let sink = Arc::new(RecordingSink::default());
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::Recall,
                "deploy pipeline is red",
                0.8,
                "recalled",
            ))),
            Arc::new(ConditionalDeliberation),
        ];
        let _ws = WorkspaceCycle::new(faculties, Arc::new(SalienceArbiter))
            .with_capture(sink.clone())
            .run("status?")
            .await;

        let traces = sink.0.lock().unwrap();
        let t = &traces[0];
        // The assembled context is exactly the phase-1 recall winner.
        assert_eq!(t.context_broadcast.len(), 1);
        assert_eq!(t.context_broadcast[0].faculty, FacultyId::Recall);
        // The final broadcast holds the context PLUS the deliberation verdict.
        assert_eq!(
            t.broadcast.len(),
            2,
            "assembled context + deliberation output"
        );
        assert!(
            t.broadcast.iter().any(|c| c.decision.is_some()),
            "verdict appended"
        );
        // Both phases' bids are in the full competition record.
        assert_eq!(t.bids.len(), 2);
    }

    // what this catches: a perception faculty does NOT bid in the deliberation
    // phase (reacts_to_broadcast == false), and a deliberation faculty does NOT
    // bid in the perception phase — each faculty fires once, in its tier. This is
    // the cbar needsRealTime() split: no double inference.
    #[tokio::test]
    async fn faculties_fire_only_in_their_declared_phase() {
        // A faculty that records how many times it was asked to contribute, and in
        // what broadcast state, so we can prove single-phase firing.
        struct CountingDeliberation(std::sync::Arc<std::sync::atomic::AtomicUsize>);
        #[async_trait]
        impl Faculty for CountingDeliberation {
            fn id(&self) -> FacultyId {
                FacultyId::Deliberation
            }
            fn reacts_to_broadcast(&self) -> bool {
                true
            }
            async fn contribute(&self, ws: &Workspace) -> Option<Contribution> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // If we are ever called, the broadcast must be populated (phase 2).
                assert!(
                    !ws.broadcast.is_empty(),
                    "deliberation must only fire after phase 1 assembled context"
                );
                Some(Contribution::verdict(Decision::pass(), 0.5, "noted"))
            }
        }
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(FixedFaculty(Contribution::context(
                FacultyId::WorldModel,
                "ctx",
                0.7,
                "perception",
            ))),
            Arc::new(CountingDeliberation(calls.clone())),
        ];
        let _ws = cycle(faculties).run("burst").await;
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "deliberation faculty fired exactly once, in its phase"
        );
    }

    // what this catches: the situation-aware focuser must make Situation::PostAction
    // REAL — on a re-perception tick it drops the standing SOCIAL re-grounding
    // (stable contributions: roster/doctrine/workspace-map) so the tool result +
    // working memory own the window, while FreshContext keeps the fuller grounding.
    // Regression guard for the "cognition soup crowds out the code" failure: if a
    // future edit makes PostAction stop dropping stable grounding, a heads-down
    // coding turn goes back to re-dumping social framing on every act→observe tick.
    #[test]
    fn situation_focuser_drops_stable_grounding_only_post_action() {
        let arbiter = SituationFocusArbiter::new();
        // A realistic mixed tick: high-salience standing grounding (stable) plus the
        // volatile task context (the just-landed tool result + a recalled fact).
        let roster = Contribution::context(
            FacultyId::Custom("roster".into()),
            "room roster",
            0.9,
            "grounding",
        )
        .session_stable();
        let doctrine = Contribution::context(
            FacultyId::Custom("doctrine".into()),
            "operating doctrine",
            0.85,
            "grounding",
        )
        .session_stable();
        let result = Contribution::context(
            FacultyId::WorldModel,
            "[action #1] code/read → fn main()...",
            0.6,
            "result",
        );
        let recall = Contribution::context(
            FacultyId::Recall,
            "recalled: the ticket asks for X",
            0.5,
            "recall",
        );
        let candidates = vec![roster, doctrine, result.clone(), recall.clone()];

        // Fresh ask: fuller grounding — every bid survives, exactly like the blind
        // salience floor.
        let fresh = arbiter.focus(
            candidates.clone(),
            &FocusContext {
                world_state: "ask",
                situation: Situation::FreshContext,
            },
        );
        assert_eq!(fresh.len(), 4, "FreshContext keeps the fuller grounding");

        // Post-action re-perception keeps the standing grounding. INVERTED 2026-08-07.
        //
        // This arm used to assert the opposite — `stable` contributions dropped before
        // the top-k — on the rationale "they were already perceived; re-dumping them
        // just crowds the result out". The premise does not hold: every settle
        // iteration composes a FRESH prompt (act_observe holds `world` constant and
        // lets only memory change), so framing dropped at iteration 2 is simply ABSENT
        // from iteration 2's window, not remembered from iteration 1.
        //
        // What that cost, measured live: Atlas held work card 4780a02e on a healthy
        // renewing lease and his turn carried no board at all — the evicted set was
        // exactly the three StandingFraming sources, including the workspace-map
        // (0.90, the highest bid present) that the old comment itself named as
        // something that should own the window. He then reported "no open tasks",
        // correctly, about a window we had emptied.
        //
        // The real constraint the old rationale was reaching for — a re-perception
        // should not pay full freight for context that has not changed — is REAL, and
        // is now #167's job (Situation-weighted per-layer budgets) or a per-source
        // "re-grounding is redundant this tick" bid. Not a serialization flag used as
        // an attention verdict: `Contribution::stable` is documented as ordering-only,
        // "salience still governs which contributions are included and truncated".
        let post = arbiter.focus(
            candidates,
            &FocusContext {
                world_state: "result",
                situation: Situation::PostAction,
            },
        );
        assert_eq!(
            post.len(),
            4,
            "a citizen who ACTED must not go blind to what she is acting on — \
             standing framing competes on salience like everything else"
        );
        assert!(
            post.iter().any(|c| c.stable),
            "standing grounding survives a re-perception tick: {:?}",
            post.iter().map(|c| c.content.as_str()).collect::<Vec<_>>()
        );
        assert!(
            post.iter().any(|c| c.content.contains("[action #1]")),
            "the tool result MUST survive — it's what 'what next' turns on"
        );
    }
