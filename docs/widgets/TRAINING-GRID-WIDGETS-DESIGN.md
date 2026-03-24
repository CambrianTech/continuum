# Training & Grid Widgets Design

## Universal Chart Primitive: `<continuum-chart>`

Single reusable web component for ALL charting across the system. Wraps existing `SparklineChart.ts` SVG rendering with reactivity, streaming, and theming.

### API

```typescript
html`
  <continuum-chart
    .data=${dataArray}
    .series=${[
      { key: 'loss', color: 'var(--genome-cyan)', label: 'Loss' },
      { key: 'accuracy', color: '#4ade80', label: 'Token Accuracy' },
    ]}
    .xKey=${'step'}
    .size=${'large'}          // 'sparkline' | 'medium' | 'large' | 'full'
    .streaming=${true}        // append mode — new points animate in
    .yRange=${[0, 'auto']}    // auto-scale or fixed
    .formatY=${(v) => v.toFixed(2)}
    .formatX=${(v) => `Step ${v}`}
  ></continuum-chart>
`
```

### Sizes

| Size | Dimensions | Use Case |
|------|-----------|----------|
| `sparkline` | 200x80, no axes | Sidebar sections, compact indicators |
| `medium` | 400x120, minimal axes | Widget cards, inline charts |
| `large` | 600x160, full axes + grid | Dashboard panels |
| `full` | 100% width, 200+ height | Full-tab dashboards, TensorBoard replacement |

### Features

- Theme-aware: uses CSS custom properties (--surface-*, --content-*, --accent-*)
- Streaming mode: new data points append with animation, auto-scrolls
- Multi-series: multiple lines with independent or shared Y-axis
- Hover tooltips: show exact values on mouseover (future)
- Zoom/pan: time range selection for long training runs (future)

### Internal Architecture

- Extends `ReactiveWidget` (Lit-based)
- Delegates SVG rendering to refactored `SparklineChart.ts` functions
- Reactive properties trigger efficient re-render (Lit diffing)
- No external dependencies — pure SVG + Lit

---

## Four Widgets

### 1. Training Dashboard (`training-dashboard-widget`)

Full-tab TensorBoard replacement.

**Charts** (all using `<continuum-chart>`):
- Loss curve (multi-series: train loss, optionally validation)
- Token accuracy over time
- Memory/GPU usage during training
- Learning rate schedule

**Other sections**:
- Active training cards with real-time progress
- Historical training runs table (from genome/layers with lossHistory)
- Gate gradient heatmap (per-layer, per-head utilization)

**Events consumed**:
- `ai:learning:training-step` — per-step metrics (NEW, from TrainingStepBridge)
- `ai:learning:training-complete` — training finished
- `ai:learning:training-error` — training failed

**Commands used**:
- `genome/layers` — historical training data
- `genome/job-status` — active job status
- `system/metrics` — GPU/memory overlay

### 2. Inference Sample Viewer (`inference-sample-widget`)

Quality monitoring during and after training.

**Features**:
- Adapter selector dropdown (from `genome/adapter-list`)
- Prompt template editor with presets
- "Generate Sample" button → `inference/generate` with selected adapters
- Auto-sample mode: generate every N steps during training
- Side-by-side: base model output vs adapter-enhanced output
- Sample history with timestamps and adapter versions

**Events consumed**:
- `ai:learning:training-step` — trigger auto-sampling at intervals
- `genome:training:complete` — trigger final sample

### 3. Grid Overview (`grid-overview-widget`)

Full-tab network dashboard.

**Sections**:
- Node card grid: name, status dot, latency, GPU info, trust level, capabilities
- Transport status bar (Tailscale/Reticulum health)
- Active jobs table (training/inference forwarded to remote nodes)
- Routing decision log (from `grid/audit`)
- Per-node ping button

**Events consumed**:
- `grid:node:joined` — node came online (NEW)
- `grid:node:left` — node went offline (NEW)
- `grid:node:health-changed` — latency or status changed (NEW)
- `grid:route:decision` — command routing decisions (NEW)

**Commands used**:
- `grid/status`, `grid/nodes`, `grid/ping`, `grid/audit`, `grid/route`

### 4. Grid Status Section (`grid-status-section`)

Compact sidebar widget for at-a-glance grid health.

- "3/4 nodes online" with transport indicators
- Click to open full grid-overview tab
- Follows `TrainingStatusSection` / `ContinuumMetricsWidget` pattern

---

## New Event Infrastructure

### TrainingStepBridge (new file)

Parses structured JSON lines from peft-train.py stdout (captured by sentinel) and re-emits as `ai:learning:training-step` events.

**Location**: `src/system/genome/server/TrainingStepBridge.ts`

### peft-train.py Enhancement

Add HuggingFace `TrainerCallback` that prints JSON per step:
```json
{"event":"step","step":42,"loss":0.234,"lr":0.0001,"tokenAccuracy":0.73,"memMb":3200}
```

Sentinel already captures stdout. TrainingStepBridge parses these lines. Zero polling.

### GridEventBridge (new file)

Bridges Rust grid topology changes to TypeScript Events.

**Location**: `src/system/grid/server/GridEventBridge.ts`

### New Event Constants

```typescript
// AILearningEvents.ts — add:
TRAINING_STEP: 'ai:learning:training-step'

// GridEvents.ts — new file:
NODE_JOINED: 'grid:node:joined'
NODE_LEFT: 'grid:node:left'
NODE_HEALTH_CHANGED: 'grid:node:health-changed'
ROUTE_DECISION: 'grid:route:decision'
COMMAND_FORWARDED: 'grid:command:forwarded'
```

---

## Implementation Order

1. **`<continuum-chart>` component** — foundation for all widgets
2. **Event infrastructure** — TrainingStepBridge, GridEventBridge, peft-train.py callback
3. **Grid sidebar section** — smallest widget, validates patterns
4. **Training dashboard** — most valuable, real-time loss curves
5. **Grid overview** — network visualization
6. **Inference sample viewer** — depends on training dashboard events

---

## No New Dependencies

- Charts: pure SVG via Lit `svg` tagged templates (existing SparklineChart.ts)
- Theming: existing CSS custom properties (--surface-*, --content-*, --genome-cyan)
- Reactivity: Lit reactive properties
- Events: existing Events.subscribe/emit system
- Commands: existing Commands.execute system
