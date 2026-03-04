# HUD Micro-Widget Architecture

> "Every gauge, ring, and readout is a proper component - same base class, same events, same Positron hooks"

---

## The Principle

Even the tiniest HUD element follows the same architecture as full widgets:

```
widgets/
├── hud/
│   ├── shared/
│   │   ├── BaseHUDWidget.ts       # Base class for all HUD components
│   │   └── HUDTypes.ts            # Shared types
│   │
│   ├── circular-gauge/
│   │   ├── CircularGauge.ts       # The component
│   │   ├── CircularGauge.css      # Styles
│   │   └── README.md              # Usage docs
│   │
│   ├── linear-bar/
│   │   ├── LinearBar.ts
│   │   └── LinearBar.css
│   │
│   ├── waveform/
│   │   ├── Waveform.ts
│   │   └── Waveform.css
│   │
│   ├── numeric-ticker/
│   │   ├── NumericTicker.ts
│   │   └── NumericTicker.css
│   │
│   ├── status-indicator/
│   │   ├── StatusIndicator.ts
│   │   └── StatusIndicator.css
│   │
│   ├── central-ring/
│   │   ├── CentralRing.ts         # The big focal element
│   │   └── CentralRing.css
│   │
│   └── sparkline/
│       ├── Sparkline.ts
│       └── Sparkline.css
```

---

## BaseHUDWidget

All HUD micro-widgets extend this:

