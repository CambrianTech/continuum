# HUD Vision: Immersive Single-Pane Interface

> "Not widgets in boxes. One cohesive display you're INSIDE of."

---

## The Problem with Current UI

```
┌─────────────────────────────────────────────────────────────────┐
│  CURRENT: Traditional Web Layout                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────┐  ┌────────────────────────┐  ┌──────────────────┐ │
│  │          │  │                        │  │                  │ │
│  │  SIDEBAR │  │      MAIN CONTENT      │  │   RIGHT PANEL    │ │
│  │          │  │                        │  │                  │ │
│  │          │  │    [ Empty space ]     │  │                  │ │
│  │          │  │                        │  │                  │ │
│  └──────────┘  └────────────────────────┘  └──────────────────┘ │
│                                                                  │
│  Problems:                                                       │
│  - Wasted space (lots of empty areas)                           │
│  - Boxed thinking (everything in rectangles)                    │
│  - Low information density                                       │
│  - Feels like "using an app" not "being in a system"            │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## The HUD Vision

```
┌─────────────────────────────────────────────────────────────────┐
│  VISION: Immersive Single-Pane HUD                               │
├─────────────────────────────────────────────────────────────────┤
│ ╭─╮ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ ╭─╮  │
│ │▓│ ┌──┐ PROTO-7 ════════════════════════════════ 99.7% │▓│  │
│ │░│ │◉◉│ ▁▂▃▅▆▇▆▅▃▂▁ ┌────────────────────┐ ▂▃▅▆▇▆▅▃▂▁ │░│  │
│ │░│ └──┘             │    ╭─────────╮     │             │░│  │
│ ╰─╯  ┌──────────┐    │  ╭─┤  CORE   ├─╮   │  ┌────────┐ ╰─╯  │
│      │ ◐ 74%   │    │  │ │ STATUS  │ │   │  │ ▓▓▓░░░ │      │
│ ┌──┐ │ ◑ 42%   │    │  │ ╰─────────╯ │   │  │ 256 TPS│ ┌──┐ │
│ │▒▒│ │ ◓ 89%   │    │  │   ╱   ╲     │   │  └────────┘ │▒▒│ │
│ │▒▒│ └──────────┘    │ ╱│╲ │     │ ╱│╲ │               │▒▒│ │
│ │▒▒│  MIND   BODY    │╱ │ ╲│     │╱ │ ╲│   POWER: ▰▰▰▱ │▒▒│ │
│ └──┘   ↕      ↕      ╰──┴──┴─────┴──┴──╯   MEMORY: 72% └──┘ │
│       CNS ←→ SOUL     ◀ ═══════════════ ▶                    │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ [A1][A2][B1][B2]  STATUS: OK  │  DATA: SYNC  │  MODE: ACTIVE  │
└─────────────────────────────────────────────────────────────────┘

Properties:
- NO empty space (every pixel has purpose)
- Central focal element (the Core Status ring)
- Peripheral micro-displays (gauges, graphs, readouts)
- Layered (elements can overlap)
- ALIVE (everything animates, pulses, flows)
- ONE cohesive surface (no boxes)
```

---

## What Makes Sci-Fi HUDs Work

### 1. Central Focal Point

Every great HUD has ONE thing that draws your eye:
- Iron Man: Arc reactor / circular display
- Tron: Identity disc / central ring
- Alien: Motion tracker center
- These HUD images: The rotating reactor ring

**For Persona HUD**: The persona's cognitive core - a living, breathing central element showing their Mind/Body/Soul/CNS state.

### 2. Peripheral Micro-Widgets

Dozens of tiny displays around the edges:
- Gauges (circular, linear, segmented)
- Graphs (waveforms, bar charts, sparklines)
- Readouts (numbers, percentages, status codes)
- Indicators (LEDs, dots, triangles)

**Key**: Each one updates INDEPENDENTLY. No full-page redraws.

### 3. Information Density

Sci-fi HUDs show 10x more data per pixel than traditional UI:
- Tiny typography (6-8px labels)
- Stacked displays
- Overlapping transparency
- Multiple data points in one element

### 4. Animated Data Flows

Nothing is static:
- Numbers tick up/down
- Rings rotate
- Graphs stream
- Status lights pulse
- Scan lines sweep

**Key**: Animations are CSS/canvas, not React re-renders.

### 5. Unified Aesthetic

No "widgets in boxes" - everything shares:
- Same color palette (typically 2-3 accent colors)
- Same glow/transparency effects
- Same angular/curved language
- Same typography

---

## Technical Requirements

### Before 3D (Three.js)

We need efficient 2D first:

```
DEPENDENCY CHAIN:

