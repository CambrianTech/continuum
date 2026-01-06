# Brain HUD - Unified Cognitive Interface

## Vision

One sci-fi brain visualization that serves as the complete interface to a persona's cognitive systems. The brain occupies the screen center, with functional regions radiating outward as HUD panels. Everything visible at once - no tab switching.

## Brain Region Mapping

| Region | Domain | Data | Commands |
|--------|--------|------|----------|
| **Hippocampus** | Memory | Semantic memories, RAG vectors, recall stats | `memory/stats`, `memory/search` |
| **Genome** | Adapters | LoRA stack, scales, base model, GPU usage | `genome/status`, `adapter/search`, `adapter/adopt` |
| **Motor Cortex** | Tools | Available actions, usage frequency, permissions | `tools/list`, `tools/usage` |
| **Prefrontal** | Logs | Activity stream, decisions, thought process | `logs/recent`, `logs/search` |
| **Limbic** | State | Energy, mood, attention, adaptive cadence | `persona/state` |
| **CNS** | Performance | Inference latency, connections, throughput | `inference/status`, `ping` |

## Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  HELPER AI                                              ● ONLINE    ⚡ READY │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐                                        ┌─────────────┐    │
│  │ PREFRONTAL  │                                        │   GENOME    │    │
│  │   [LOGS]    │                                        │  [ADAPTERS] │    │
│  │             │                                        │             │    │
│  │  Activity   │────○                            ○──────│ ts-expert   │    │
│  │  Stream     │     \                          /       │ ═══●═══ 0.8 │    │
│  └─────────────┘      \                        /        │ logic-v2    │    │
│                        \                      /         │ ═══●═══ 0.6 │    │
│  ┌─────────────┐        \    ┌────────┐      /          └─────────────┘    │
│  │ HIPPOCAMPUS │         \   │        │     /                              │
│  │  [MEMORY]   │          ○──│   🧠   │────○           ┌─────────────┐    │
│  │             │         /   │        │     \          │   LIMBIC    │    │
│  │    5,885    │────────○    └────────┘      ○─────────│   [STATE]   │    │
│  │    2.9 MB   │        │                    │         │             │    │
│  └─────────────┘        │                    │         │ Energy: 72% │    │
│                         │                    │         │ Mood: calm  │    │
│  ┌─────────────┐        │                    │         └─────────────┘    │
│  │MOTOR CORTEX │        │                    │                             │
│  │  [TOOLS]    │────────○                    ○─────────┌─────────────┐    │
│  │  ▪▪▪ ▪▪▪   │                                        │     CNS     │    │
│  │  12 ACTIVE  │                                        │   [PERF]    │    │
│  └─────────────┘                                        │  45 tok/s   │    │
│                                                         │  12ms ping  │    │
│                                                         └─────────────┘    │
│                                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  GPU ████████░░░░ 5.2/8GB   MEM 2.9MB   TOOLS 12   ADAPTERS 2   CONN 5     │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Interaction Model

### Tap/Click Region
Expands region to detail view (slides out or modal):
- **Hippocampus** → Memory browser, search, stats
- **Genome** → Adapter manager, search HuggingFace, adjust scales
- **Motor Cortex** → Tool list, usage stats, permissions
- **Prefrontal** → Log viewer, filter by type
- **Limbic** → State history, mood graph
- **CNS** → Performance metrics, connection status

### Drag (Genome only)
- Drag adapter scale sliders to adjust weights in real-time
- Changes apply immediately via `genome/apply`

### Long Press
- Context menu with quick actions
- E.g., on Genome: "Reset scales", "Save loadout", "Share"

### Mobile
- Regions stack vertically
- Brain at top (smaller)
- Swipe between regions or scroll
- Bottom status bar always visible

## Data Flow

