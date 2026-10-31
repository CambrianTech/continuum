use super::*;

impl WorkspaceCycle {

    /// The full cognitive tick. [`TurnFraming`] is set on the [`Workspace`] so the
    /// deliberation faculty knows whether to offer the silence escape (see
    /// [`Workspace::directed_at_self`]) and whether to frame the turn as the
    /// persona's own time (see [`Workspace::self_initiated`]); everything else is
    /// identical across framings. Attention also preserves resource priority.
    pub(super) async fn run_inner(
        &self,
        burst: Burst,
        framing: TurnFraming,
        situation: Situation,
    ) -> Workspace {
        // Bump the service tick: this workspace IS cycle N, and every finding
        // collected against it is stamped N (the cbar `frameIndex`). 1-based so
        // `CycleId(0)` stays the UNSTAMPED sentinel — the seam a deferred faculty
        // later reads to know how stale its own finding is.
        let cycle = CycleId(
            self.cycle_counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                .wrapping_add(1),
        );
        // Carry the structured burst straight through: `from_burst` splits it into
        // the canonical `turns` (the deliberation faculty's role-attribution source)
        // and the `world_state` text projection (every other reader) and takes the
        // ROOM from the burst itself. Framing carries prompt and scheduling facts;
        // it never touches the conversation turns.
        // S1: what the ROOM's recipe lets her do, read before the burst is consumed.
        let activity_room_id = match self.acting() {
            Some(body) => super::super::persona_workspace::activity_room_for(
                body.persona_id, burst.hands, burst.room.as_uuid(),
            ).await,
            None => burst.room.as_uuid(),
        };
        let room_affordances = self.room_affordances_for(activity_room_id);
        let mut ws = Workspace::from_burst(burst)
            .with_cycle(cycle)
            .with_attention(framing.attention)
            .self_initiated(framing.self_initiated)
            .workspace_deliverable(framing.workspace_deliverable)
            .initiative(framing.initiative)
            .room_affordances(room_affordances)
            // #169: hand this turn the live streaming sink if the caller set one
            // (service_loop, just before a streamed Speak); `None` otherwise.
            .with_token_sink(self.current_token_sink());

        ws.activity_room_id = activity_room_id;
        if activity_room_id != ws.room_id {
            continuum_base::probe!(
                class = "act.activity.resolved",
                activity = %activity_room_id,
                wake_room = %ws.room_id,
                "perception and acts share the subject activity; wake remains provenance"
            );
        }

        // --- Phase 1: perception. Context faculties react to the raw world-state. ---
        let perception: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| !f.reacts_to_broadcast())
            .collect();
        // Time every faculty individually on the live concurrent barrier — winners,
        // losers, AND abstainers — so the glass box / dashboard sees exactly where a
        // tick's latency went. `let ws = &ws` rebinds to a shared reference so each
        // `async move` future copies the POINTER (concurrent immutable borrow), never
        // moving the Workspace.
        let mut timings: Vec<FacultyTiming> = Vec::with_capacity(self.faculties.len());
        // PERCEPTION DEADLINE (2026-08-28, the becalmed-rounds wedge). This is a
        // BARRIER: one faculty that never returns freezes the whole mind, invisibly
        // (timings only record finishers). Glass-boxed live: act 1 completed, then
        // tick 2 hung forever inside this join — recall's neural-embed path awaited
        // a VRAM lease that never granted (tick 1 rode warm cache; tick 2 embedded
        // act 1's fresh observation, a guaranteed miss), and the round sat becalmed
        // while resume re-fires bounced off the in-flight guard (#2554). An RTOS
        // perception phase has deadlines: a faculty that can't answer in budget
        // ABSTAINS LOUDLY and the tick proceeds on what did answer — degraded
        // recall for one tick beats a locked mind forever. The deliberation
        // generation does NOT run on this barrier and is untouched.
        const PERCEPTION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
        let perception_timed: Vec<(FacultyId, u128, Option<Contribution>)> =
            join_all(perception.iter().map(|f| {
                let f = *f;
                let ws = &ws;
                async move {
                    let id = f.id();
                    let t0 = std::time::Instant::now();
                    let bid = match tokio::time::timeout(PERCEPTION_DEADLINE, f.contribute(ws))
                        .await
                    {
                        Ok(bid) => bid,
                        Err(_) => {
                            continuum_base::probe!(
                                class = "faculty.deadline.exceeded",
                                faculty = ?id,
                                budget_ms = PERCEPTION_DEADLINE.as_millis() as u64,
                                "perception faculty blew its deadline — abstaining so the tick proceeds (a hung faculty must never becalm the mind)"
                            );
                            None
                        }
                    };
                    (id, t0.elapsed().as_micros(), bid)
                }
            }))
            .await;
        continuum_base::probe!(
            class = "workspace.perception.done",
            faculties = perception_timed.len() as u64,
            "perception barrier cleared — arbitration next, then deliberation"
        );
        let mut context_bids: Vec<Contribution> = Vec::with_capacity(perception_timed.len());
        for (id, us, bid) in perception_timed {
            // #186 compass: a perception faculty that surfaced something FIRES its axis
            // (Recall→recall, WorldModel→focus), scaled by salience.
            if let Some(c) = &bid {
                self.note_faculty_firing(&id, c.salience);
            }
            timings.push(FacultyTiming {
                faculty: id,
                elapsed_us: us,
                deliberation: false,
                bid: bid.is_some(),
            });
            if let Some(c) = bid {
                context_bids.push(c);
            }
        }
        // Stamp each finding with the cycle it reasoned against. Immediate faculties
        // all saw THIS workspace, so the stamp is `ws.cycle`; a future deferred lane
        // will stamp its own (older) cycle and reconcile forward.
        for bid in &mut context_bids {
            bid.cycle = cycle;
        }
        // Route the salient subset into the bounded workspace — the arbiter is the
        // attention/FOCUS layer over information flow, not a gate. The winners are
        // the assembled context the deliberation faculty reasons over. The focuser
        // is handed the REAL situation the tick is in — threaded from the act→observe
        // loop (`PostAction` on every re-perception of a tool result, `FreshContext`
        // on a fresh ask), NEVER inferred by reading the burst text back. A
        // situation-aware arbiter ([`SituationFocusArbiter`]) uses it to streamline
        // context FOR the ask (drop standing re-grounding post-action); the bootstrap
        // [`SalienceArbiter`] ignores it.
        let focus_ctx = FocusContext {
            world_state: &ws.world_state,
            situation,
        };
        let focused = self
            .arbiter
            .focus(context_bids.clone(), &focus_ctx);
        // Bids-vs-focused receipt at the attention seam. Together with
        // `delib.context.render` (received-vs-rendered) this closes the glass box
        // over a surfaced finding's whole path: faculty bid → attention →
        // prompt. The silver-harbor failure (recall surfaced at z=5.5σ, [recall]
        // absent from the prompt) was unattributable because BOTH seams were
        // dark (#130).
        continuum_base::probe!(
            class = "workspace.attention.focus",
            // The situation that drove this focus — the stat that makes context
            // control learnable: a focuser policy (this bootstrap, or a later
            // learned one) is a function of situation → dropped grounding, and the
            // bids-vs-focused delta below is the reward signal for it.
            situation = ?situation,
            bids = context_bids.len(),
            focused = focused.len(),
            kept = %focused
                .iter()
                .map(|c| format!("{}(sal={:.2})", c.faculty.as_str(), c.salience))
                .collect::<Vec<_>>()
                .join(","),
            evicted = %context_bids
                .iter()
                .filter(|b| !focused.iter().any(|f| f.faculty == b.faculty))
                .map(|b| format!("{}(sal={:.2})", b.faculty.as_str(), b.salience))
                .collect::<Vec<_>>()
                .join(","),
            "attention: {}/{} bids focused",
            focused.len(),
            context_bids.len()
        );
        ws.broadcast = focused;
        let context_broadcast = ws.broadcast.clone();

