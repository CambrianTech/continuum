/**
 * CompactStageElement — UI for the alloy 'compact' stage
 *
 * Utilization-aware mixed-precision compaction.
 * Controls: utilization thresholds (dead/dormant/low/medium/high), target size, quantization
 * Maps 1:1 to ForgeAlloy CompactStage schema.
 *
 * Head precision tiers (from Rust HeadPrecision):
 *   Dead (<deadThreshold)       → Removed entirely
 *   Dormant (<dormantThreshold) → Ternary (1.6 bits)
 *   Low (<lowThreshold)         → Q2 (2 bits)
 *   Medium (<mediumThreshold)   → Q4 (4 bits)
 *   High (<highThreshold)       → Q8 (8 bits)
 *   Critical (>=highThreshold)  → BF16 (16 bits, preserved)
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class CompactStageElement extends StageElement {

  @reactive() private _deadThreshold = 0.1;
  @reactive() private _dormantThreshold = 0.2;
  @reactive() private _lowThreshold = 0.3;
  @reactive() private _mediumThreshold = 0.5;
  @reactive() private _highThreshold = 0.7;
  @reactive() private _targetSizeGb = 0;
  @reactive() private _enableQuantization = true;

  get stageType(): string { return 'compact'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'compact',
      deadThreshold: this._deadThreshold,
      dormantThreshold: this._dormantThreshold,
      lowThreshold: this._lowThreshold,
      mediumThreshold: this._mediumThreshold,
      highThreshold: this._highThreshold,
      ...(this._targetSizeGb > 0 ? { targetSizeGb: this._targetSizeGb } : {}),
      enableQuantization: this._enableQuantization,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._deadThreshold >= this._dormantThreshold) errors.push('Dead threshold must be less than dormant');
    if (this._dormantThreshold >= this._lowThreshold) errors.push('Dormant threshold must be less than low');
    if (this._lowThreshold >= this._mediumThreshold) errors.push('Low threshold must be less than medium');
    if (this._mediumThreshold >= this._highThreshold) errors.push('Medium threshold must be less than high');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #ffaa00; }

      .threshold-viz {
        display: flex;
        height: 24px;
        border-radius: 4px;
        overflow: hidden;
        margin: 4px 0 8px;
        font-size: 8px;
        font-weight: 700;
        letter-spacing: 0.03em;
      }

      .threshold-seg {
        display: flex;
        align-items: center;
        justify-content: center;
        color: rgba(0,0,0,0.7);
        transition: width 0.2s;
        white-space: nowrap;
        overflow: hidden;
      }

      .quant-toggle {
        font-size: 10px;
        padding: 3px 10px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .quant-toggle.active {
        background: rgba(255, 170, 0, 0.15);
        border-color: #ffaa00;
        color: #ffaa00;
      }
    `,
  ];

  private renderThresholdViz(): TemplateResult {
    const d = this._deadThreshold * 100;
    const dor = (this._dormantThreshold - this._deadThreshold) * 100;
    const low = (this._lowThreshold - this._dormantThreshold) * 100;
    const med = (this._mediumThreshold - this._lowThreshold) * 100;
    const high = (this._highThreshold - this._mediumThreshold) * 100;
    const crit = (1 - this._highThreshold) * 100;

    return html`
      <div class="threshold-viz">
        <div class="threshold-seg" style="width:${d}%;background:#ff4444">DEL</div>
        <div class="threshold-seg" style="width:${dor}%;background:#ff8844">1.6b</div>
        <div class="threshold-seg" style="width:${low}%;background:#ffaa44">Q2</div>
        <div class="threshold-seg" style="width:${med}%;background:#ffcc44">Q4</div>
        <div class="threshold-seg" style="width:${high}%;background:#88cc44">Q8</div>
        <div class="threshold-seg" style="width:${crit}%;background:#44cc88">BF16</div>
      </div>
    `;
  }

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      ${this.renderThresholdViz()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Dead (&lt; ${(this._deadThreshold * 100).toFixed(0)}%)</span>
          <div class="slider-row">
            <input type="range" min="0" max="0.3" step="0.01"
              .value=${String(this._deadThreshold)}
              @input=${(e: Event) => { this._deadThreshold = parseFloat((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value" style="color:#ff4444">${(this._deadThreshold * 100).toFixed(0)}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Dormant (&lt; ${(this._dormantThreshold * 100).toFixed(0)}%)</span>
          <div class="slider-row">
            <input type="range" min="0.05" max="0.5" step="0.01"
              .value=${String(this._dormantThreshold)}
              @input=${(e: Event) => { this._dormantThreshold = parseFloat((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value" style="color:#ff8844">${(this._dormantThreshold * 100).toFixed(0)}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Low (&lt; ${(this._lowThreshold * 100).toFixed(0)}%)</span>
          <div class="slider-row">
            <input type="range" min="0.1" max="0.6" step="0.01"
              .value=${String(this._lowThreshold)}
              @input=${(e: Event) => { this._lowThreshold = parseFloat((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value" style="color:#ffaa44">${(this._lowThreshold * 100).toFixed(0)}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Medium (&lt; ${(this._mediumThreshold * 100).toFixed(0)}%)</span>
          <div class="slider-row">
            <input type="range" min="0.2" max="0.8" step="0.01"
              .value=${String(this._mediumThreshold)}
              @input=${(e: Event) => { this._mediumThreshold = parseFloat((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value" style="color:#ffcc44">${(this._mediumThreshold * 100).toFixed(0)}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">High (&lt; ${(this._highThreshold * 100).toFixed(0)}%)</span>
          <div class="slider-row">
            <input type="range" min="0.4" max="0.95" step="0.01"
              .value=${String(this._highThreshold)}
              @input=${(e: Event) => { this._highThreshold = parseFloat((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value" style="color:#88cc44">${(this._highThreshold * 100).toFixed(0)}%</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Target Size (GB)</span>
          <input class="field-input" type="number" min="0" step="0.5" placeholder="Auto"
            .value=${this._targetSizeGb > 0 ? String(this._targetSizeGb) : ''}
            @change=${(e: Event) => { this._targetSizeGb = parseFloat((e.target as HTMLInputElement).value) || 0; this.emitChange(); }}>
          <span class="field-hint">0 = auto (threshold-driven)</span>
        </div>
      </div>
      <div style="margin-top:8px">
        <button class="quant-toggle ${this._enableQuantization ? 'active' : ''}"
          @click=${() => { this._enableQuantization = !this._enableQuantization; this.emitChange(); }}>
          ${this._enableQuantization ? 'Mixed-precision ON' : 'Mixed-precision OFF'}
        </button>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('compact-stage-element')) {
  customElements.define('compact-stage-element', CompactStageElement);
}
