/**
 * SourceConfigStageElement — Front bookend: declare model capabilities
 *
 * Maps to ForgeAlloy SourceConfigStage.
 * Context window, input modalities, target devices.
 */

import { html, css, reactive, type TemplateResult, type CSSResultGroup } from '../../shared/ReactiveWidget';
import { StageElement, STAGE_BASE_STYLES } from './StageElement';

const CONTEXT_PRESETS = [
  { value: 2048, label: '2K', color: '#5a6070' },
  { value: 4096, label: '4K', color: '#64c8ff' },
  { value: 8192, label: '8K', color: '#64c8ff' },
  { value: 16384, label: '16K', color: '#00d4ff' },
  { value: 32768, label: '32K', color: '#00ffc8' },
  { value: 65536, label: '64K', color: '#ffaa00' },
  { value: 131072, label: '128K', color: '#ff6464' },
] as const;

const MODALITIES = [
  { id: 'text', icon: 'T', label: 'Text', color: '#64c8ff' },
  { id: 'vision', icon: 'V', label: 'Vision', color: '#00ffc8' },
  { id: 'audio', icon: 'A', label: 'Audio', color: '#c864ff' },
  { id: 'video', icon: 'R', label: 'Video', color: '#ff6464' },
] as const;

const DEVICES = [
  { id: 'iPhone', label: 'iPhone', size: '4-8GB', icon: 'P' },
  { id: 'MacBook Air 8GB', label: 'Air 8GB', size: '8GB', icon: 'L' },
  { id: 'MacBook Air 16GB', label: 'Air 16GB', size: '16GB', icon: 'L' },
  { id: 'MacBook Pro 32GB', label: 'Pro 32GB', size: '32GB', icon: 'L' },
  { id: 'RTX 3090', label: '3090', size: '24GB', icon: 'G' },
  { id: 'RTX 4090', label: '4090', size: '24GB', icon: 'G' },
  { id: 'RTX 5090', label: '5090', size: '32GB', icon: 'G' },
] as const;

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
    if (mod === 'text') return; // text is always on
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

      .ctx-presets {
        display: flex;
        gap: 3px;
      }

      .ctx-btn {
        flex: 1;
        padding: 6px 2px;
        font-size: 10px;
        font-weight: 700;
        border: 1px solid rgba(255,255,255,0.08);
        border-radius: 4px;
        background: rgba(255,255,255,0.03);
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
        text-align: center;
      }

      .ctx-btn:hover {
        border-color: rgba(255,255,255,0.2);
      }

      .ctx-btn.active {
        border-color: var(--ctx-color, #64c8ff);
        color: var(--ctx-color, #64c8ff);
        background: color-mix(in srgb, var(--ctx-color, #64c8ff) 12%, transparent);
        box-shadow: 0 0 6px color-mix(in srgb, var(--ctx-color, #64c8ff) 20%, transparent);
      }

      .modality-grid {
        display: flex;
        gap: 6px;
      }

      .mod-btn {
        flex: 1;
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 3px;
        padding: 8px 4px;
        border: 1px solid rgba(255,255,255,0.08);
        border-radius: 6px;
        background: rgba(255,255,255,0.02);
        cursor: pointer;
        transition: all 0.2s;
      }

      .mod-btn:hover {
        border-color: rgba(255,255,255,0.15);
      }

      .mod-btn.active {
        border-color: var(--mod-color, #64c8ff);
        background: color-mix(in srgb, var(--mod-color, #64c8ff) 10%, transparent);
      }

      .mod-btn.locked {
        opacity: 0.7;
        cursor: default;
      }

      .mod-icon {
        width: 24px;
        height: 24px;
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 11px;
        font-weight: 800;
        background: rgba(255,255,255,0.06);
        color: var(--content-secondary, #8a92a5);
        transition: all 0.2s;
      }

      .mod-btn.active .mod-icon {
        background: var(--mod-color, #64c8ff);
        color: #000;
      }

      .mod-label {
        font-size: 9px;
        font-weight: 600;
        color: var(--content-secondary, #8a92a5);
        transition: color 0.2s;
      }

      .mod-btn.active .mod-label {
        color: var(--mod-color, #64c8ff);
      }

      .device-grid {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .dev-btn {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 4px 8px;
        border: 1px solid rgba(255,255,255,0.08);
        border-radius: 4px;
        background: rgba(255,255,255,0.02);
        cursor: pointer;
        transition: all 0.15s;
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
      }

      .dev-btn:hover {
        border-color: rgba(255,255,255,0.2);
      }

      .dev-btn.active {
        border-color: #00ffc8;
        color: #00ffc8;
        background: rgba(0, 255, 200, 0.08);
      }

      .dev-icon {
        font-size: 9px;
        font-weight: 800;
        opacity: 0.5;
      }

      .dev-btn.active .dev-icon {
        opacity: 1;
      }

      .dev-size {
        font-size: 8px;
        opacity: 0.5;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      <div class="stage-controls single-col">
        <div class="field">
          <span class="field-label">Context Window</span>
          <div class="ctx-presets">
            ${CONTEXT_PRESETS.map(p => html`
              <button class="ctx-btn ${this._contextLength === p.value ? 'active' : ''}"
                style="--ctx-color:${p.color}"
                @click=${() => { this._contextLength = p.value; this.emitChange(); }}>${p.label}</button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">Input Modalities</span>
          <div class="modality-grid">
            ${MODALITIES.map(m => html`
              <button class="mod-btn ${this._inputModalities.includes(m.id) ? 'active' : ''} ${m.id === 'text' ? 'locked' : ''}"
                style="--mod-color:${m.color}"
                @click=${() => this.toggleModality(m.id)}>
                <div class="mod-icon">${m.icon}</div>
                <span class="mod-label">${m.label}</span>
              </button>
            `)}
          </div>
        </div>
        <div class="field">
          <span class="field-label">Target Devices</span>
          <div class="device-grid">
            ${DEVICES.map(d => html`
              <button class="dev-btn ${this._targetDevices.includes(d.id) ? 'active' : ''}"
                @click=${() => this.toggleDevice(d.id)}>
                <span class="dev-icon">${d.icon}</span>
                <span>${d.label}</span>
                <span class="dev-size">${d.size}</span>
              </button>
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