```typescript
// widgets/hud/shared/BaseHUDWidget.ts

import { Events } from '../../../system/core/shared/Events';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export interface HUDWidgetConfig {
  /** Unique identifier for this widget instance */
  id?: string;

  /** Width in pixels */
  width?: number;

  /** Height in pixels */
  height?: number;

  /** CSS color for primary accent */
  accentColor?: string;

  /** Enable glow effects */
  glow?: boolean;

  /** Animation speed multiplier (1.0 = normal) */
  animationSpeed?: number;
}

export interface HUDState {
  value: number | string | boolean;
  [key: string]: unknown;
}

/**
 * Base class for all HUD micro-widgets.
 *
 * Features:
 * - Canvas or SVG rendering (override renderMethod)
 * - Independent animation loop (RAF-based)
 * - Positron state layer integration
 * - Events.subscribe/emit for data binding
 * - Surgical updates (only this widget redraws)
 */
export abstract class BaseHUDWidget extends HTMLElement {
  protected canvas: HTMLCanvasElement | null = null;
  protected ctx: CanvasRenderingContext2D | null = null;
  protected svg: SVGElement | null = null;

  protected config: HUDWidgetConfig;
  protected state: HUDState = { value: 0 };
  protected animationFrameId: number | null = null;
  protected subscriptions: Array<() => void> = [];

  // Subclasses define their render method
  protected abstract renderMethod: 'canvas' | 'svg' | 'dom';

  constructor(config: HUDWidgetConfig = {}) {
    super();
    this.config = {
      width: 100,
      height: 100,
      accentColor: '#00d4ff',
      glow: true,
      animationSpeed: 1.0,
      ...config
    };
    this.attachShadow({ mode: 'open' });
  }

  connectedCallback(): void {
    this.setupRenderer();
    this.setupStyles();
    this.setupSubscriptions();
    this.startAnimationLoop();
    this.onConnected();
  }

  disconnectedCallback(): void {
    this.stopAnimationLoop();
    this.cleanupSubscriptions();
    this.onDisconnected();
  }

  // ─────────────────────────────────────────────────────────────
  // LIFECYCLE HOOKS (override in subclasses)
  // ─────────────────────────────────────────────────────────────

  /** Called after widget is connected to DOM */
  protected onConnected(): void {}

  /** Called before widget is removed from DOM */
  protected onDisconnected(): void {}

  /** Called each animation frame */
  protected abstract onRender(timestamp: number): void;

  // ─────────────────────────────────────────────────────────────
  // RENDERING SETUP
  // ─────────────────────────────────────────────────────────────

  private setupRenderer(): void {
    if (this.renderMethod === 'canvas') {
      this.canvas = document.createElement('canvas');
      this.canvas.width = this.config.width!;
      this.canvas.height = this.config.height!;
      this.ctx = this.canvas.getContext('2d')!;
      this.shadowRoot!.appendChild(this.canvas);
    } else if (this.renderMethod === 'svg') {
      this.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      this.svg.setAttribute('width', String(this.config.width));
      this.svg.setAttribute('height', String(this.config.height));
      this.shadowRoot!.appendChild(this.svg);
    }
  }

  private setupStyles(): void {
    const style = document.createElement('style');
    style.textContent = `
      :host {
        display: inline-block;
        width: ${this.config.width}px;
        height: ${this.config.height}px;
      }

      canvas, svg {
        display: block;
        ${this.config.glow ? `filter: drop-shadow(0 0 5px ${this.config.accentColor});` : ''}
      }
    `;
    this.shadowRoot!.appendChild(style);
  }

  // ─────────────────────────────────────────────────────────────
  // ANIMATION LOOP
  // ─────────────────────────────────────────────────────────────

  private startAnimationLoop(): void {
    const loop = (timestamp: number) => {
      this.onRender(timestamp * this.config.animationSpeed!);
      this.animationFrameId = requestAnimationFrame(loop);
    };
    this.animationFrameId = requestAnimationFrame(loop);
  }

  private stopAnimationLoop(): void {
    if (this.animationFrameId !== null) {
      cancelAnimationFrame(this.animationFrameId);
      this.animationFrameId = null;
    }
  }

  // ─────────────────────────────────────────────────────────────
  // POSITRON INTEGRATION
  // ─────────────────────────────────────────────────────────────

  /**
   * Subscribe to an event. Automatically cleaned up on disconnect.
   */
  protected subscribe<T>(eventName: string, handler: (data: T) => void): void {
    const unsubscribe = Events.subscribe(eventName, handler);
    this.subscriptions.push(unsubscribe);
  }

  /**
   * Emit an event (state changes, user interactions, etc.)
   */
  protected emit<T>(eventName: string, data: T): void {
    Events.emit(eventName, data);
  }

  private setupSubscriptions(): void {
    // Override in subclass to set up event subscriptions
  }

  private cleanupSubscriptions(): void {
    this.subscriptions.forEach(unsub => unsub());
    this.subscriptions = [];
  }

  // ─────────────────────────────────────────────────────────────
  // STATE MANAGEMENT
  // ─────────────────────────────────────────────────────────────

  /**
   * Update widget state. Triggers semantic event if configured.
   */
  protected setState(newState: Partial<HUDState>, options?: { semantic?: boolean }): void {
    const oldState = { ...this.state };
    this.state = { ...this.state, ...newState };

    // Emit state change event
    this.emit(`hud:${this.tagName.toLowerCase()}:state-changed`, {
      widgetId: this.config.id,
      oldState,
      newState: this.state,
      semantic: options?.semantic ?? false
    });
  }

  /**
   * Get current state value
   */
  public getValue(): number | string | boolean {
    return this.state.value;
  }

  /**
   * Set primary value (convenience method)
   */
  public setValue(value: number | string | boolean): void {
    this.setState({ value });
  }

  // ─────────────────────────────────────────────────────────────
  // UTILITY METHODS
  // ─────────────────────────────────────────────────────────────

  /**
   * Clear canvas (call at start of each render)
   */
  protected clearCanvas(): void {
    if (this.ctx && this.canvas) {
      this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    }
  }

  /**
   * Get center point of canvas
   */
  protected getCenter(): { x: number; y: number } {
    return {
      x: this.config.width! / 2,
      y: this.config.height! / 2
    };
  }

  /**
   * Parse CSS color to RGBA
   */
  protected colorToRGBA(color: string, alpha: number = 1): string {
    // Simple implementation - could be expanded
    if (color.startsWith('#')) {
      const r = parseInt(color.slice(1, 3), 16);
      const g = parseInt(color.slice(3, 5), 16);
      const b = parseInt(color.slice(5, 7), 16);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }
    return color;
  }
}
```

---

## Example: CircularGauge

