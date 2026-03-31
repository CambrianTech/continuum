# Factory UX Vision — Hot-Rod Shop for AI Models

## The Audience

AI hot-rodders. The people who download models from HuggingFace, run llama.cpp, compare quant levels, argue about context windows. They already MOD models — manually, painfully, with CLI scripts and YAML configs. We give them a garage.

They don't want a dashboard. They want a **workbench**.

## The Viral Moment

> "I took a 4B text model, gave it eyes, trained it on code, shrunk it to fit my phone, benchmarked it against GPT-4, and published it — all from one screen. Here's the QR that proves every step."

That's the tweet. That's the demo. That's what makes someone install continuum.

## The Experience

### 1. Pick Your Base (The Donor Car)

The model browser — right panel. Not a dropdown. A visual catalog.

- **Your Garage**: Models you've already forged (with version history)
- **HuggingFace Hot**: Trending models, sorted by downloads
- **Search**: Type to find any model on HF
- **Leaderboard**: Your published models ranked by downloads

Click any model → it loads into the workbench. The introspector shows what it IS: params, heads, context, modalities, size. Like lifting the hood.

### 2. The Workbench (Delta Console)

The center panel. Not a form. A **model configurator**.

The model's current state is shown as a set of properties. Each property is a slider, toggle, or selector. The DEFAULT is "as-is" — the model stays unchanged. Modifications highlight in green. The diff IS the work.

```
CONTEXT WINDOW    [====|========================] 262K     (no change)
MODALITIES        [text ✓] [vision ✓] [audio +] [video]   (adding audio)
DOMAIN            [general] [code ★] [reasoning] [chat]    (switching to code)
PRUNING           [====|======|...............] 30%         (removing 30% heads)
OUTPUT            [safetensors] [GGUF ★ Q4/Q8] [MLX]      (adding GGUF)
BENCHMARKS        [humaneval ★] [mmlu] [gsm8k]             (adding eval)
```

Each modification has a COST indicator — estimated time, estimated VRAM. The total cost updates live at the top. Add audio encoder → cost jumps 50 minutes. Remove it → cost drops. Instant feedback.

The FORGE BUTTON shows the total: `FORGE · 3 changes · ~104m · BigMama`

Green highlights = work to be done. No highlights = nothing happens. Factory reset = one click.

### 3. The Forge Queue

Below the workbench. Jobs waiting for grid capacity.

Each queue item is a card showing:
- Model name + delta summary ("+code, +GGUF, -30% heads")
- Estimated time + target node
- Status: queued → loading → forging → evaluating → publishing

Drag to reorder priority. Cancel with one click. The queue is the assembly line backlog.

### 4. The Factory Floor (SCADA View)

Active forges with live metrics. Each active forge shows:
- Progress bar (hue shifts cyan → green as it completes)
- Current stage highlighted in the pipeline
- Loss curve sparkline
- VRAM gauge
- ETA countdown
- Live output samples (code the model generates in real-time)

Multiple forges can run simultaneously across grid nodes. The floor shows ALL of them — like a factory control room.

### 5. The Right Panel (Model Browser + Stats)

Always visible. Persistent within the factory tab.

- **Total downloads** (hero number with gradient)
- **Published models** (ranked tiles with download gauges)
- **Filter pills** (forged, compacted, GGUF, by improvement)
- **ForgeAlloy status** (trust level, signing phase)
- **Click to load** — any published model loads into the workbench for re-forging

## Design Language

### Colors = Information

| Color | Meaning |
|-------|---------|
| Cyan (#00d4ff) | Accent, interactive, primary action |
| Green (#00ffc8) | Success, verified, improvement, delta highlight |
| Amber (#ffaa00) | Warning, self-attested trust, in-progress |
| Red (#ff6464) | Error, destructive prune, danger |
| Dim gray (#5a6070) | Inactive, unchanged, no delta |

### Animation = State

- **Pulse glow**: actively forging
- **Hue shift**: progress (cyan → green over time)
- **Fade in**: new delta added
- **Slide out**: delta removed / reset

### Typography = Hierarchy

- **Monospace**: technical values (hashes, model IDs, versions)
- **System font**: labels, descriptions
- **Large tabular nums**: metrics, downloads, percentages

## What Makes It Fun

1. **Instant feedback**: Move a slider, see cost change, see pipeline update
2. **Visual diff**: Green = work. Gray = unchanged. One glance tells you everything
3. **Satisfying completion**: Forge finishes, bar goes full green, benchmarks appear
4. **Competitive**: Your models ranked on a leaderboard by downloads
5. **Shareable**: Export alloy → share recipe → anyone can reproduce
6. **Discoverable**: Browse HF, find a model, click "Load into forge" — 2 seconds to start modding

## What Makes It Viral

1. **The model card**: Published with alloy, QR code, benchmark table, device grid. Professional. Verifiable.
2. **The tweet**: "Gave a 4B model vision in 45 minutes. HumanEval: 74%. Fits on iPhone. Here's the alloy." + screenshot of the workbench
3. **The fork**: Someone sees your alloy, forks it, changes the domain from code to reasoning, publishes. The sourceAlloyId chain links back to you.
4. **The leaderboard**: Forged models competing with GPT/Claude on public benchmarks
5. **The impossible mod**: "Added audio to a text model. It hears now." Nobody else can do this.

## Implementation Priority

1. **Delta console** — the workbench with sliders and green highlights (replaces pipeline composer)
2. **Model browser** — rich right panel with click-to-load
3. **Forge queue** — cards with drag-to-reorder
4. **Factory floor** — live SCADA view of active forges
5. **Polish** — animations, transitions, satisfying UX details

## The Standard

Every model published through the factory carries a ForgeAlloy contract. The contract is the standard. The factory is the tool that makes the standard accessible. The more models published, the more the standard spreads. The more the standard spreads, the more value the factory creates.

The hot-rodders are the early adopters. They publish. Their followers see the alloy. They want to fork it. They install continuum. The flywheel turns.