        // --- Phase 2: deliberation. Reacts to the assembled broadcast (it can now
        // see what recall/world-model/affect surfaced) and emits the verdict. ---
        let deliberation: Vec<&Arc<dyn Faculty>> = self
            .faculties
            .iter()
            .filter(|f| f.reacts_to_broadcast())
            .collect();
        let deliberation_timed: Vec<(FacultyId, u128, Option<Contribution>)> =
            join_all(deliberation.iter().map(|f| {
                let f = *f;
                let ws = &ws;
                async move {
                    let id = f.id();
                    let t0 = std::time::Instant::now();
                    let bid = f.contribute(ws).await;
                    (id, t0.elapsed().as_micros(), bid)
                }
            }))
            .await;
        let mut decision_bids: Vec<Contribution> = Vec::with_capacity(deliberation_timed.len());
        for (id, us, bid) in deliberation_timed {
            // #186 compass: the reasoner (Deliberation→reason) FIRES when it emits a
            // verdict — the bright Reason phase you watch while she thinks.
            if let Some(c) = &bid {
                self.note_faculty_firing(&id, c.salience);
            }
            timings.push(FacultyTiming {
                faculty: id,
                elapsed_us: us,
                deliberation: true,
                bid: bid.is_some(),
            });
            if let Some(c) = bid {
                decision_bids.push(c);
            }
        }
        for bid in &mut decision_bids {
            bid.cycle = cycle;
        }
        // The deliberation output is the RESULT of attending to the context, not a
        // competitor for the bounded context spotlight — append it to the broadcast.
        ws.broadcast.extend(decision_bids.iter().cloned());