```typescript
// widgets/hud/circular-gauge/CircularGauge.ts

import { BaseHUDWidget, HUDWidgetConfig } from '../shared/BaseHUDWidget';

export interface CircularGaugeConfig extends HUDWidgetConfig {
  /** Minimum value */
  min?: number;

  /** Maximum value */
  max?: number;

  /** Arc start angle (radians) */
  startAngle?: number;

  /** Arc end angle (radians) */
  endAngle?: number;

  /** Ring thickness */
  thickness?: number;

  /** Show value text in center */
  showValue?: boolean;

  /** Value format (e.g., '%', ' TPS') */
  valueSuffix?: string;
}

export class CircularGauge extends BaseHUDWidget {
  protected renderMethod = 'canvas' as const;
  private gaugeConfig: CircularGaugeConfig;

  // Animation state
  private displayValue: number = 0;
  private targetValue: number = 0;

  constructor(config: CircularGaugeConfig = {}) {
    super(config);
    this.gaugeConfig = {
      min: 0,
      max: 100,
      startAngle: Math.PI * 0.75,
      endAngle: Math.PI * 2.25,
      thickness: 8,
      showValue: true,
      valueSuffix: '%',
      ...config
    };
  }

  protected onConnected(): void {
    // Subscribe to data updates for this gauge
    if (this.config.id) {
      this.subscribe(`data:gauge:${this.config.id}`, (value: number) => {
        this.setTargetValue(value);
      });
    }
  }

  /**
   * Set target value (will animate to it)
   */
  public setTargetValue(value: number): void {
    this.targetValue = Math.max(
      this.gaugeConfig.min!,
      Math.min(this.gaugeConfig.max!, value)
    );
  }

  /**
   * Set value immediately (no animation)
   */
  public override setValue(value: number | string | boolean): void {
    const numValue = typeof value === 'number' ? value : parseFloat(String(value));
    this.targetValue = numValue;
    this.displayValue = numValue;
    this.setState({ value: numValue });
  }

  protected onRender(timestamp: number): void {
    this.clearCanvas();

    // Animate toward target
    const delta = this.targetValue - this.displayValue;
    if (Math.abs(delta) > 0.1) {
      this.displayValue += delta * 0.1;  // Smooth interpolation
    } else {
      this.displayValue = this.targetValue;
    }

    this.drawBackground();
    this.drawArc();
    if (this.gaugeConfig.showValue) {
      this.drawValue();
    }
  }

  private drawBackground(): void {
    const ctx = this.ctx!;
    const { x, y } = this.getCenter();
    const radius = Math.min(x, y) - this.gaugeConfig.thickness! / 2 - 4;

    ctx.beginPath();
    ctx.arc(x, y, radius, this.gaugeConfig.startAngle!, this.gaugeConfig.endAngle!);
    ctx.strokeStyle = this.colorToRGBA(this.config.accentColor!, 0.2);
    ctx.lineWidth = this.gaugeConfig.thickness!;
    ctx.lineCap = 'round';
    ctx.stroke();
  }

  private drawArc(): void {
    const ctx = this.ctx!;
    const { x, y } = this.getCenter();
    const radius = Math.min(x, y) - this.gaugeConfig.thickness! / 2 - 4;

    // Calculate progress
    const range = this.gaugeConfig.max! - this.gaugeConfig.min!;
    const progress = (this.displayValue - this.gaugeConfig.min!) / range;
    const angleRange = this.gaugeConfig.endAngle! - this.gaugeConfig.startAngle!;
    const currentAngle = this.gaugeConfig.startAngle! + (progress * angleRange);

    ctx.beginPath();
    ctx.arc(x, y, radius, this.gaugeConfig.startAngle!, currentAngle);
    ctx.strokeStyle = this.config.accentColor!;
    ctx.lineWidth = this.gaugeConfig.thickness!;
    ctx.lineCap = 'round';
    ctx.stroke();
  }

  private drawValue(): void {
    const ctx = this.ctx!;
    const { x, y } = this.getCenter();

    ctx.font = 'bold 16px "JetBrains Mono", monospace';
    ctx.fillStyle = this.config.accentColor!;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(
      `${Math.round(this.displayValue)}${this.gaugeConfig.valueSuffix}`,
      x,
      y
    );
  }
}

// Register custom element
customElements.define('hud-circular-gauge', CircularGauge);
```