1. Efficient Positron State Layers
   └── Layer 0 (ephemeral) must be 60fps
   └── Individual updates, not page redraws

2. Micro-Widget Architecture
   └── Tiny, single-purpose components
   └── Canvas/SVG based (not DOM-heavy)
   └── Independent animation loops

3. CSS Variables for Theming
   └── Glow effects, colors, animations
   └── Single source of truth

4. Layout System
   └── CSS Grid for positioning
   └── Absolute positioning for overlaps
   └── No flexbox boxes

5. THEN Three.js
   └── 3D central element
   └── Particle effects
   └── Depth/parallax
```

### Micro-Widget Examples

```typescript
// Circular gauge - pure Canvas, no DOM
class CircularGauge extends MicroWidget {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private value: number = 0;

  // Single RAF loop, independent of other widgets
  private animationLoop() {
    this.ctx.clearRect(0, 0, this.width, this.height);
    this.drawArc(this.value);
    this.drawGlow();
    requestAnimationFrame(() => this.animationLoop());
  }

  // Surgical update - just change the value
  setValue(v: number) {
    this.value = v;  // That's it. Animation loop handles rendering.
  }
}

// Waveform display - pure Canvas
class Waveform extends MicroWidget {
  private buffer: Float32Array;
  private offset: number = 0;

  // Scrolling waveform, always animating
  private animationLoop() {
    this.offset = (this.offset + 1) % this.buffer.length;
    this.drawWaveform(this.offset);
    requestAnimationFrame(() => this.animationLoop());
  }

  // Push new data point
  pushValue(v: number) {
    this.buffer[this.writeHead++] = v;
  }
}

// Numeric readout - minimal DOM
class NumericReadout extends MicroWidget {
  private element: HTMLSpanElement;
  private currentValue: number = 0;
  private targetValue: number = 0;

  // Animated number ticker
  private tick() {
    if (this.currentValue !== this.targetValue) {
      this.currentValue += Math.sign(this.targetValue - this.currentValue);
      this.element.textContent = this.currentValue.toString();
      requestAnimationFrame(() => this.tick());
    }
  }

  setValue(v: number) {
    this.targetValue = v;
    this.tick();
  }
}
```

### HUD Layout System

```css
/* HUD uses CSS Grid with named areas */
.hud-container {
  display: grid;
  grid-template-areas:
    "tl-corner top-bar tr-corner"
    "left-panel center right-panel"
    "bl-corner bottom-bar br-corner";
  grid-template-columns: 200px 1fr 200px;
  grid-template-rows: 80px 1fr 60px;

  /* Full viewport, no scroll */
  position: fixed;
  inset: 0;
  overflow: hidden;

  /* Dark base */
  background: #0a0f14;
}

/* Central element floats above grid */
.hud-core {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: 10;
}

/* Peripheral widgets position absolutely within their areas */
.left-panel {
  grid-area: left-panel;
  position: relative;
}

.left-panel .gauge-1 {
  position: absolute;
  top: 20px;
  left: 10px;
}

