/**
 * QuantStageElement — Output stage: quantization for device targets
 *
 * Maps to ForgeRecipe QuantStage.
 * Format (GGUF/MLX/ONNX), quant types, device targets.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class QuantStageElement extends StageElement {

  @reactive() private _format: 'gguf' | 'mlx' | 'safetensors' | 'onnx' = 'gguf';
  @reactive() private _quantTypes: string[] = ['Q4_K_M', 'Q8_0'];
  @reactive() private _deviceTargets: string[] = [];

  get stageType(): string { return 'quant'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'quant',
      format: this._format,
      quantTypes: this._quantTypes,
      deviceTargets: this._deviceTargets,
    };
  }

  private toggleQuant(q: string): void {
    if (this._quantTypes.includes(q)) {
      this._quantTypes = this._quantTypes.filter(t => t !== q);
    } else {
      this._quantTypes = [...this._quantTypes, q];
    }
    this.emitChange();
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #00ffc8; }

      .quant-toggles {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .toggle-chip {
        font-size: 9px;
        font-family: monospace;
        padding: 3px 6px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .toggle-chip.active {
        background: rgba(0, 255, 200, 0.15);
        border-color: rgba(0, 255, 200, 0.4);
        color: #00ffc8;
      }
    `,
  ];

  protected override render(): TemplateResult {
    const quantOptions = ['Q2_K', 'Q3_K_M', 'IQ4_XS', 'Q4_K_S', 'Q4_K_M', 'Q5_K_M', 'Q6_K', 'Q8_0', 'F16'];

    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Format</span>
          <select class="field-select"
            .value=${this._format}
            @change=${(e: Event) => { this._format = (e.target as HTMLSelectElement).value as typeof this._format; this.emitChange(); }}>
            <option value="gguf">GGUF (llama.cpp)</option>
            <option value="mlx">MLX (Apple Silicon)</option>
            <option value="safetensors">Safetensors (HuggingFace)</option>
            <option value="onnx">ONNX (cross-platform)</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Quantization Types</span>
          <div class="quant-toggles">
            ${quantOptions.map(q => html`
              <button class="toggle-chip ${this._quantTypes.includes(q) ? 'active' : ''}"
                @click=${() => this.toggleQuant(q)}>${q}</button>
            `)}
          </div>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('quant-stage-element')) {
  customElements.define('quant-stage-element', QuantStageElement);
}