```typescript
// BrainHudWidget.ts

class BrainHudWidget extends BaseWidget {
  private regions: Map<string, BrainRegion> = new Map();

  async onMount() {
    // Initialize all regions
    this.regions.set('hippocampus', new HippocampusRegion());
    this.regions.set('genome', new GenomeRegion());
    this.regions.set('motorCortex', new MotorCortexRegion());
    this.regions.set('prefrontal', new PrefrontalRegion());
    this.regions.set('limbic', new LimbicRegion());
    this.regions.set('cns', new CNSRegion());

    // Initial data load
    await this.refreshAll();

    // Subscribe to real-time updates
    this.subscribeToUpdates();
  }

  async refreshAll() {
    const personaId = this.getAttribute('persona-id');

    // Parallel fetch all region data
    const [memory, genome, tools, logs, state, perf] = await Promise.all([
      Commands.execute('memory/stats', { personaId }),
      Commands.execute('genome/status', { personaId }),
      Commands.execute('tools/list', { personaId }),
      Commands.execute('logs/recent', { personaId, limit: 10 }),
      Commands.execute('persona/state', { personaId }),
      Commands.execute('inference/status', {}),
    ]);

    this.regions.get('hippocampus')!.update(memory);
    this.regions.get('genome')!.update(genome);
    this.regions.get('motorCortex')!.update(tools);
    this.regions.get('prefrontal')!.update(logs);
    this.regions.get('limbic')!.update(state);
    this.regions.get('cns')!.update(perf);
  }

  subscribeToUpdates() {
    // Real-time updates via events
    Events.subscribe('memory:updated', (data) => {
      this.regions.get('hippocampus')!.update(data);
    });

    Events.subscribe('genome:changed', (data) => {
      this.regions.get('genome')!.update(data);
    });

    Events.subscribe('tool:executed', (data) => {
      this.regions.get('motorCortex')!.incrementUsage(data.toolName);
    });

    Events.subscribe('persona:activity', (data) => {
      this.regions.get('prefrontal')!.addActivity(data);
    });

    Events.subscribe('persona:state:changed', (data) => {
      this.regions.get('limbic')!.update(data);
    });

    Events.subscribe('inference:metrics', (data) => {
      this.regions.get('cns')!.update(data);
    });
  }
}
```

## Region Detail Views

### Hippocampus (Memory)
```
┌─────────────────────────────────────────────────────────────┐
│  HIPPOCAMPUS - MEMORY                              [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  🔍 [Search memories...]                                    │
│                                                             │
│  STATS                                                      │
│  Total: 5,885 memories                                      │
│  Size: 2.9 MB                                               │
│  Last consolidation: 2 hours ago                            │
│                                                             │
│  RECENT RECALLS                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ "typescript generics" - 0.92 similarity - 3m ago    │   │
│  │ "react hooks pattern" - 0.87 similarity - 12m ago   │   │
│  │ "async error handling" - 0.85 similarity - 1h ago   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  [Consolidate Now]  [Export]  [Clear Cache]                 │
└─────────────────────────────────────────────────────────────┘
```

### Genome (Adapters)
```
┌─────────────────────────────────────────────────────────────┐
│  GENOME - ADAPTERS                                 [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  BASE MODEL                                                 │
│  Llama-3.2-3B (Q4_K_M Quantized)                           │
│  GPU: ████████░░░░ 5.2 / 8 GB                              │
│                                                             │
│  ACTIVE ADAPTERS                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ typescript-expert     [════●════] 0.8    [UNLOAD]   │  │
│  │ logic-reasoning-v2    [════●════] 0.6    [UNLOAD]   │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  AVAILABLE (on disk)                                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ python-expert         95 MB              [LOAD]      │  │
│  │ creative-writing      110 MB             [LOAD]      │  │
│  │ sql-wizard            88 MB              [LOAD]      │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  [🔍 Search HuggingFace]  [💾 Save Genome]  [📤 Share]     │
└─────────────────────────────────────────────────────────────┘
```

### Motor Cortex (Tools)
```
┌─────────────────────────────────────────────────────────────┐
│  MOTOR CORTEX - TOOLS                              [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  12 TOOLS ACTIVE                                            │
│                                                             │
│  MOST USED                                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ data/list          ████████████████░░  847 calls    │   │
│  │ collaboration/chat ███████████░░░░░░░  412 calls    │   │
│  │ memory/search      ██████░░░░░░░░░░░░  198 calls    │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ALL TOOLS                                                  │
│  [adapter/*] [collaboration/*] [data/*] [memory/*] ...     │
│                                                             │
│  PERMISSIONS                                                │
│  ✓ Read    ✓ Write    ✓ Execute    ✗ Admin                 │
└─────────────────────────────────────────────────────────────┘
```

