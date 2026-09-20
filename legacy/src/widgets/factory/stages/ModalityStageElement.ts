/**
 * ModalityStageElement — UI for the alloy 'modality' stage
 *
 * Bolt vision, audio, or multimodal encoders onto a text model.
 * Controls: modality type, encoder model, projection arch, freeze options, training
 * Maps 1:1 to ForgeRecipe ModalityStage schema.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { nothing } from 'lit';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

const RECOMMENDED_ENCODERS: Record<string, { model: string; dataset: string; label: string }> = {
  vision:     { model: 'openai/clip-vit-large-patch14',   dataset: 'liuhaotian/LLaVA-Instruct-150K', label: 'CLIP ViT-L/14' },
  audio:      { model: 'openai/whisper-large-v3',         dataset: 'librispeech_asr',                 label: 'Whisper Large v3' },
  multimodal: { model: 'openai/clip-vit-large-patch14',   dataset: 'MBZUAI/VideoInstruct-100K',       label: 'CLIP + Video' },
};

export class ModalityStageElement extends StageElement {

  @reactive() private _modality: 'vision' | 'audio' | 'multimodal' = 'vision';
  @reactive() private _encoderModel = '';
  @reactive() private _projectionArch: 'mlp' | 'cross-attention' | 'linear' = 'mlp';
  @reactive() private _freezeBase = true;
  @reactive() private _freezeEncoder = true;
  @reactive() private _trainingDataset = '';
  @reactive() private _trainingSteps = 1000;
  @reactive() private _projectionDim = 0;
  // Resolution / quality controls (vision = pixels, audio = sample rate)
  @reactive() private _resolution = 448;
  @reactive() private _sampleRate = 16000;

  get stageType(): string { return 'modality'; }

  get stageConfig(): Record<string, unknown> {
    const rec = RECOMMENDED_ENCODERS[this._modality];
    return {
      type: 'modality',
      modality: this._modality,
      encoderModel: this._encoderModel || rec?.model || '',
      projectionArch: this._projectionArch,
      freezeBase: this._freezeBase,
      freezeEncoder: this._freezeEncoder,
      ...(this._trainingDataset || rec?.dataset ? { trainingDataset: this._trainingDataset || rec?.dataset } : {}),
      trainingSteps: this._trainingSteps,
      ...(this._projectionDim > 0 ? { projectionDim: this._projectionDim } : {}),
      ...(this._modality === 'vision' || this._modality === 'multimodal'
        ? { resolution: this._resolution } : {}),
      ...(this._modality === 'audio' || this._modality === 'multimodal'
        ? { sampleRate: this._sampleRate } : {}),
    };
  }

  override validate(): string[] {
    const errors: string[] = [];
    const rec = RECOMMENDED_ENCODERS[this._modality];
    if (!this._encoderModel && !rec?.model) errors.push('Encoder model is required');
    if (this._trainingSteps < 1) errors.push('Training steps must be at least 1');
    return errors;
  }

  private onModalityChange(modality: typeof this._modality): void {
    this._modality = modality;
    // Auto-fill recommended encoder + dataset
    const rec = RECOMMENDED_ENCODERS[modality];
    if (rec) {
      this._encoderModel = rec.model;
      this._trainingDataset = rec.dataset;
    }
    this.emitChange();
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #64ffc8; }

      .modality-pills {
        display: flex;
        gap: 6px;
      }

      .modality-pill {
        font-size: 11px;
        font-weight: 600;
        padding: 5px 14px;
        border-radius: 4px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .modality-pill:hover {
        border-color: #64ffc8;
        color: #64ffc8;
      }

      .modality-pill.active {
        background: rgba(100, 255, 200, 0.15);
        border-color: #64ffc8;
        color: #64ffc8;
      }

      .freeze-toggles {
        display: flex;
        gap: 8px;
      }

      .freeze-chip {
        font-size: 10px;
        padding: 3px 8px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .freeze-chip.active {
        background: rgba(100, 255, 200, 0.15);
        border-color: #64ffc8;
        color: #64ffc8;
      }

      .auto-hint {
        font-size: 9px;
        color: var(--content-tertiary, #5a6070);
        font-style: italic;
        margin-top: 2px;
      }
    `,
  ];

  protected override render(): TemplateResult {
    const rec = RECOMMENDED_ENCODERS[this._modality];

    return html`
      ${this.renderHeader()}
      <div class="stage-controls single-col">
        <div class="field">
          <span class="field-label">Modality</span>
          <div class="modality-pills">
            ${(['vision', 'audio', 'multimodal'] as const).map(m => html`
              <button class="modality-pill ${this._modality === m ? 'active' : ''}"
                @click=${() => this.onModalityChange(m)}>${m}</button>
            `)}
          </div>
        </div>
      </div>
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Encoder Model</span>
          <input class="field-input" type="text"
            placeholder=${rec?.model ?? 'e.g. openai/clip-vit-large-patch14'}
            .value=${this._encoderModel}
            @change=${(e: Event) => { this._encoderModel = (e.target as HTMLInputElement).value; this.emitChange(); }}>
          ${rec ? html`<span class="auto-hint">Default: ${rec.label}</span>` : ''}
        </div>
        <div class="field">
          <span class="field-label">Projection</span>
          <select class="field-select"
            .value=${this._projectionArch}
            @change=${(e: Event) => { this._projectionArch = (e.target as HTMLSelectElement).value as typeof this._projectionArch; this.emitChange(); }}>
            <option value="mlp">MLP (recommended)</option>
            <option value="cross-attention">Cross-Attention</option>
            <option value="linear">Linear</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Training Steps</span>
          <div class="slider-row">
            <input type="range" min="100" max="10000" step="100"
              .value=${String(this._trainingSteps)}
              @input=${(e: Event) => { this._trainingSteps = parseInt((e.target as HTMLInputElement).value); this.emitChange(); }}>
            <span class="slider-value">${this._trainingSteps}</span>
          </div>
        </div>
        <div class="field">
          <span class="field-label">Freeze Layers</span>
          <div class="freeze-toggles">
            <button class="freeze-chip ${this._freezeBase ? 'active' : ''}"
              @click=${() => { this._freezeBase = !this._freezeBase; this.emitChange(); }}>Base Model</button>
            <button class="freeze-chip ${this._freezeEncoder ? 'active' : ''}"
              @click=${() => { this._freezeEncoder = !this._freezeEncoder; this.emitChange(); }}>Encoder</button>
          </div>
          <span class="field-hint">Freeze = train projection only (faster, less VRAM)</span>
        </div>
        ${this._modality === 'vision' || this._modality === 'multimodal' ? html`
          <div class="field">
            <span class="field-label">Resolution (px)</span>
            <select class="field-select"
              .value=${String(this._resolution)}
              @change=${(e: Event) => { this._resolution = parseInt((e.target as HTMLSelectElement).value); this.emitChange(); }}>
              <option value="224">224 (fast, low quality)</option>
              <option value="336">336</option>
              <option value="448">448 (default)</option>
              <option value="672">672</option>
              <option value="896">896 (high quality)</option>
              <option value="1344">1344 (very high)</option>
            </select>
          </div>
        ` : nothing}
        ${this._modality === 'audio' || this._modality === 'multimodal' ? html`
          <div class="field">
            <span class="field-label">Sample Rate</span>
            <select class="field-select"
              .value=${String(this._sampleRate)}
              @change=${(e: Event) => { this._sampleRate = parseInt((e.target as HTMLSelectElement).value); this.emitChange(); }}>
              <option value="8000">8kHz (phone quality)</option>
              <option value="16000">16kHz (default, speech)</option>
              <option value="22050">22kHz</option>
              <option value="44100">44.1kHz (CD quality)</option>
              <option value="48000">48kHz (studio)</option>
            </select>
          </div>
        ` : nothing}
        <div class="field">
          <span class="field-label">Training Dataset</span>
          <input class="field-input" type="text"
            placeholder=${rec?.dataset ?? 'HuggingFace dataset path'}
            .value=${this._trainingDataset}
            @change=${(e: Event) => { this._trainingDataset = (e.target as HTMLInputElement).value; this.emitChange(); }}>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('modality-stage-element')) {
  customElements.define('modality-stage-element', ModalityStageElement);
}
