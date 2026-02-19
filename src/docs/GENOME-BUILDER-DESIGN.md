# Genome Builder - Visual Assembly Interface

> **Note**: This is now part of the unified Brain HUD. See [BRAIN-HUD-DESIGN.md](./BRAIN-HUD-DESIGN.md) for the integrated design where Genome is one region of the brain visualization.

## Vision

A game-like, mobile-first interface for assembling AI personas from modular components:
- **Adapters** = Skills/abilities (LoRA weights)
- **Genomes** = Skill loadouts (adapter stacks with scales)
- **Base Models** = The foundation (quantized or BF16)

## Design Principles

### 1. Video Game Feel
- **Skill trees** for browsing adapters by category
- **Loadout slots** for active adapters (limited by GPU memory)
- **XP/Progression** from training your own adapters
- **Achievements** for discovering good combinations
- **Leaderboards** for community-shared genomes

### 2. Mobile-First (iPhone/iPad)
- Touch-optimized drag-and-drop
- Swipe gestures for adapter scales (0.0-1.0 sliders)
- Portrait and landscape layouts
- Offline mode (local adapters work without internet)
- Haptic feedback on actions

### 3. Visual Wiring (Scratch-like)
- Nodes represent components (adapters, base models, quantization)
- Connections show data flow
- Color-coded by category (code=blue, creative=purple, etc.)
- Snap-to-grid for clean layouts

## Components

### Adapter Card
```
┌─────────────────────────┐
│ 🔧 typescript-expert    │
│ ───────────────────     │
│ Base: Llama-3.2-3B      │
│ Rank: 64 | Size: 120MB  │
│ ★★★★☆ (4.2k downloads)  │
│                         │
│ Scale: [══════●═══] 0.7 │
│                         │
│ [Try] [Adopt] [Details] │
└─────────────────────────┘
```

### Genome Stack (Loadout)
```
┌───────────────────────────────────────────────┐
│ 🧬 "Senior Developer" Genome                  │
│ ─────────────────────────────────────────     │
│ Base: Llama-3.2-3B (Quantized Q4_K_M)         │
│                                               │
│ Active Adapters:                              │
│ ┌─────────┐ ┌─────────┐ ┌─────────┐          │
│ │TS Expert│→│Code Rev │→│Git Pro  │          │
│ │  0.8    │ │  0.5    │ │  0.3    │          │
│ └─────────┘ └─────────┘ └─────────┘          │
│                                               │
│ Memory: [████████░░] 5.2/8 GB                 │
│ Est. Speed: ~45 tok/sec                       │
│                                               │
│ [▶ Activate] [📤 Share] [✏️ Edit]            │
└───────────────────────────────────────────────┘
```

### Search/Browse View
```
┌───────────────────────────────────────────────┐
│ 🔍 [Search adapters...                    ]   │
│ ─────────────────────────────────────────     │
│ Categories:                                   │
│ [Code] [Writing] [Creative] [Language] [All]  │
│                                               │
│ Sort: [Downloads ▼] Filter: [Llama-3.2 ▼]    │
│                                               │
│ ┌──────────────┐ ┌──────────────┐            │
│ │python-expert │ │react-master  │            │
│ │★★★★★ 12.5k  │ │★★★★☆ 8.2k   │            │
│ └──────────────┘ └──────────────┘            │
│ ┌──────────────┐ ┌──────────────┐            │
│ │rust-systems  │ │sql-wizard    │            │
│ │★★★★☆ 5.1k   │ │★★★★☆ 4.8k   │            │
│ └──────────────┘ └──────────────┘            │
└───────────────────────────────────────────────┘
```

## Command Mapping

| UI Action | Command | Description |
|-----------|---------|-------------|
| Search | `adapter/search` | Query HuggingFace/local/mesh |
| Try | `adapter/try` | A/B test before adopting |
| Adopt | `adapter/adopt` | Add to persona genome |
| Activate | `genome/apply` | Load adapter stack |
| Check Memory | `genome/status` | GPU usage/pressure |
| Share | `genome/export` | Export as shareable config |

## Implementation Phases

### Phase 1: Core Widget
- [ ] GenomeBuilderWidget extends BaseWidget
- [ ] Adapter card component (display only)
- [ ] Genome stack visualization
- [ ] Memory usage meter
- [ ] Search integration via `adapter/search`

### Phase 2: Interaction
- [ ] Drag-and-drop adapters into genome slots
- [ ] Slider controls for adapter scales
- [ ] Try button with A/B comparison display
- [ ] Adopt button with confirmation

### Phase 3: Mobile
- [ ] Touch gesture support
- [ ] Responsive layouts (phone/tablet)
- [ ] Offline adapter cache
- [ ] Haptic feedback

### Phase 4: Game Mechanics
- [ ] Achievement system for genome combinations
- [ ] Progress tracking for training own adapters
- [ ] Community genome sharing
- [ ] Leaderboards (optional)

### Phase 5: Visual Wiring (Scratch-like)
- [ ] Node-based editor for complex stacks
- [ ] Visual connections between components
- [ ] Branching/conditional adapter chains
- [ ] Save/load visual layouts

## Technical Notes

### Commands Are Already AI-Safe
Adapters commands are exposed to personas as tools:
```typescript
// Persona can search for skills
await Commands.execute('adapter/search', { query: 'typescript', baseModel: 'llama-3.2' });

// Persona can try before adopting
await Commands.execute('adapter/try', { adapterId: 'user/ts-expert', testPrompt: '...' });

// Persona can adopt skills
await Commands.execute('adapter/adopt', { adapterId: 'user/ts-expert', scale: 0.7 });
```

### Real-Time Memory Updates
Subscribe to genome state changes:
```typescript
Events.subscribe('genome:memory:updated', (state) => {
  // Update memory meter
  memoryMeter.value = state.memoryUsedMB / state.memoryBudgetMB;
});
```

### Mobile PWA
The widget system already supports PWA mode - GenomeBuilder would work as:
- Standalone app via "Add to Home Screen"
- Offline-capable with adapter caching
- Push notifications for training completion

## Open Questions

1. **Persona self-modification**: Should personas be able to modify their own genomes via tools?
2. **Training integration**: How to visualize ongoing fine-tuning progress?
3. **Multi-device sync**: Cloud sync for genome configurations?
4. **Marketplace**: Community adapter marketplace with ratings/reviews?

## References

- PersonaGenome.ts - Current implementation
- adapter/* commands - Search/try/adopt workflow
- CandleGrpcAdapter - Rust inference integration
- MIT Scratch - Visual programming inspiration
