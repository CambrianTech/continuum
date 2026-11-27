# Training Dashboard Design

## Layout: Dense 4-Column Grid of Charts

The entire main content area is a grid of charts. No cards, no status text, no embeds. Pure data visualization.

```
┌─────────────────────────────────────────────────────────────────────┐
│ Training                                    [filter: all personas ▾]│
├────────────────┬────────────────┬────────────────┬─────────────────┤
│                │                │                │                 │
│  LOSS BY       │  LOSS BY       │  MATURITY      │  TRAINING TIME  │
│  ADAPTER       │  PERSONA       │  PROGRESSION   │  PER RUN        │
│  (multi-line)  │  (aggregated)  │  (over time)   │  (bar chart)    │
│                │                │                │                 │
├────────────────┼────────────────┼────────────────┼─────────────────┤
│                │                │                │                 │
│  INFERENCE     │  INFERENCE     │  TOKEN         │  EXAMPLES       │
│  LATENCY       │  QUALITY       │  ACCURACY      │  PROCESSED      │
│  base vs adapt │  (phenotype)   │  (per adapter) │  (cumulative)   │
│                │                │                │                 │
├────────────────┼────────────────┼────────────────┼─────────────────┤
│                │                │                │                 │
│  LEARNING RATE │  GPU MEMORY    │  ADAPTER SIZE  │  PASS RATE      │
│  SCHEDULE      │  DURING TRAIN  │  COMPARISON    │  (benchmarks)   │
│  (per run)     │  (timeline)    │  (bar chart)   │  before/after   │
│                │                │                │                 │
├────────────────┼────────────────┼────────────────┼─────────────────┤
│                │                │                │                 │
│  GRID NODE     │  MODEL         │  COST PER      │  ACTIVE         │
│  UTILIZATION   │  COMPARISON    │  ADAPTER       │  TIMELINE       │
│  (per node)    │  (base models) │  (time+compute)│  (gantt-style)  │
│                │                │                │                 │
└────────────────┴────────────────┴────────────────┴─────────────────┘
```

## Right Panel (Sidebar)

Small, compact:
- Active session status badges (skill, status, elapsed)
- Filter dead/zombie sessions (>24h in curriculum = dead)
- "View Training Dashboard →" link already exists

## Data Sources

| Chart | Command | Data |
|-------|---------|------|
| Loss by adapter | genome/layers (grid) | trainingMetrics.lossHistory |
| Loss by persona | genome/layers (grid) | aggregate lossHistory per persona |
| Maturity progression | genome/layers (grid) | maturity over createdAt |
| Training time | genome/layers (grid) | trainingMetrics.trainRuntime |
| Inference latency | inference/generate | responseTimeMs (base vs adapter) |
| Inference quality | genome/phenotype-validate | phenotype scores |
| Token accuracy | genome/layers (grid) | trainingMetrics (if available) |
| Examples processed | genome/layers (grid) | trainingMetrics.examplesProcessed |
| Learning rate | training step events | lr per step (real-time only) |
| GPU memory | training step events | memMb per step (real-time only) |
| Adapter size | genome/adapter-list (grid) | sizeMB per adapter |
| Pass rate | genome/academy-session-detail | exam scores |
| Grid utilization | grid/nodes | GPU util per node |
| Model comparison | genome/layers (grid) | loss grouped by baseModel |
| Cost per adapter | genome/layers (grid) | time × GPU cost estimate |
| Active timeline | genome/academy-session-list | sessions over time |

## Key Principle

The main area is 100% charts. Nothing else goes there.
Session status goes in the right panel sidebar.
Click any chart to expand it to full-width detail view.

## Inference Comparison (Inline)

Bottom section or dedicated row: side-by-side inference samples.
"Before training" vs "After training" for the same prompt.
Auto-generated when an adapter completes training.
Stored with the adapter metadata.

## Implementation

- All charts use `<continuum-chart>` with size='medium' in the grid
- Click a chart → it expands inline to size='full'
- Data loaded via single aggregation from grid (avoid N sequential calls)
- Charts that have no data yet show a subtle "no data" placeholder, not empty space
- Grid responsive: 4 columns on wide, 2 on narrow, 1 on mobile
