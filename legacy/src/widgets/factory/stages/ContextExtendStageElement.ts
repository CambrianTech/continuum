/**
 * ContextExtendStageElement — UI for the alloy 'context-extend' stage
 *
 * Controls: target length, RoPE method (YaRN, NTK, linear, dynamic-NTK), training steps
 * Maps 1:1 to ForgeRecipe ContextExtendStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

const CONTEXT_PRESETS: Record<string, { length: number; label: string }> = {
  '32k':  { length: 32768,  label: '32K' },
  '64k':  { length: 65536,  label: '64K' },
  '128k': { length: 131072, label: '128K' },
  '256k': { length: 262144, label: '256K' },
};

export class ContextExtendStageElement extends StageElement {

  @reactive() private _targetLength = 131072;
  @reactive() private _method: 'yarn' | 'ntk' | 'linear' | 'dynamic-ntk' = 'yarn';
  @reactive() private _trainingSteps = 200;
  @reactive() private _trainingDataset = '';

  get stageType(): string { return 'context-extend'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'context-extend',
      targetLength: this._targetLength,
      method: this._method,
      trainingSteps: this._trainingSteps,
      ...(this._trainingDataset ? { trainingDataset: this._trainingDataset } : {}),
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._targetLength < 1024) errors.push('Target length must be at least 1024');
    if (this._trainingSteps < 1) errors.push('Training steps must be at least 1');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #c864ff; }

      .preset-row {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .preset-btn {
        font-size: 10px;
        padding: 3px 10px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .preset-btn:hover {
        border-color: #c864ff;
        color: #c864ff;
      }

      .preset-btn.active {
        background: rgba(200, 100, 255, 0.15);
        border-color: #c864ff;
        color: #c864ff;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Target Context Length</span>
          <div class="preset-row">
            ${Object.entries(CONTEXT_PRESETS).map(([key, p]) => html`
              <button class="preset-btn ${this._targetLength === p.length ? 'active' : ''}"
                @click=${() => { this._targetLength = p.length; this.emitChange(); }}>${p.label}</button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">RoPE Method</span>
          <select class="field-select"
            .value=${this._method}
            @change=${(e: Event) => { this._method = (e.target as HTMLSelectElement).value as typeof this._method; this.emitChange(); }}>
            <option value="yarn">YaRN (recommended)</option>
            <option value="ntk">NTK-Aware</option>
            <option value="linear">Linear Scaling</option>
            <option value="dynamic-ntk">Dynamic NTK</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Fine-tune Steps</span>
          <div class="slider-row">
            <input type="range" min="0" max="2000" step="50"
              .value=${String(this._trainingSteps)}
              @input=${(e: Event) => { this._trainingSteps = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._trainingSteps || 'none'}</span>
          </div>
          <span class="field-hint">0 = config-only (no training), >0 = fine-tune on long sequences</span>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('context-extend-stage-element')) {
  customElements.define('context-extend-stage-element', ContextExtendStageElement);
}
