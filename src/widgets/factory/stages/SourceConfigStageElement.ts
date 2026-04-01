/**
 * SourceConfigStageElement — Front bookend: declare model capabilities
 *
 * Maps to ForgeAlloy SourceConfigStage.
 * Context window, input modalities, target devices.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

export class SourceConfigStageElement extends StageElement {

  @reactive() private _contextLength = 4096;
  @reactive() private _inputModalities: string[] = ['text'];
  @reactive() private _targetDevices: string[] = [];

  get stageType(): string { return 'source-config'; }

  get stageConfig(): Record<string, unknown> {
    return {
      type: 'source-config',
      contextLength: this._contextLength,
      inputModalities: this._inputModalities,
      targetDevices: this._targetDevices,
    };
  }

  private toggleModality(mod: string): void {
    if (this._inputModalities.includes(mod)) {
      this._inputModalities = this._inputModalities.filter(m => m !== mod);
    } else {
      this._inputModalities = [...this._inputModalities, mod];
    }
    this.emitChange();
  }

  private toggleDevice(dev: string): void {
    if (this._targetDevices.includes(dev)) {
      this._targetDevices = this._targetDevices.filter(d => d !== dev);
    } else {
      this._targetDevices = [...this._targetDevices, dev];
    }
    this.emitChange();
  }

  static override styles: CSSResultGroup = [
    STAGE_BASE_STYLES,
    css`
      :host { border-left: 3px solid #64c8ff; }

      .modality-toggles, .device-toggles {
        display: flex;
        gap: 4px;
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
        border-color: rgba(0, 212, 255, 0.4);
        color: #00d4ff;
      }
    `,
  ];

  protected override render(): TemplateResult {
    const modalities = ['text', 'vision', 'audio', 'video'];
    const devices = ['iPhone', 'MacBook Air 8GB', 'MacBook Pro 32GB', 'RTX 3090', 'RTX 5090'];

    return html`
      ${this.renderHeader()}
      <div class="stage-controls">
        <div class="field">
          <span class="field-label">Context Window</span>
          <select class="field-select"
            .value=${String(this._contextLength)}
            @change=${(e: Event) => { this._contextLength = parseInt((e.target as HTMLSelectElement).value); this.emitChange(); }}>
            <option value="2048">2K</option>
            <option value="4096">4K</option>
            <option value="8192">8K</option>
            <option value="16384">16K</option>
            <option value="32768">32K</option>
            <option value="65536">64K</option>
            <option value="131072">128K</option>
          </select>
        </div>
        <div class="field">
          <span class="field-label">Input Modalities</span>
          <div class="modality-toggles">
            ${modalities.map(m => html`
              <button class="toggle-chip ${this._inputModalities.includes(m) ? 'active' : ''}"
                @click=${() => this.toggleModality(m)}>${m}</button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">Target Devices</span>
          <div class="device-toggles">
            ${devices.map(d => html`
              <button class="toggle-chip ${this._targetDevices.includes(d) ? 'active' : ''}"
                @click=${() => this.toggleDevice(d)}>${d}</button>
            `)}
          </div>
        </div>
      </div>
      ${this.renderGate()}
    `;
  }
}

if (!customElements.get('source-config-stage-element')) {
  customElements.define('source-config-stage-element', SourceConfigStageElement);
}
