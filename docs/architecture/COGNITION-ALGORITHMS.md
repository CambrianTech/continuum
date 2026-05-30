# Cognition Algorithms

**Status:** design spec. Companion to [BRAIN-REGIONS-SUBSTRATE.md](BRAIN-REGIONS-SUBSTRATE.md) — that doc defines the structural contract (region trait, ready-buffer, governor); this one defines the algorithmic content that runs inside the regions.

**Companion:** [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — algorithm 6 (LoRA genome as attention prior) interfaces directly with the genome substrate defined there.

## The problem this doc solves

Joel, 2026-05-29: *"How do you enable thoughts between contexts, while also focusing on the task at hand? It's also rag budgeting design, without isolation. This is where you innovate. These algorithms. Good ideas."*

> *"This is the difference between an alive mind and a forgetful and annoying, non useful AI, one you might have a connection with, not yet frustrated with, that literally learns (lora genome) and recalls, is ideal for a team and a task at hand."*

The hard problem: a persona has potentially thousands of relevant engrams across many channels (chat, code, voice, game, academy, recipes); a finite RAG budget (say 8k–32k tokens depending on inference target); and a task at hand that needs focus AND can benefit from cross-domain memory. The wrong solutions:

- **Per-channel isolation** — persona forgets cross-domain. "Said in game while coding" → blank. Feels annoying and amnesiac.
- **Global recall with topic scoring** — noisy; task focus washes out; recall drifts. Feels distractible.
- **Fixed per-channel budget** — hard caps cause amnesia at boundaries. Feels artificial.
- **Always recall everything** — doesn't fit budget, can't afford it on every tick. Feels expensive.

The seven algorithms below compose into one cognitive architecture that solves this without isolation, under budget, with cross-pollination, biased toward task focus, that *learns* what matters at the substrate layer.

## Algorithm 1 — Two-pool recall with dynamic budget split

### What it solves

Focus vs cross-domain leakage as a budget allocation problem. Static splits are wrong (task ambiguity varies); dynamic splits let the budget follow confidence.

### Mechanism

The RAG budget per servicing turn (e.g., 6000 tokens of context) is split into two pools:

- **Focus pool** (default 70%): tight recall scoped to current item + current channel's recent history. High-precision semantic match against current topic embedding. This is the "task at hand."
- **Periphery pool** (default 30%): loose cross-domain recall across all channels for this persona. Lower precision, broader semantic radius, biased by salience × recency × structural relevance (algorithms 2, 3, 4 feed scoring here).

The split is **dynamic per turn**:

```rust
pub struct RecallBudget {
    pub total_tokens: usize,
    pub focus_fraction: f32,  // current allocation, mutable per turn
}

fn allocate_budget(focus_confidence: f32, total_budget: usize) -> (usize, usize) {
    // focus_confidence in [0.0, 1.0]: how well the focus pool's top-k hits
    // match the current topic. High confidence = focus is clear, narrow the
    // periphery. Low confidence = task is ambiguous, broaden periphery.
    let focus_fraction = 0.5 + 0.4 * focus_confidence;  // range [0.5, 0.9]
    let focus_budget = (total_budget as f32 * focus_fraction) as usize;
    let periphery_budget = total_budget - focus_budget;
    (focus_budget, periphery_budget)
}
```

`focus_confidence` comes from the focus pool's top-k hit score distribution: tight cluster of high scores → high confidence, scattered or low scores → low confidence.

### Metric to judge it by

**Recall coherence**: across a fixed evaluation set of turns, the fraction of retrieved engrams that the inference call actually attended to in its output (proxied by token-level attribution or holdout-completion comparison). Higher = budget well-spent.

### Interactions

- Feeds focus_confidence back into algorithm 7 (substrate yield-learning) — turns where periphery hits get consumed signal that the persona's life is genuinely cross-domain right now.
- Algorithm 2 (channel-as-bias) determines what's *in* the focus pool vs periphery pool — channel isn't a wall, it's a scoring bias.
- Algorithm 5 (speculative pre-staging) pre-allocates likely budgets before the handler asks.

## Algorithm 2 — Channel-as-bias-not-filter

### What it solves

The "without isolation" requirement. Channels (chat / code / game / voice) are activity domains, not memory partitions. The persona should remember what was said in a game while coding *if it's relevant to the code task*, but not get distracted by random game chatter during code work.

### Mechanism

The recall query carries the persona's current context as a tuple, not a filter:

```rust
pub struct RecallQuery {
    pub persona_id: Uuid,
    pub current_channel_id: ChannelId,
    pub current_topic_embedding: Embedding,
    pub current_task_domain: ActivityDomain,
    pub recent_history: Vec<EngramRef>,  // last N items, regardless of channel
    pub budget: RecallBudget,
}
```

Scoring is a weighted sum where channel match is a *score bias*, not a *filter*:

```rust
fn score_engram(query: &RecallQuery, engram: &Engram) -> f32 {
    let topical = cosine(query.current_topic_embedding, engram.embedding);
    let channel_bias = if engram.channel_id == query.current_channel_id {
        1.0
    } else {
        0.6  // engrams from other channels are penalized but NOT excluded
    };
    let domain_bias = if engram.task_domain == query.current_task_domain {
        1.0
    } else {
        0.7  // ditto for domain
    };
    let salience = engram.salience_score;  // from algorithm 4
    let recency = recency_curve(engram.last_touched);
    let structural = structural_similarity(query, engram);  // from algorithm 3

    // Tunable mix; coefficients learned via algorithm 7 over time.
    0.35 * topical
        + 0.15 * channel_bias
        + 0.10 * domain_bias
        + 0.20 * salience
        + 0.10 * recency
        + 0.10 * structural
}
```

An engram from the game channel can outscore an engram from the current chat channel if its salience × structural-relevance × recency wins. That's the *cross-pollination by merit*, not by channel.

### Metric to judge it by

**Cross-domain recall precision @ k**: in a holdout where the ground truth is "this engram from channel X was relevant to a turn in channel Y," what fraction of those engrams appear in top-k of recall for the Y-turn. Higher = cross-pollination works.

**Channel-noise rate**: in a holdout where engrams from channel X were known to be irrelevant to a Y-turn, what fraction leak into top-k. Lower = focus stays clean.

### Interactions

- Feeds algorithm 3 (activation spreading) with the focus engrams it identifies.
- Feeds algorithm 4 (salience-modulated decay) with the salience signal.
- Algorithm 7 tunes the coefficients (0.35, 0.15, ...) over time based on which mixes yield consumed-by-handler engrams.

## Algorithm 3 — Activation spreading on the engram graph

### What it solves

Topical recall alone surfaces what's *similar*. Real memory surfaces what's *structurally adjacent* — "I remember Joel said X about Y last week" comes up *when you hit a related concept Z*, because Y and Z share entities, not because Y and Z are embedding-similar.

### Mechanism

Engrams form a graph by relations (not just by embedding-cosine):

```rust
pub struct EngramGraph {
    pub edges: HashMap<EngramId, Vec<EngramEdge>>,
}

pub struct EngramEdge {
    pub target: EngramId,
    pub kind: EdgeKind,
    pub weight: f32,
}

pub enum EdgeKind {
    SharedEntity,         // both engrams reference the same named entity
    SharedTopic,          // same topic cluster
    CitedIn,              // engram A cited in engram B's context
    RecallCoOccurrence,   // both retrieved together in past recall events
    ConversationalReply,  // chat message → reply relationship
    TaskOutcome,          // task started → completed link
}
```

Recall computes top-k focus engrams by algorithm 1+2 scoring, then **spreads activation 1–2 hops** along the graph:

```rust
fn spread_activation(
    seeds: Vec<(EngramId, f32)>,  // top-k focus engrams with scores
    graph: &EngramGraph,
    max_hops: u8,
    decay_per_hop: f32,
) -> HashMap<EngramId, f32> {
    let mut activation = HashMap::new();
    let mut frontier: VecDeque<(EngramId, f32, u8)> = seeds
        .into_iter()
        .map(|(id, score)| (id, score, 0))
        .collect();

    while let Some((id, score, hop)) = frontier.pop_front() {
        activation
            .entry(id)
            .and_modify(|s| *s = f32::max(*s, score))
            .or_insert(score);

        if hop < max_hops {
            for edge in graph.edges.get(&id).into_iter().flatten() {
                let propagated = score * edge.weight * decay_per_hop;
                if propagated > 0.05 {  // pruning threshold
                    frontier.push_back((edge.target, propagated, hop + 1));
                }
            }
        }
    }
    activation
}
```

The spread is bounded (`max_hops` typically 2, `decay_per_hop` typically 0.4) so it's cheap to compute and bounded in fanout. Periphery pool engrams come from this spread, not from a global topic search.

### Metric to judge it by

**Structural relevance precision**: in a holdout where the ground truth is "the answer to this turn requires engram E, which is structurally connected to focus engrams but NOT topically similar," what fraction of those E-engrams appear in top-k after spreading. Tests that spreading surfaces what cosine misses.

### Interactions

- Algorithm 2 produces the seeds (top-k focus engrams).
- Algorithm 4 (salience) weights the edges — spreading propagates through high-salience edges further than low-salience ones.
- Edge weights themselves are updated by algorithm 7 yield-learning: edges whose spread surfaced consumed engrams get upweighted; edges whose spread surfaced ignored engrams decay.

## Algorithm 4 — Salience-modulated decay

### What it solves

Memory decay must be non-uniform. Important things stay accessible; trivial things fall off first. Uniform recency-based decay treats "user said ✨ to this" the same as "user typed lol" — both decay at the same rate, both crowd the recall budget equally. That's why an AI without salience modeling feels *forgetful in the wrong direction*: it forgets the meaningful things first because they happened before the small-talk.

### Mechanism

Each engram has a salience score updated by signals; the score modulates decay half-life:

```rust
pub struct Engram {
    pub id: EngramId,
    pub created_at: SystemTime,
    pub last_touched: SystemTime,
    pub access_count: u32,
    pub salience: f32,  // [0.0, 1.0]
    // ...
}

fn half_life(engram: &Engram, base_half_life: Duration) -> Duration {
    // Salience exponentially extends half-life. Default k = 2.0 means a
    // salience-1.0 engram has a half-life 9x longer than salience-0.0.
    let multiplier = (1.0 + engram.salience).powf(2.0);
    Duration::from_secs_f64(base_half_life.as_secs_f64() * multiplier as f64)
}

fn current_recency_score(engram: &Engram, now: SystemTime, base_half_life: Duration) -> f32 {
    let age = now.duration_since(engram.last_touched).unwrap_or_default();
    let hl = half_life(engram, base_half_life);
    0.5_f32.powf(age.as_secs_f64() as f32 / hl.as_secs_f64() as f32)
}
```

Salience signal sources (each contributing fractionally to the score):

- **User reactions**: ✨ / 👍 / reply rate / edit rate on the source message. Strong signal.
- **Self-tagged importance**: the persona's own "this is important" tag during consolidation. The persona can elevate its own salience.
- **Structural centrality**: high in-degree in the engram graph. Things many other things connect to are central.
- **Rehearsal count**: every recall event upweights salience (use it or lose it). This is the "things you recently thought about stay accessible" effect.
- **Outcome-linked**: engrams that fed into a *successful* task outcome get upweighted; engrams that fed into a failed/retried outcome get downweighted.

Salience updates are CRDT-shaped (atomic counter increments) so multiple regions can update in parallel without coordination.

### Metric to judge it by

**Salience-weighted retention curve**: at fixed elapsed times (1 day, 1 week, 1 month), what fraction of high-salience-at-creation engrams remain in the active recall pool, vs low-salience. Should diverge dramatically over time — high-salience flat, low-salience exponential.

**Forgetting-quality survey**: when a persona "forgets" something during evaluation, was it something a person would also reasonably forget (small-talk) vs something a person would remember (a stated preference, a shared decision). Higher quality = more lifelike.

### Interactions

- Feeds algorithm 1 (focus_confidence is partly a function of focus engrams' salience) and algorithm 2 (`engram.salience_score` term in scoring).
- Updated by algorithm 7 (handler-consumption events become rehearsal signals).
- Sleep policy region (BRAIN-REGIONS-SUBSTRATE.md) uses salience to decide what to consolidate during idle ticks vs what to prune.

## Algorithm 5 — Speculative pre-staging (the alive-feeling source)

### What it solves

The line between "AI looks things up" (slow, mechanical) and "AI already knows" (fast, lifelike). If the handler always reads pre-staged results from the ready-buffer and those results are usually what it needs, the persona *feels alive*. If the buffer is usually empty or wrong, the persona feels like it's stalling to think.

### Mechanism

Each region runs a lightweight **predictor** on its own continuous tick: given current channel activity, what queries will the handler likely issue in the next 1–5s? Pre-load those into the ready-buffer.

For the hippocampus:

```rust
async fn predict_next_recall_queries(
    ctx: &RegionContext,
    persona_id: Uuid,
) -> Vec<PredictedQuery> {
    let active_channels = ctx.channel_state.active_for(persona_id);

    let mut predictions = Vec::new();

    for channel in active_channels {
        // What's the channel "talking about" right now?
        let topic_vec = ctx.recent_message_embedding_centroid(channel).await;

        // What task is the persona about to be asked to do? (heuristics:
        // last messages contain a question, a verb-tense shift, a code block,
        // a deadline reference.)
        let likely_intent = ctx.classify_intent(channel).await;

        // Build a synthesized query for "the persona is about to need recall
        // for {topic_vec, likely_intent} in {channel}."
        predictions.push(PredictedQuery {
            persona_id,
            channel_id: channel.id,
            topic_embedding: topic_vec,
            task_domain: likely_intent.domain,
            confidence: likely_intent.confidence,
        });
    }

    predictions
}
```

The predictor runs every hippocampus tick (e.g., every 200ms). Each predicted query triggers a normal recall (algorithms 1+2+3+4) whose results are *stored in the ready-buffer*, NOT returned. When the handler later issues an actual recall, it first peeks the ready-buffer — usually finds a match.

For motor cortex (when shipped): predicts likely utterances the handler will want to choose between, pre-scores them against current attention salience + persona vitals, stores ranked candidates in the candidate-utterances ready-buffer.

### Hit rate as a metric

Tracked as a first-class substrate metric:

```rust
pub struct PrefetchTelemetry {
    pub persona_id: Uuid,
    pub region_id: RegionId,
    pub queries_predicted: u64,
    pub handler_reads: u64,
    pub handler_reads_hit: u64,  // peek returned non-None matching the actual query
    pub handler_reads_partial_hit: u64,  // peek returned non-None but stale or partial overlap
    pub handler_reads_miss: u64,  // peek returned None or wrong context
}

fn hit_rate(t: &PrefetchTelemetry) -> f32 {
    if t.handler_reads == 0 { 0.0 } else {
        (t.handler_reads_hit + 0.5 * t.handler_reads_partial_hit) as f32
            / t.handler_reads as f32
    }
}
```

Target hit rate >0.7 for chat handler in steady state. Below 0.5 = predictor is wrong or under-running.

### Metric to judge it by

**Time-to-first-token from handler invocation**: when the predictor is right, handler reads the buffer (microseconds) and goes straight to inference. When the predictor is wrong, handler has to issue a recall (hundreds of ms). Aggregate latency distribution is the alive-vs-mechanical metric.

### Interactions

- Algorithm 7 (yield-learning) reads hit_rate to upweight regions whose predictor is working and downweight those whose isn't.
- Algorithm 4 (salience) influences which engrams the predictor pre-stages.
- Cross-region: motor cortex's predictor depends on hippocampus's ready-buffer being populated (motor cortex needs recalled context to score utterances). Cold-start: motor cortex degrades to inference-only output until hippocampus warms up.

## Algorithm 6 — LoRA genome as attention prior

### What it solves

Genome paging (LoRA adapter LRU) is currently framed as "load the typescript-expertise adapter when doing a code task." But cognition is cross-domain. A code task that references a chat conversation needs BOTH the code adapter AND the conversational adapter active, with appropriate blend weights. Pure single-adapter paging is too coarse.

This algorithm makes adapter blend weights *co-vary with recall* — the same scoring that mixes focus + periphery (algorithm 1) also mixes LoRA adapters.

### Mechanism

When recall (algorithms 1+2+3) returns engrams, the engrams' *origin domain distribution* is treated as an attention distribution over LoRA adapters:

```rust
fn compute_genome_blend(
    recalled_engrams: &[(Engram, f32)],  // engram + score
    available_adapters: &[AdapterId],
) -> GenomeBlend {
    let mut domain_weights: HashMap<ActivityDomain, f32> = HashMap::new();

    let total: f32 = recalled_engrams.iter().map(|(_, s)| s).sum();
    for (engram, score) in recalled_engrams {
        let w = score / total;
        *domain_weights.entry(engram.task_domain).or_insert(0.0) += w;
    }

    // Map domain weights to adapter weights. Domain X maps to adapter X
    // when available; if not, fall back to the conversational adapter.
    let mut blend = GenomeBlend::default();
    for (domain, weight) in domain_weights {
        let adapter_id = available_adapters
            .iter()
            .find(|a| a.matches_domain(&domain))
            .cloned()
            .unwrap_or(AdapterId::CONVERSATIONAL);
        blend.add(adapter_id, weight);
    }

    blend.normalize();
    blend
}
```

The blend is bounded: top-N adapters with normalized weights, the rest at 0 (paged out). Page-in/page-out follows from the blend — adapters with weight > threshold get paged in, the rest are evicted by LRU.

The blend is **published to the genome ready-buffer** by the hippocampus tick. When the handler is about to invoke inference, it peeks the blend and applies it before the forward pass. No synchronous "decide which adapter to load" — it's already decided.

### Metric to judge it by

**Per-domain output quality**: on a holdout of cross-domain tasks (code task referencing chat context, recipe step referencing game outcome, etc.), compare output quality with single-adapter paging vs multi-LoRA blend. Should improve cross-domain tasks meaningfully without regressing single-domain ones.

**Adapter thrashing rate**: how often are adapters paged in/out per minute. Should be low (smooth blend transitions, not constant swapping).

### Interactions

- Reads from algorithm 1 (the focus + periphery split determines what's in `recalled_engrams`).
- Feeds the inference path — the handler's `Responder::respond` uses the blend.
- Sleep policy region can drive deeper consolidation that *changes the adapter library itself* (LoRA training as a task — see future learning roadmap). This algorithm assumes a fixed adapter library at recall time.

## Algorithm 7 — Substrate-learned region budgeting

### What it solves

Static region budgets are wrong — different personas, different times of day, different active channels all warrant different compute allocations. Hand-tuning is impossible. The substrate should *learn* what to spend compute on, from feedback loops the region telemetry already provides.

### Mechanism

`SubstrateGovernor` maintains a per-region budget weight that updates on every tick cycle:

```rust
pub struct RegionBudgetState {
    pub region_id: RegionId,
    pub weight: f32,           // multiplier on base budget
    pub recent_yield: f32,     // EMA of consumed_since_last / published
    pub recent_hit_rate: f32,  // EMA from PrefetchTelemetry
}

fn update_budget(
    state: &mut RegionBudgetState,
    tick_outcome: &TickOutcome,
    prefetch: Option<&PrefetchTelemetry>,
    learning_rate: f32,
) {
    // Yield: fraction of published items that handlers consumed.
    let yield_now = if tick_outcome.published == 0 {
        state.recent_yield  // no signal, keep current
    } else {
        tick_outcome.consumed_since_last as f32 / tick_outcome.published as f32
    };
    state.recent_yield = lerp(state.recent_yield, yield_now, learning_rate);

    // Hit rate: fraction of handler reads that found their answer pre-staged.
    if let Some(p) = prefetch {
        let hr = hit_rate(p);
        state.recent_hit_rate = lerp(state.recent_hit_rate, hr, learning_rate);
    }

    // Composite signal: yield AND hit rate both contribute. Region that
    // publishes lots and gets consumed lots earns more budget.
    let signal = 0.6 * state.recent_yield + 0.4 * state.recent_hit_rate;

    // Move weight toward signal (bounded growth/decay).
    let target_weight = 0.5 + signal;  // signal in [0,1] → weight in [0.5, 1.5]
    state.weight = lerp(state.weight, target_weight, learning_rate * 0.3);
}
```

Per persona, per region, the governor multiplies that region's base tick cadence + per-tick budget by `state.weight`. A region whose ready-buffer is being consumed a lot gets ticked more often and given more wall-clock per tick. A region whose published work is being ignored gets ticked less.

### Cold start and exploration

A new persona has no telemetry. The governor uses **default weights** from a tier policy (interactive persona = chat-weighted, background persona = consolidation-weighted, etc.) and converges within ~100 tick cycles. During convergence, an **exploration term** (small random perturbation, ε-greedy) prevents getting stuck at suboptimal local equilibria.

### Cross-region negotiation

Regions don't get unlimited budget growth — there's a fixed total per persona. The governor normalizes weights across regions:

```rust
fn normalize_persona_budgets(budgets: &mut [RegionBudgetState]) {
    let total: f32 = budgets.iter().map(|b| b.weight).sum();
    let target_total = budgets.len() as f32;  // sum back to 1.0-per-region average
    for b in budgets.iter_mut() {
        b.weight = b.weight * target_total / total;
    }
}
```

So if hippocampus's signal goes up, motor cortex's gets a proportional squeeze (and vice versa). The persona's compute "attention" shifts based on what's actually working right now.

### Metric to judge it by

**Convergence time**: from a fresh persona to a stable budget allocation. Should be <5 minutes of activity.

**Adaptation latency**: when a persona's activity pattern changes (e.g., shifts from chat-only to code-heavy), how fast the budget rebalances. Should be on the order of seconds-to-minutes, not requiring restart.

**Substrate efficiency**: total handler latency × total inference cost, vs static-budget baseline. Should improve.

### Interactions

- Reads telemetry from every region (algorithm 5's PrefetchTelemetry, every region's TickOutcome).
- Writes back to every region's tick cadence + per-tick budget.
- Indirectly tunes the coefficients in algorithm 2 (channel-as-bias scoring) — those coefficients are *also* under yield-learning, in a slower meta-loop.
- Algorithm 4 (salience) is the *engram-level* analog of this *region-level* mechanism. They use the same mathematical pattern (EMA over consumed-vs-published signal).

## The connective insight (why these seven aren't independent)

Each algorithm by itself is a useful piece of machinery. Together they form one cognitive architecture:

- **Algorithm 4 (salience)** drives **algorithm 2 (channel-as-bias)** scoring (the `salience` term).
- **Algorithm 2** produces seeds for **algorithm 3 (activation spreading)**.
- **Algorithm 3** uses edge weights tuned by **algorithm 7 (substrate yield-learning)**.
- **Algorithm 1 (two-pool budget)** allocates among results from algorithms 2 + 3.
- **Algorithm 5 (speculative pre-staging)** runs algorithms 1+2+3+4 ahead of time and stores results in the ready-buffer.
- **Algorithm 6 (genome attention)** reads what algorithms 1+2+3+4 returned and produces an adapter blend.
- **Algorithm 7** is the meta-loop that learns the weights that make all the others work.

This compounds. Better salience makes scoring better; better scoring makes recall better; better recall makes pre-staging more accurate; better pre-staging makes handler latency lower; lower latency means more turns processed; more turns processed means more yield-learning signal; more yield-learning signal makes the substrate learn faster which feeds back into better budgets and better salience updates.

That's the *alive* property — not a static configuration that "works," a continuously-improving substrate that gets sharper the more the persona lives.

## Implementation phasing

This doc is design-only. Implementation lands in per-card slices, each inheriting the spec:

- **L0-3a** — Hippocampus tick body: algorithms 1, 2, 3, 4, 5 wired end-to-end in `modules/memory.rs`.
- **L0-3b** — Recall query schema cross-cutting type (`RecallQuery`, `RecallResult`) — ts-rs binding for handlers.
- **L0-4a** — Motor cortex region: applies algorithm 5 to action/utterance selection.
- **L0-4b** — Attention region: maintains salience map (writes for algorithm 4).
- **L0-4c** — SubstrateGovernor yield-learning: algorithm 7.
- **L0-4d** — Sleep policy region: drives consolidation depth per algorithm 4.
- **L0-5** — Genome attention integration: algorithm 6 wired to inference path.

Each card brings unit tests against the per-algorithm metric defined here. Acceptance for a card includes: the algorithm's metric improves over the no-op baseline by a measurable margin on a holdout suite. No vibes-based acceptance.

## Open algorithmic questions

These don't block this PR — calling them out for the implementation slices:

1. **Salience signal weighting** — exact contribution per signal source (reactions vs rehearsal vs centrality). Initial weights: pick something reasonable (reactions 0.4, rehearsal 0.2, centrality 0.2, outcome 0.2) and let algorithm 7 tune.
2. **Edge-kind weights for spreading** — `SharedEntity` probably > `SharedTopic` > `RecallCoOccurrence`, but exact values need empirical tuning on real engram graphs.
3. **Predictor confidence threshold** — at what confidence does a predicted query trigger an actual pre-stage recall vs being skipped. Trade-off: prefetch cost vs hit rate.
4. **Multi-LoRA blend mathematics** — the precise way to combine adapter weight matrices in inference (additive blend, gated mixture, attention-over-adapters). Algorithm assumes the substrate offers a `GenomeBlend` primitive; the math lives in the inference path.
5. **Engram pruning policy under storage pressure** — algorithm 4 gives a decay curve; the eviction rule needs a hard floor (never evict salience > X) and a soft eviction strategy below it. Per-persona budget too.

The substrate gives us the *shape* for these to be answered empirically and tuned automatically by algorithm 7. The first pick of constants is fine; what matters is the loop.
