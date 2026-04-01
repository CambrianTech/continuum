/**
 * LoraStageElement — UI for the alloy 'lora' stage
 *
 * Controls: rank, alpha, dropout, target modules, QLoRA config, dataset, epochs, merge
 * Maps 1:1 to ForgeAlloy LoraStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

const ALL_TARGET_MODULES = [
  'q_proj', 'k_proj', 'v_proj', 'o_proj',
  'gate_proj', 'up_proj', 'down_proj',
] as const;

export class LoraStageElement extends StageElement {

  @reactive() private _rank = 32;
  @reactive() private _alpha = 64;
  @reactive() private _dropout = 0.05;
  @reactive() private _targetModules: string[] = ['q_proj', 'k_proj', 'v_proj', 'o_proj'];
  @reactive() private _quantize = true;
  @reactive() private _quantizeBits: 4 | 8 = 4;
  @reactive() private _dataset = '';
  @reactive() private _epochs = 2;
  @reactive() private _learningRate = '1e-4';
  @reactive() private _batchSize = 4;
  @reactive() private _mergeAfter = true;

  get stageType(): string { return 'lora'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'lora',
      rank: this._rank,
      alpha: this._alpha,
      dropout: this._dropout,
      targetModules: this._targetModules,
      quantize: this._quantize,
      quantizeBits: this._quantizeBits,
      ...(this._dataset ? { dataset: this._dataset } : {}),
      epochs: this._epochs,
      learningRate: this._learningRate,
      batchSize: this._batchSize,
      mergeAfter: this._mergeAfter,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._rank < 1 || this._rank > 256) errors.push('Rank must be 1-256');
    if (this._alpha < 1 || this._alpha > 512) errors.push('Alpha must be 1-512');
    if (this._targetModules.length === 0) errors.push('At least one target module required');
    return errors;
  }

  private toggleModule(mod: string): void {
    if (this._targetModules.includes(mod)) {
      this._targetModules = this._targetModules.filter(m => m !== mod);
    } else {
      this._targetModules = [...this._targetModules, mod];
    }
    this.emitChange();
  }

  private selectAllModules(): void {
    this._targetModules = [...ALL_TARGET_MODULES];
    this.emitChange();
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #9664ff; }

      .module-toggles {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .module-chip {
        font-size: 10px;
        font-family: monospace;
        padding: 3px 8px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .module-chip.active {
        background: rgba(150, 100, 255, 0.15);
        border-color: #9664ff;
        color: #9664ff;
      }

      .module-chip.select-all {
        font-family: inherit;
        font-weight: 600;
      }

      .qlora-row {
        display: flex;
        align-items: center;
        gap: 10px;
      }

      .qlora-toggle {
        font-size: 10px;
        padding: 3px 10px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .qlora-toggle.active {
        background: rgba(150, 100, 255, 0.15);
        border-color: #9664ff;
        color: #9664ff;
      }

      .merge-toggle {
        font-size: 10px;
        padding: 3px 10px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .merge-toggle.active {
        background: rgba(0, 255, 200, 0.1);
        border-color: #00ffc8;
        color: #00ffc8;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Rank (r)</span>
          <div class="slider-row">
            <input type="range" min="4" max="256" step="4"
              .value=${String(this._rank)}
              @input=${(e: Event) => { this._rank = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._rank}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Alpha</span>
          <div class="slider-row">
            <input type="range" min="4" max="512" step="4"
              .value=${String(this._alpha)}
              @input=${(e: Event) => { this._alpha = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._alpha}</span>
          </div>
          <span class="field-hint">Typical: alpha = 2x rank</span>
        </div>
        <div class="field">
          <span class="field-label">Epochs</span>
          <div class="slider-row">
            <input type="range" min="1" max="10" step="1"
              .value=${String(this._epochs)}
              @input=${(e: Event) => { this._epochs = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._epochs}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Learning Rate</span>
          <select class="field-select"
            .value=${this._learningRate}
            @change=${(e: Event) => { this._learningRate = (e.target as HTMLSelectElement).value; this.emitChange(); }}>
            <option value="5e-5">5e-5</option>
            <option value="1e-4">1e-4 (recommended)</option>
            <option value="2e-4">2e-4</option>
            <option value="5e-4">5e-4</option>
          </select>
        </div>
      </div>
      <div class="stage-controls single-col">
        <div class="field">
          <span class="field-label">Target Modules</span>
          <div class="module-toggles">
            <button class="module-chip select-all"
              @click=${this.selectAllModules}>All</button>
            ${ALL_TARGET_MODULES.map(mod => html`
              <button class="module-chip ${this._targetModules.includes(mod) ? 'active' : ''}"
                @click=${() => this.toggleModule(mod)}>${mod}</button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">QLoRA</span>
          <div class="qlora-row">
            <button class="qlora-toggle ${this._quantize ? 'active' : ''}"
              @click=${() => { this._quantize = !this._quantize; this.emitChange(); }}>
              ${this._quantize ? 'QLoRA ON' : 'QLoRA OFF'}
            </button>
            ${this._quantize ? html`
              <button class="qlora-toggle ${this._quantizeBits === 4 ? 'active' : ''}"
                @click=${() => { this._quantizeBits = 4; this.emitChange(); }}>4-bit</button>
              <button class="qlora-toggle ${this._quantizeBits === 8 ? 'active' : ''}"
                @click=${() => { this._quantizeBits = 8; this.emitChange(); }}>8-bit</button>
            ` : ''}
            <button class="merge-toggle ${this._mergeAfter ? 'active' : ''}"
              @click=${() => { this._mergeAfter = !this._mergeAfter; this.emitChange(); }}>
              ${this._mergeAfter ? 'Merge after' : 'Keep adapter'}
            </button>
          </div>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('lora-stage-element')) {
  customElements.define('lora-stage-element', LoraStageElement);
}