---

## Example: Waveform

```typescript
// widgets/hud/waveform/Waveform.ts

import { BaseHUDWidget, HUDWidgetConfig } from '../shared/BaseHUDWidget';

export interface WaveformConfig extends HUDWidgetConfig {
  /** Number of samples to display */
  samples?: number;

  /** Line thickness */
  lineWidth?: number;

  /** Fill area under line */
  fill?: boolean;

  /** Auto-scroll speed (0 = manual, > 0 = auto) */
  scrollSpeed?: number;
}

export class Waveform extends BaseHUDWidget {
  protected renderMethod = 'canvas' as const;
  private waveformConfig: WaveformConfig;

  private buffer: Float32Array;
  private writeHead: number = 0;
  private scrollOffset: number = 0;

  constructor(config: WaveformConfig = {}) {
    super(config);
    this.waveformConfig = {
      samples: 100,
      lineWidth: 2,
      fill: true,
      scrollSpeed: 1,
      ...config
    };
    this.buffer = new Float32Array(this.waveformConfig.samples!);
  }

  protected onConnected(): void {
    // Subscribe to data stream
    if (this.config.id) {
      this.subscribe(`data:waveform:${this.config.id}`, (value: number) => {
        this.pushValue(value);
      });
    }
  }

  /**
   * Push a new value to the waveform buffer
   */
  public pushValue(value: number): void {
    this.buffer[this.writeHead] = value;
    this.writeHead = (this.writeHead + 1) % this.buffer.length;
  }

  /**
   * Set entire buffer at once
   */
  public setBuffer(data: number[]): void {
    for (let i = 0; i < Math.min(data.length, this.buffer.length); i++) {
      this.buffer[i] = data[i];
    }
  }

  protected onRender(timestamp: number): void {
    this.clearCanvas();

    // Auto-scroll
    if (this.waveformConfig.scrollSpeed! > 0) {
      this.scrollOffset = (timestamp * this.waveformConfig.scrollSpeed! * 0.01) % 1;
    }

    this.drawWaveform();
  }

  private drawWaveform(): void {
    const ctx = this.ctx!;
    const width = this.config.width!;
    const height = this.config.height!;
    const samples = this.buffer.length;
    const stepX = width / (samples - 1);

    ctx.beginPath();

    for (let i = 0; i < samples; i++) {
      // Read from buffer with offset for scrolling effect
      const bufferIndex = (i + this.writeHead) % samples;
      const value = this.buffer[bufferIndex];

      // Map value (0-1) to y position
      const x = i * stepX;
      const y = height - (value * height * 0.8) - (height * 0.1);

      if (i === 0) {
        ctx.moveTo(x, y);
      } else {
        ctx.lineTo(x, y);
      }
    }

    // Draw line
    ctx.strokeStyle = this.config.accentColor!;
    ctx.lineWidth = this.waveformConfig.lineWidth!;
    ctx.stroke();

    // Fill area under curve
    if (this.waveformConfig.fill) {
      ctx.lineTo(width, height);
      ctx.lineTo(0, height);
      ctx.closePath();
      ctx.fillStyle = this.colorToRGBA(this.config.accentColor!, 0.2);
      ctx.fill();
    }
  }
}

// Register custom element
customElements.define('hud-waveform', Waveform);
```

---

## Example: Numeric Ticker