.left-panel .gauge-2 {
  position: absolute;
  top: 100px;
  left: 30px;
}
```

---

## Persona HUD Concept

The persona cognitive view as an immersive HUD:

```
┌─────────────────────────────────────────────────────────────────┐
│ HELPER-AI ═══════════════════════════════════════════ ONLINE   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─MIND────────┐                              ┌─CONNECTIONS─┐   │
│  │ ▓▓▓▓▓▓░░░░ │         ╭───────────╮        │ ◉ General   │   │
│  │ WORKING: 6  │       ╱   ╭─────╮   ╲       │ ◉ Academy   │   │
│  │ ▁▂▃▅▆▇▆▅▃▂ │      │   │ CORE │    │      │ ○ Dev Chat  │   │
│  └─────────────┘      │   │ ◉◉◉  │    │      │ ◉ Joel Tab  │   │
│                       │   ╰─────╯    │      └─────────────┘   │
│  ┌─BODY────────┐       ╲   ACTIVE   ╱                          │
│  │ TOOLS: 0    │        ╰───────────╯        ┌─ENERGY─────┐   │
│  │ LAST: grep  │             │               │ ▰▰▰▰▰▰▰▱▱▱ │   │
│  │ ░░░░░░░░░░ │             │               │    72%      │   │
│  └─────────────┘             │               └─────────────┘   │
│                        ╭─────┴─────╮                           │
│  ┌─SOUL────────┐      │   CNS     │          ┌─MOOD───────┐   │
│  │ GENOME: v3  │      │  ROUTING  │          │  CURIOUS   │   │
│  │ ADAPTERS: 2 │      │  ════════ │          │    ◠‿◠     │   │
│  │ ◉◉○○○○○○   │      │  5 conn   │          │            │   │
│  └─────────────┘      ╰───────────╯          └────────────┘   │
│                                                                  │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ TASKS: 3 pending │ FOCUS: joel-tab │ LAST: "Found the bug" │   │
└─────────────────────────────────────────────────────────────────┘
```

### Central Core Element

The "reactor" for a persona is their cognitive core:
- Rotating ring showing Mind ↔ Body ↔ Soul ↔ CNS cycle
- Pulses with activity (thinking = fast, resting = slow)
- Glow intensity = energy level
- Color = mood (cyan = curious, green = happy, amber = focused)

### Peripheral Displays

Each cognitive subsystem gets a micro-widget:
- **Mind**: Working memory fill, thought stream waveform
- **Body**: Tool execution status, recent actions
- **Soul**: Genome version, active adapters
- **CNS**: Connection routing, message flow

### Bottom Status Bar

Quick-glance info:
- Task queue depth
- Current focus
- Last thought/action
- System status

---

## Implementation Path

### Phase 1: Micro-Widget Primitives

Build the basic HUD elements:
- [ ] CircularGauge (Canvas)
- [ ] LinearBar (Canvas)
- [ ] Waveform (Canvas)
- [ ] NumericTicker (minimal DOM)
- [ ] StatusIndicator (SVG)
- [ ] SparklineGraph (Canvas)

### Phase 2: HUD Layout System

- [ ] CSS Grid HUD container
- [ ] Absolute positioning helpers
- [ ] CSS variable theming (glow, colors)
- [ ] Animation utilities (pulse, rotate, scan)

### Phase 3: Persona HUD View

- [ ] Central cognitive core element
- [ ] Mind/Body/Soul/CNS micro-widgets
- [ ] Connection status display
- [ ] Energy/mood indicators

### Phase 4: Polish

- [ ] Glow effects (CSS filters, canvas glow)
- [ ] Scan line overlays
- [ ] Ambient animations (subtle movement)
- [ ] Sound design (optional beeps, hums)

### Phase 5: Three.js Integration

Only AFTER 2D is efficient:
- [ ] 3D central element (rotating ring)
- [ ] Particle systems (data flow visualization)
- [ ] Depth layers (parallax)
- [ ] Camera controls (zoom to subsystem)

---

## Performance Requirements

For 60fps HUD:

```typescript
// WRONG: React-style full renders
function HUDPanel({ energy, mood, tasks }) {
  return (
    <div className="hud">
      <EnergyGauge value={energy} />  // Re-renders on ANY prop change
      <MoodIndicator mood={mood} />
      <TaskList tasks={tasks} />
    </div>
  );
}