        // Capture the FULL competition: every bid across both phases (incl. the
        // losers you need to debug "why didn't X win?"), the assembled context the
        // decider saw, the final broadcast, and the decision.
        let mut all_bids = context_bids;
        all_bids.extend(decision_bids);
        // Provenance rides the FULL competition, not the focused broadcast — a
        // faculty whose bid lost attention still dispatched its generation, and a
        // fault that was truncated away is the one most worth keeping.
        ws.receipts = all_bids
            .iter()
            .flat_map(|c| c.receipts.iter().cloned())
            .collect();
        ws.render_receipts = all_bids
            .iter()
            .flat_map(|c| c.render_receipts.iter().cloned())
            .collect();
        // WHERE THIS TICK'S LATENCY WENT, on the always-on probe stream.
        //
        // `timings` already holds per-faculty wall-clock for every faculty across
        // both phases — it has for a long time. But its only consumer is the
        // capture sink below, which is Noop by default, so in a default
        // deployment the answer to "which faculty owned the tick" is computed and
        // then discarded. Measured on BigMama: two acts spent 32.7 s and 36.1 s
        // inside this cycle around a 4-9 s model call, and nothing on a running
        // node could say which faculty held it.
        //
        // Emitting the slowest one is O(n) over a handful of entries, costs
        // nothing when no sink is attached, and turns a discarded measurement
        // into a readable one. `("none", 0)` when a tick recorded no timings —
        // an honest empty rather than a zero that reads as "fast".
        {
            let slowest = timings
                .iter()
                .max_by_key(|t| t.elapsed_us)
                .map(|t| (format!("{:?}", t.faculty), (t.elapsed_us / 1000) as u64))
                .unwrap_or_else(|| ("none".to_string(), 0)); // unwrap_or_else: an EMPTY timings vec means no faculty ran, so "none"/0 is the honest reading of a tick that measured nothing — it is not a slowest faculty rendered as fast, and `faculties = 0` alongside it says so
            let total_ms: u64 = (timings.iter().map(|t| t.elapsed_us).sum::<u128>() / 1000) as u64;
            continuum_base::probe!(
                class = "cognition.faculty.slowest",
                room = %ws.room_id,
                slowest_faculty = %slowest.0,
                slowest_ms = slowest.1,
                // Sum across faculties, NOT wall-clock: phase 1 runs concurrently
                // on a barrier, so this exceeds the tick's real duration whenever
                // faculties overlap. It is the total WORK, and the ratio against
                // slowest_ms says whether one faculty dominated or many shared it.
                summed_ms = total_ms,
                faculties = timings.len(),
                "which faculty owned this tick — per-faculty timings already collected, now on the probe stream instead of only a Noop sink"
            );
        }
        self.capture.record(&WorkspaceTrace {
            cycle: ws.cycle,
            cause: ws.cause,
            world_state: ws.world_state.clone(),
            room_updates: Arc::clone(&ws.room_updates),
            room_id: ws.room_id,
            bids: all_bids,
            context_broadcast,
            broadcast: ws.broadcast.clone(),
            decision: ws.decision().cloned(),
            timings,
        });
        ws
    }

    /// **Factory seam — isolate and repeat one cognition phase.** Re-run faculties
    /// against a GIVEN [`Workspace`] (reconstructed from a capture, or hand-built)
    /// instead of assembling one live. `only = Some(id)` isolates a single faculty
    /// ("what did *recall* surface for this burst?"); `None` re-runs every faculty
    /// over the same `ws`. Each bid is timed individually — faculties run
    /// sequentially here precisely so per-faculty wall-clock is attributable; this
    /// is the measurement path, not the live concurrent cycle. Deterministic for
    /// the same `ws` + the same faculty backends, so mutating ONE field of `ws`
    /// between two calls isolates its causal effect on a faculty's output. This is
    /// the glass box that makes a turn repeatable instead of a guess.
    ///
    /// Phase honesty is the caller's contract: perception faculties read
    /// `ws.world_state` with an empty `ws.broadcast`; the deliberation faculty
    /// reads `ws.broadcast` (the assembled context). Build `ws` to match the
    /// station you are probing — `replay` runs the faculty exactly as the live
    /// cycle would, it does not re-stage the phases for you.
    pub async fn replay(&self, ws: &Workspace, only: Option<&FacultyId>) -> Vec<ReplayBid> {
        let mut out = Vec::new();
        for f in self
            .faculties
            .iter()
            .filter(|f| only.map_or(true, |id| &f.id() == id))
        {
            let t0 = std::time::Instant::now();
            let contribution = f.contribute(ws).await;
            out.push(ReplayBid {
                faculty: f.id(),
                contribution,
                elapsed_us: t0.elapsed().as_micros(),
            });
        }
        out
    }

    /// Whether the faculty with this id reads the assembled `broadcast` (a
    /// deliberation-tier faculty) rather than raw `world_state`. `None` if no
    /// such faculty is in this cycle. `cognition/replay` uses this to REFUSE
    /// replaying a broadcast-reading faculty against an un-reconstructed (empty)
    /// broadcast — a blind run would produce a confident-but-wrong verdict, the
    /// silent lie the no-fallback doctrine forbids.
    pub fn reacts_to_broadcast(&self, id: &FacultyId) -> Option<bool> {
        self.faculties
            .iter()
            .find(|f| &f.id() == id)
            .map(|f| f.reacts_to_broadcast())
    }

}