```typescript
// widgets/hud/numeric-ticker/NumericTicker.ts

import { BaseHUDWidget, HUDWidgetConfig } from '../shared/BaseHUDWidget';

export interface NumericTickerConfig extends HUDWidgetConfig {
  /** Number of decimal places */
  decimals?: number;

  /** Prefix (e.g., '$', '€') */
  prefix?: string;

  /** Suffix (e.g., '%', ' TPS') */
  suffix?: string;

  /** Animation speed */
  tickSpeed?: number;

  /** Font size */
  fontSize?: number;
}

export class NumericTicker extends BaseHUDWidget {
  protected renderMethod = 'dom' as const;  // DOM for text
  private tickerConfig: NumericTickerConfig;

  private displayElement: HTMLSpanElement | null = null;
  private displayValue: number = 0;
  private targetValue: number = 0;

  constructor(config: NumericTickerConfig = {}) {
    super(config);
    this.tickerConfig = {
      decimals: 0,
      prefix: '',
      suffix: '',
      tickSpeed: 0.1,
      fontSize: 24,
      ...config
    };
  }

  protected onConnected(): void {
    // Create DOM element
    this.displayElement = document.createElement('span');
    this.displayElement.className = 'ticker-value';
    this.shadowRoot!.appendChild(this.displayElement);

    // Add styles
    const style = document.createElement('style');
    style.textContent = `
      :host {
        display: inline-block;
        font-family: 'Orbitron', 'JetBrains Mono', monospace;
      }

      .ticker-value {
        font-size: ${this.tickerConfig.fontSize}px;
        font-weight: 600;
        color: ${this.config.accentColor};
        ${this.config.glow ? `text-shadow: 0 0 10px ${this.config.accentColor};` : ''}
      }
    `;
    this.shadowRoot!.appendChild(style);

    // Subscribe to data
    if (this.config.id) {
      this.subscribe(`data:ticker:${this.config.id}`, (value: number) => {
        this.setTargetValue(value);
      });
    }

    this.updateDisplay();
  }

  public setTargetValue(value: number): void {
    this.targetValue = value;
  }

  public override setValue(value: number | string | boolean): void {
    const numValue = typeof value === 'number' ? value : parseFloat(String(value));
    this.targetValue = numValue;
    this.displayValue = numValue;
    this.updateDisplay();
  }

  protected onRender(timestamp: number): void {
    // Animate toward target
    const delta = this.targetValue - this.displayValue;
    if (Math.abs(delta) > 0.01) {
      this.displayValue += delta * this.tickerConfig.tickSpeed!;
      this.updateDisplay();
    }
  }

  private updateDisplay(): void {
    if (this.displayElement) {
      const formatted = this.displayValue.toFixed(this.tickerConfig.decimals);
      this.displayElement.textContent =
        `${this.tickerConfig.prefix}${formatted}${this.tickerConfig.suffix}`;
    }
  }
}

// Register custom element
customElements.define('hud-numeric-ticker', NumericTicker);
```

---

## Usage in a HUD Layout

