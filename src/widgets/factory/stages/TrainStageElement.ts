/**
 * TrainStageElement — UI for the alloy 'train' stage
 *
 * Controls: domain, dataset, steps, learning rate, batch size, scheduler, precision, optimizations
 * Maps 1:1 to ForgeAlloy TrainStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class TrainStageElement extends StageElement {

  @reactive() private _domain = 'code';
  @reactive() private _dataset = '';
  @reactive() private _steps = 1000;
  @reactive() private _learningRate = '2e-4';
  @reactive() private _batchSize = 4;
  @reactive() private _gradientAccumulation = 4;
  @reactive() private _scheduler: 'cosine' | 'linear' | 'constant' = 'cosine';
  @reactive() private _warmupRatio = 0.03;
  @reactive() private _precision: 'bf16' | 'fp16' | 'fp32' = 'bf16';
  @reactive() private _sequenceLength = 2048;
  @reactive() private _flashAttention = true;
  @reactive() private _gradientCheckpointing = true;

  get stageType(): string { return 'train'; }

  get stageConfig(): Record<string, unknown> {
    const optimizations: string[] = [];
    if (this._flashAttention) optimizations.push('flash_attention');
    if (this._gradientCheckpointing) optimizations.push('gradient_checkpointing');

    return {
      type: 'train',
      domain: this._domain,
      ...(this._dataset ? { dataset: this._dataset } : {}),
      steps: this._steps,
      learningRate: this._learningRate,
      batchSize: this._batchSize,
      gradientAccumulation: this._gradientAccumulation,
      scheduler: this._scheduler,
      warmupRatio: this._warmupRatio,
      precision: this._precision,
      sequenceLength: this._sequenceLength,
      optimizations,
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    if (this._steps < 1) errors.push('Steps must be at least 1');
    if (this._batchSize < 1 || this._batchSize > 64) errors.push('Batch size must be 1-64');
    return errors;
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host {
        border-left: 3px solid #00d4ff;
      }

      .optimization-toggles {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
      }

      .toggle-chip {
        font-size: 10px;
        padding: 3px 8px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .toggle-chip.active {
        background: rgba(0, 212, 255, 0.15);
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Domain</span>
          <select class="field-select"
            .value=${this._domain}
            @change=${(e: Event) => { this._domain = (e.target as HTMLSelectElement).value; this.emitChange(); }}>
            <option value="code">Code</option>
            <option value="reasoning">Reasoning</option>
            <option value="general">General</option>
            <option value="chat">Chat</option>
            <option value="science">Science</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Steps</span>
          <div class="slider-row">
            <input type="range" min="50" max="5000" step="50"
              .value=${String(this._steps)}
              @input=${(e: Event) => { this._steps = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._steps}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Learning Rate</span>
          <select class="field-select"
            .value=${this._learningRate}
            @change=${(e: Event) => { this._learningRate = (e.target as HTMLSelectElement).value; this.emitChange(); }}>
            <option value="1e-5">1e-5</option>
            <option value="5e-5">5e-5</option>
            <option value="1e-4">1e-4</option>
            <option value="2e-4">2e-4</option>
            <option value="5e-4">5e-4</option>
            <option value="1e-3">1e-3</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Batch Size</span>
          <div class="slider-row">
            <input type="range" min="1" max="32" step="1"
              .value=${String(this._batchSize)}
              @input=${(e: Event) => { this._batchSize = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._batchSize}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Scheduler</span>
          <select class="field-select"
            .value=${this._scheduler}
            @change=${(e: Event) => { this._scheduler = (e.target as HTMLSelectElement).value as typeof this._scheduler; this.emitChange(); }}>
            <option value="cosine">Cosine</option>
            <option value="linear">Linear</option>
            <option value="constant">Constant</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Precision</span>
          <select class="field-select"
            .value=${this._precision}
            @change=${(e: Event) => { this._precision = (e.target as HTMLSelectElement).value as typeof this._precision; this.emitChange(); }}>
            <option value="bf16">BF16 (recommended)</option>
            <option value="fp16">FP16</option>
            <option value="fp32">FP32</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Sequence Length</span>
          <select class="field-select"
            .value=${String(this._sequenceLength)}
            @change=${(e: Event) => { this._sequenceLength = parseInt((e.target as HTMLSelectElement).value); this.emitChange(); }}>
            <option value="256">256</option>
            <option value="512">512</option>
            <option value="1024">1024</option>
            <option value="2048">2048</option>
            <option value="4096">4096</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Optimizations</span>
          <div class="optimization-toggles">
            <button class="toggle-chip ${this._flashAttention ? 'active' : ''}"
              @click=${() => { this._flashAttention = !this._flashAttention; this.emitChange(); }}>Flash Attention</button>
            <button class="toggle-chip ${this._gradientCheckpointing ? 'active' : ''}"
              @click=${() => { this._gradientCheckpointing = !this._gradientCheckpointing; this.emitChange(); }}>Grad Checkpoint</button>
          </div>
        </div>
      </div>
      ${this._dataset ? html`
        <div class="field">
          <span class="field-label">Dataset</span>
          <span class="field-hint">${this._dataset}</span>
        </div>
      ` : ''}
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('train-stage-element')) {
  customElements.define('train-stage-element', TrainStageElement);
}
