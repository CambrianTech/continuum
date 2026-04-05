/**
 * StageElement — Abstract base for alloy pipeline stage UI components
 *
 * Each ForgeAlloy stage type (prune, train, lora, quant, eval, publish, etc.)
 * extends this class. The spec defines the interface, the UI implements it.
 *
 * Responsibilities:
 * - Render controls for the stage's parameters
 * - Validate parameter values against spec constraints
 * - Emit stage config as a typed JSON object matching the alloy schema
 * - Display read-only results when viewing an executed alloy
 *
 * The pipeline composer renders these as composable blocks.
 * Add a new stage type to the alloy → create a matching StageElement → done.
 */

import {
  ReactiveWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../../shared/ReactiveWidget';

/** Base styles shared by all stage elements */
export const STAGE_BASE_STYLES = css`
  :host {
    display: block;
    background: var(--surface-elevated, rgba(255,255,255,0.04));
    border: 1px solid var(--border-color, rgba(255,255,255,0.08));
    border-radius: 8px;
    padding: 12px 16px;
    transition: border-color 0.2s;
  }

  :host(:hover) {
    border-color: rgba(255,255,255,0.15);
  }

  .stage-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }

  .stage-type {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 2px 8px;
    border-radius: 3px;
  }

  .stage-order {
    font-size: 10px;
    color: var(--content-tertiary, #5a6070);
    font-variant-numeric: tabular-nums;
  }

  .stage-controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .stage-controls.single-col {
    grid-template-columns: 1fr;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .field-label {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--content-secondary, #8a92a5);
  }

  .field-input, .field-select {
    background: rgba(0,0,0,0.3);
    border: 1px solid var(--border-color, rgba(255,255,255,0.12));
    border-radius: 4px;
    color: var(--content-primary, #e0e6ed);
    font-size: 12px;
    padding: 6px 8px;
    font-family: inherit;
    outline: none;
    transition: border-color 0.2s;
  }

  .field-input:focus, .field-select:focus {
    border-color: var(--accent-primary, #00d4ff);
  }

  .field-select option {
    background: #0a1520;
    color: #e0e6ed;
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .slider-row input[type="range"] {
    flex: 1;
    accent-color: var(--accent-primary, #00d4ff);
    height: 3px;
  }

  .slider-value {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    min-width: 40px;
    text-align: right;
    color: var(--accent-primary, #00d4ff);
  }

  .field-hint {
    font-size: 9px;
    color: var(--content-tertiary, #5a6070);
    font-style: italic;
  }

  .validation-error {
    font-size: 10px;
    color: #ff4444;
    margin-top: 2px;
  }

  /* ── Gate (transition control between stages) ──── */

  .gate {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px dashed var(--border-color, rgba(255,255,255,0.06));
  }

  .gate-label {
    font-size: 9px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--content-tertiary, #5a6070);
  }

  .gate-toggle {
    font-size: 9px;
    padding: 2px 6px;
    border-radius: 3px;
    border: 1px solid var(--border-color, rgba(255,255,255,0.1));
    background: transparent;
    color: var(--content-tertiary, #5a6070);
    cursor: pointer;
    transition: all 0.15s;
  }

  .gate-toggle:hover {
    border-color: rgba(255,255,255,0.3);
    color: var(--content-secondary, #8a92a5);
  }

  .gate-toggle.active-auto {
    background: rgba(0, 255, 200, 0.1);
    border-color: rgba(0, 255, 200, 0.3);
    color: #00ffc8;
  }

  .gate-toggle.active-manual {
    background: rgba(255, 170, 0, 0.1);
    border-color: rgba(255, 170, 0, 0.3);
    color: #ffaa00;
  }

  .gate-toggle.active-conditional {
    background: rgba(0, 212, 255, 0.1);
    border-color: rgba(0, 212, 255, 0.3);
    color: #00d4ff;
  }
`;

/** Stage type color coding */
export const STAGE_COLORS: Record<string, string> = {
  // Input stages
  'source-config':  'rgba(100, 200, 255, 0.15)',
  'context-extend': 'rgba(200, 100, 255, 0.15)',
  modality:         'rgba(100, 255, 200, 0.15)',
  // Transform stages
  prune:            'rgba(255, 100, 100, 0.15)',
  train:            'rgba(0, 212, 255, 0.15)',
  lora:             'rgba(150, 100, 255, 0.15)',
  compact:          'rgba(255, 170, 0, 0.15)',
  'expert-prune':   'rgba(255, 150, 100, 0.15)',
  // Output stages
  quant:            'rgba(0, 255, 200, 0.15)',
  package:          'rgba(0, 200, 255, 0.15)',
  eval:             'rgba(255, 255, 100, 0.15)',
  publish:          'rgba(100, 200, 255, 0.15)',
  deliver:          'rgba(100, 200, 255, 0.15)',
  deploy:           'rgba(100, 255, 200, 0.15)',
};

export const STAGE_TEXT_COLORS: Record<string, string> = {
  // Input stages
  'source-config':  '#64c8ff',
  'context-extend': '#c864ff',
  modality:         '#64ffc8',
  // Transform stages
  prune:            '#ff6464',
  train:            '#00d4ff',
  lora:             '#9664ff',
  compact:          '#ffaa00',
  'expert-prune':   '#ff9664',
  // Output stages
  quant:            '#00ffc8',
  package:          '#00c8ff',
  eval:             '#ffff64',
  publish:          '#64c8ff',
  deliver:          '#64c8ff',
  deploy:           '#64ffc8',
};

/** Gate mode between stages — controls pipeline flow */
export type GateMode = 'auto' | 'manual' | 'conditional';

export abstract class StageElement extends ReactiveWidget {

  /** Which stage in the pipeline (0-indexed) */
  @reactive() order = 0;

  /** Whether the stage is in edit mode (true) or read-only view (false) */
  @reactive() editable = true;

  /** Gate mode — how this stage transitions to the next */
  @reactive() gate: GateMode = 'auto';

  /** The alloy stage type name */
  abstract get stageType(): string;

  /** Current parameter values as alloy stage config */
  abstract get stageConfig(): Record<string, unknown>;

  /** Validate current params — returns error messages (empty = valid) */
  validate(): string[] { return []; }

  /** Emit config change to parent */
  protected emitChange(): void {
    this.dispatchEvent(new CustomEvent('stage-change', {
      detail: { order: this.order, type: this.stageType, config: this.stageConfig },
      bubbles: true,
      composed: true,
    }));
  }

  /** Cycle gate mode: auto → manual → conditional → auto */
  protected cycleGate(): void {
    const modes: GateMode[] = ['auto', 'manual', 'conditional'];
    const idx = modes.indexOf(this.gate);
    this.gate = modes[(idx + 1) % modes.length];
    this.emitChange();
  }

  /** Render gate control — shown at the bottom of each stage */
  protected renderGate(): TemplateResult {
    const activeClass = `active-${this.gate}`;
    const label = this.gate === 'auto' ? 'Auto-continue'
      : this.gate === 'manual' ? 'Manual review'
      : 'Conditional';

    return html`
      <div class="gate">
        <span class="gate-label">Next:</span>
        <button class="gate-toggle ${activeClass}" @click=${this.cycleGate}>
          ${label}
        </button>
      </div>
    `;
  }

  /** Render the stage header with type badge and order number */
  protected renderHeader(): TemplateResult {
    const bg = STAGE_COLORS[this.stageType] ?? 'rgba(255,255,255,0.1)';
    const color = STAGE_TEXT_COLORS[this.stageType] ?? '#e0e6ed';

    return html`
      <div class="stage-header">
        <span class="stage-type" style="background:${bg};color:${color}">${this.stageType}</span>
        <span class="stage-order">Stage ${this.order + 1}</span>
      </div>
    `;
  }
}