```typescript
// widgets/hud/persona-hud/PersonaHUD.ts

import { BaseWidget } from '../../shared/BaseWidget';
import { Events } from '../../../system/core/shared/Events';

// Import HUD components
import '../circular-gauge/CircularGauge';
import '../waveform/Waveform';
import '../numeric-ticker/NumericTicker';
import '../central-ring/CentralRing';

export class PersonaHUD extends BaseWidget {
  // References to micro-widgets
  private energyGauge!: HTMLElement;
  private moodGauge!: HTMLElement;
  private thoughtWaveform!: HTMLElement;
  private taskTicker!: HTMLElement;
  private coreRing!: HTMLElement;

  constructor() {
    super({
      widgetName: 'PersonaHUD',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: true
    });
  }

  protected async renderWidget(): Promise<void> {
    this.shadowRoot!.innerHTML = `
      <style>
        :host {
          display: block;
          width: 100%;
          height: 100%;
          background: #0a0f14;
          position: relative;
        }

        .hud-container {
          display: grid;
          grid-template-areas:
            "tl top tr"
            "left center right"
            "bl bottom br";
          grid-template-columns: 200px 1fr 200px;
          grid-template-rows: 100px 1fr 80px;
          height: 100%;
          padding: 20px;
          gap: 10px;
        }

        .center { grid-area: center; display: flex; justify-content: center; align-items: center; }
        .left { grid-area: left; display: flex; flex-direction: column; gap: 20px; }
        .right { grid-area: right; display: flex; flex-direction: column; gap: 20px; }
        .top { grid-area: top; display: flex; justify-content: center; gap: 20px; }
        .bottom { grid-area: bottom; display: flex; justify-content: center; gap: 20px; }

        .micro-widget-group {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .label {
          font-size: 8px;
          text-transform: uppercase;
          letter-spacing: 1px;
          color: rgba(0, 212, 255, 0.5);
          font-family: 'JetBrains Mono', monospace;
        }
      </style>

      <div class="hud-container">
        <!-- Left panel: Mind + Body gauges -->
        <div class="left">
          <div class="micro-widget-group">
            <span class="label">Energy</span>
            <hud-circular-gauge
              id="energy-gauge"
              width="120"
              height="120"
              accent-color="#00d4ff"
            ></hud-circular-gauge>
          </div>

          <div class="micro-widget-group">
            <span class="label">Working Memory</span>
            <hud-waveform
              id="memory-waveform"
              width="180"
              height="60"
              accent-color="#00ff88"
            ></hud-waveform>
          </div>
        </div>

        <!-- Center: Core cognitive ring -->
        <div class="center">
          <hud-central-ring
            id="core-ring"
            width="300"
            height="300"
          ></hud-central-ring>
        </div>

        <!-- Right panel: Stats + connections -->
        <div class="right">
          <div class="micro-widget-group">
            <span class="label">Tasks</span>
            <hud-numeric-ticker
              id="task-ticker"
              prefix=""
              suffix=" pending"
              decimals="0"
            ></hud-numeric-ticker>
          </div>

          <div class="micro-widget-group">
            <span class="label">Connections</span>
            <hud-circular-gauge
              id="conn-gauge"
              width="80"
              height="80"
              max="10"
            ></hud-circular-gauge>
          </div>
        </div>

        <!-- Bottom status bar -->
        <div class="bottom">
          <hud-numeric-ticker id="tps" suffix=" TPS" decimals="1"></hud-numeric-ticker>
          <hud-numeric-ticker id="latency" suffix="ms" decimals="0"></hud-numeric-ticker>
        </div>
      </div>
    `;

    this.setupDataBindings();
  }

  private setupDataBindings(): void {
    // Bind persona state to HUD widgets
    Events.subscribe('persona:state:energy', (data: { energy: number }) => {
      const gauge = this.shadowRoot!.querySelector('#energy-gauge') as any;
      gauge?.setTargetValue(data.energy * 100);
    });

    Events.subscribe('persona:mind:thought-activity', (data: { level: number }) => {
      const waveform = this.shadowRoot!.querySelector('#memory-waveform') as any;
      waveform?.pushValue(data.level);
    });

    Events.subscribe('persona:tasks:count', (data: { count: number }) => {
      const ticker = this.shadowRoot!.querySelector('#task-ticker') as any;
      ticker?.setTargetValue(data.count);
    });
  }
}

customElements.define('persona-hud', PersonaHUD);
```

---

## Key Principles

### 1. Same Architecture as Full Widgets

Every HUD element:
- Has its own directory
- Extends a base class
- Uses Events.subscribe/emit
- Has typed configuration
- Is a registered custom element

### 2. Independent Animation Loops

Each micro-widget has its own `requestAnimationFrame` loop:
- No central render coordinator
- Each updates at its own pace
- Pausing one doesn't affect others

### 3. Surgical Updates

When data changes:
- Only the affected widget redraws
- No DOM thrashing
- No full-page re-renders

### 4. Positron Integration

All widgets can:
- Subscribe to Positron events
- Emit state changes (for AI perception)
- Participate in semantic layer (if configured)

### 5. Composability

Micro-widgets compose into larger HUD layouts:
- PersonaHUD uses CircularGauge, Waveform, etc.
- Each is still independent
- Layout is CSS Grid, not widget responsibility

---

## File Structure Summary

```
widgets/hud/
├── shared/
│   ├── BaseHUDWidget.ts        # Base class
│   ├── HUDTypes.ts             # Shared types
│   └── HUDTheme.css            # CSS variables
│
├── circular-gauge/
│   ├── CircularGauge.ts
│   ├── CircularGauge.css
│   └── README.md
│
├── linear-bar/
│   └── LinearBar.ts
│
├── waveform/
│   └── Waveform.ts
│
├── numeric-ticker/
│   └── NumericTicker.ts
│
├── status-indicator/
│   └── StatusIndicator.ts
│
├── sparkline/
│   └── Sparkline.ts
│
├── central-ring/
│   └── CentralRing.ts
│
└── persona-hud/
    └── PersonaHUD.ts           # Composite HUD
```

---

*Every pixel is a component. Every component is a citizen.*