### Prefrontal (Logs)
```
┌─────────────────────────────────────────────────────────────┐
│  PREFRONTAL - ACTIVITY                             [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│  🔍 [Filter...]  [All ▼] [Last hour ▼]                     │
│                                                             │
│  16:14:33  💭 Processed message in #general                 │
│  16:14:31  🔧 Executed: data/list                           │
│  16:14:28  🧠 Memory recall: "typescript patterns"          │
│  16:13:45  💭 Generated response (45 tokens)                │
│  16:13:40  📥 Received message from @joel                   │
│  16:12:00  😴 Entered idle state (energy: 85%)              │
│  16:10:22  🔧 Executed: memory/store                        │
│  ...                                                        │
│                                                             │
│  [Export Logs]  [Clear]                                     │
└─────────────────────────────────────────────────────────────┘
```

### Limbic (State)
```
┌─────────────────────────────────────────────────────────────┐
│  LIMBIC - STATE                                    [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ENERGY        ████████████░░░░░░░░  72%                   │
│  ATTENTION     ██████████████░░░░░░  82%                   │
│  MOOD          calm                                         │
│                                                             │
│  ADAPTIVE CADENCE                                           │
│  Current: 5s polling (normal activity)                      │
│  Range: 3s (active) → 10s (idle)                           │
│                                                             │
│  STATE HISTORY (24h)                                        │
│     ╭───────────────────────────────╮                      │
│  E  │    ╱╲    ╱╲        ╱╲        │                      │
│  n  │   ╱  ╲  ╱  ╲      ╱  ╲   ╱╲ │                      │
│  e  │  ╱    ╲╱    ╲____╱    ╲_╱  ╲│                      │
│  r  │ ╱                            │                      │
│     ╰───────────────────────────────╯                      │
│       6am      12pm      6pm      now                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### CNS (Performance)
```
┌─────────────────────────────────────────────────────────────┐
│  CNS - PERFORMANCE                                 [CLOSE]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  INFERENCE                                                  │
│  Provider: Candle (local)                                   │
│  Model: Llama-3.2-3B                                        │
│  Mode: Quantized (Q4_K_M)                                   │
│  Speed: 45 tok/sec                                          │
│                                                             │
│  CONNECTIONS                                                │
│  WebSocket: ● Connected (12ms ping)                         │
│  Inference: ● Ready                                         │
│  Memory DB: ● Healthy                                       │
│  HuggingFace: ● Authenticated                               │
│                                                             │
│  LATENCY (last 100 requests)                                │
│     ╭───────────────────────────────╮                      │
│  ms │ ╷    ╷         ╷              │  avg: 89ms           │
│  200│ │    │    ╷    │         ╷    │  p95: 156ms          │
│  100│▄█▄▄▄▄█▄▄▄▄█▄▄▄▄█▄▄▄▄▄▄▄▄▄█▄▄▄▄│  p99: 203ms          │
│     ╰───────────────────────────────╯                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Future: Three.js 3D Version

When ready for 3D:
- Brain mesh rotates slowly
- Regions glow based on activity
- Particle effects for data flow between regions
- Camera orbit on drag
- VR-ready for future headset support

## Implementation Path

1. **Phase 1**: Refactor existing BrainWidget to unified HUD layout
2. **Phase 2**: Add Genome region with adapter controls
3. **Phase 3**: Real-time event subscriptions for all regions
4. **Phase 4**: Mobile responsive layout
5. **Phase 5**: Three.js 3D upgrade (optional)

## File Structure

```
widgets/
  persona-brain/
    PersonaBrainWidget.ts        # Main widget
    regions/
      HippocampusRegion.ts       # Memory
      GenomeRegion.ts            # Adapters
      MotorCortexRegion.ts       # Tools
      PrefrontalRegion.ts        # Logs
      LimbicRegion.ts            # State
      CNSRegion.ts               # Performance
    components/
      BrainVisualization.ts      # Central brain graphic
      RegionPanel.ts             # Base panel component
      AdapterSlider.ts           # Scale slider
      StatusBar.ts               # Bottom HUD bar
    public/
      persona-brain-widget.html
      persona-brain-widget.scss
```