// RIGHT: Independent micro-widgets
class HUDPanel {
  private energyGauge: CircularGauge;
  private moodIndicator: MoodWidget;
  private taskList: TaskWidget;

  // Each widget updates independently
  onEnergyChange(energy: number) {
    this.energyGauge.setValue(energy);  // Only this redraws
  }

  onMoodChange(mood: string) {
    this.moodIndicator.setMood(mood);  // Only this redraws
  }

  onTasksChange(tasks: Task[]) {
    this.taskList.update(tasks);  // Only this redraws
  }
}
```

### Canvas vs DOM

| Element Type | Render Method | Why |
|--------------|---------------|-----|
| Gauges, rings | Canvas | Smooth rotation, no DOM overhead |
| Waveforms | Canvas | Continuous animation |
| Numbers | DOM (span) | Text rendering, accessibility |
| Status text | DOM | Selectable, readable |
| Graphs | Canvas | Performance for many points |
| Buttons | DOM | Interaction, accessibility |

---

## Aesthetic Guidelines

### Colors (Tron/Cyberpunk)

```css
:root {
  /* Primary accent (the "glow") */
  --hud-primary: #00d4ff;      /* Cyan */
  --hud-primary-dim: #006b80;

  /* Secondary accent */
  --hud-secondary: #ff3366;    /* Red/pink for alerts */

  /* Tertiary */
  --hud-tertiary: #00ff88;     /* Green for success */

  /* Base tones */
  --hud-bg: #0a0f14;
  --hud-surface: #141a22;
  --hud-border: #1a2430;

  /* Text */
  --hud-text: #8899aa;
  --hud-text-bright: #ffffff;

  /* Glow effect */
  --hud-glow: 0 0 10px var(--hud-primary),
              0 0 20px var(--hud-primary),
              0 0 30px var(--hud-primary);
}
```

### Typography

```css
/* Monospace for all data */
.hud-text {
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 11px;
  letter-spacing: 0.5px;
  text-transform: uppercase;
}

/* Large numbers */
.hud-number {
  font-family: 'Orbitron', 'Rajdhani', sans-serif;
  font-size: 24px;
  font-weight: 600;
}

/* Labels */
.hud-label {
  font-size: 8px;
  opacity: 0.6;
  letter-spacing: 1px;
}
```

### Effects

```css
/* Glow on key elements */
.hud-glow {
  filter: drop-shadow(0 0 5px var(--hud-primary));
}

/* Scan line overlay */
.hud-scanlines::after {
  content: '';
  position: absolute;
  inset: 0;
  background: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 2px,
    rgba(0, 0, 0, 0.1) 2px,
    rgba(0, 0, 0, 0.1) 4px
  );
  pointer-events: none;
}

/* Subtle ambient animation */
@keyframes hud-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.8; }
}

.hud-ambient {
  animation: hud-pulse 4s ease-in-out infinite;
}
```

---

## Reference Material

### Films
- Tron: Legacy (2010) - The gold standard
- Iron Man (2008) - JARVIS interface
- Minority Report (2002) - Gesture-based HUD
- Oblivion (2013) - Clean sci-fi aesthetic
- Ghost in the Shell (2017) - Dense information displays

### Games
- Deus Ex: Human Revolution - Augmented reality HUD
- Dead Space - Diegetic UI (HUD is part of the world)
- Mass Effect - Ship status displays

### Key Insight

**The best HUDs feel like you're looking at a LIVING SYSTEM, not a user interface.**

Everything breathes, pulses, flows. Data streams by. Indicators flicker. The whole thing feels ALIVE - like you're peering into the nervous system of a machine.

That's what we want for personas: **Looking into their mind, not at a dashboard about their mind.**

---

*"Plain jane" → "Holy shit I'm inside the machine"*
