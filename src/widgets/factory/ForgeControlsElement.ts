/**
 * ForgeControlsElement — Forge parameter form + start button
 *
 * Emits 'forge-start' event with alloy-shaped params when user clicks START.
 * Emits 'forge-export' event when user clicks Export Alloy.
 * The parent widget handles command execution.
 */

import {
  ReactiveWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import './stages/PipelineComposer';

/** Forge profiles — presets for common configurations */
const FORGE_PROFILES: Record<string, { prune: number; cycles: number; lr: string; steps: number; label: string; risk: string }> = {
  conservative: { prune: 10, cycles: 5, lr: '1e-4', steps: 2000, label: 'Conservative', risk: 'Low — safe improvement' },
  balanced:     { prune: 30, cycles: 3, lr: '2e-4', steps: 1000, label: 'Balanced', risk: 'Medium — best tradeoff' },
  aggressive:   { prune: 50, cycles: 2, lr: '5e-4', steps: 500,  label: 'Aggressive', risk: 'High — maximum compression' },
  yolo:         { prune: 70, cycles: 1, lr: '1e-3', steps: 250,  label: 'YOLO', risk: 'Extreme — might break the model' },
};

export class ForgeControlsElement extends ReactiveWidget {

  @reactive() forging = false;
  @reactive() starting = false;
  @reactive() progressPct = 0;
  @reactive() progressLabel = '';

  @reactive() private _model = 'Qwen/Qwen3.5-4B';
  @reactive() private _domain = 'code';
  @reactive() private _experts = 0;
  @reactive() private _steps = 2000;
  @reactive() private _pruneLevel = 30;
  @reactive() private _pruneStrategy: 'entropy' | 'gradient' | 'combined' = 'entropy';
  @reactive() private _cycles = 3;
  @reactive() private _learningRate = '2e-4';

  private get _isMoe(): boolean {
    return this._model.includes('35B') || this._model.includes('MoE');
  }

  /** Current settings as alloy-shaped params */
  get forgeParams(): Record<string, unknown> {
    return {
      model: this._model,
      domain: this._domain,
      experts: this._isMoe ? (this._experts || 64) : 0,
      steps: this._steps,
      pruneLevel: this._pruneLevel / 100,
      pruneStrategy: this._pruneStrategy,
      cycles: this._cycles,
      learningRate: this._learningRate,
    };
  }

  /** Current settings as an alloy recipe */
  get alloyRecipe(): Record<string, unknown> {
    const base = this._model.split('/').pop()?.toLowerCase() ?? 'model';
    return {
      name: `${base}-${this._domain}-forged`,
      version: '1.0.0',
      author: 'continuum-ai',
      tags: [this._domain, 'forged', 'experiential-plasticity', 'forge-alloy'],
      license: 'apache-2.0',
      source: {
        baseModel: this._model,
        architecture: base.includes('qwen3.5') ? 'qwen3_5' : base.includes('qwen2') ? 'qwen2' : 'llama',
      },
      stages: [
        { type: 'prune', strategy: this._pruneStrategy, level: this._pruneLevel / 100 },
        { type: 'train', domain: this._domain, steps: this._steps, learningRate: this._learningRate },
      ],
      cycles: this._cycles,
    };
  }

  private applyProfile(name: string): void {
    const p = FORGE_PROFILES[name];
    if (!p) return;
    this._pruneLevel = p.prune;
    this._cycles = p.cycles;
    this._learningRate = p.lr;
    this._steps = p.steps;
  }

  private onStartForge(): void {
    this.dispatchEvent(new CustomEvent('forge-start', { detail: this.forgeParams, bubbles: true, composed: true }));
  }

  private onExportAlloy(): void {
    this.dispatchEvent(new CustomEvent('forge-export', { detail: this.alloyRecipe, bubbles: true, composed: true }));
  }

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
    :host { display: block; }

    .controls {
      background: var(--surface-elevated, rgba(255,255,255,0.04));
      border: 1px solid var(--border-color, rgba(255,255,255,0.08));
      border-radius: 8px;
      padding: 16px 20px;
    }

    .controls-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 12px;
      margin-bottom: 16px;
    }

    .control-group {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }

    .control-label {
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.04em;
      color: var(--content-secondary, #8a92a5);
    }

    .control-select, .control-input {
      background: rgba(0,0,0,0.3);
      border: 1px solid var(--border-color, rgba(255,255,255,0.12));
      border-radius: 4px;
      color: var(--content-primary, #e0e6ed);
      font-size: 13px;
      padding: 8px 10px;
      font-family: inherit;
      outline: none;
      transition: border-color 0.2s;
    }

    .control-select:focus, .control-input:focus {
      border-color: var(--accent-primary, #00d4ff);
    }

    .control-select option {
      background: #0a1520;
      color: #e0e6ed;
    }

    .slider-row {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .slider-row input[type="range"] {
      flex: 1;
      accent-color: var(--accent-primary, #00d4ff);
      height: 4px;
    }

    .slider-value {
      font-size: 13px;
      font-weight: 600;
      font-variant-numeric: tabular-nums;
      min-width: 50px;
      text-align: right;
      color: var(--accent-primary, #00d4ff);
    }

    .profile-row {
      display: flex;
      gap: 6px;
      flex-wrap: wrap;
    }

    .profile-btn {
      padding: 4px 10px;
      font-size: 11px;
      border: 1px solid rgba(255,255,255,0.2);
      border-radius: 4px;
      background: rgba(255,255,255,0.05);
      color: rgba(255,255,255,0.8);
      cursor: pointer;
      transition: all 0.15s;
    }

    .profile-btn:hover {
      background: var(--accent-primary, #00d4ff);
      color: #000;
      border-color: transparent;
    }

    pipeline-composer {
      margin: 12px 0;
    }

    .button-row {
      display: flex;
      gap: 8px;
      margin-bottom: 16px;
    }

    .forge-button {
      flex: 1;
      position: relative;
      overflow: hidden;
      padding: 10px;
      background: linear-gradient(135deg, rgba(0, 212, 255, 0.2), rgba(0, 255, 200, 0.2));
      border: 1px solid var(--accent-primary, #00d4ff);
      border-radius: 6px;
      color: var(--accent-primary, #00d4ff);
      font-size: 14px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      cursor: pointer;
      transition: all 0.2s;
    }

    .forge-button:hover {
      background: linear-gradient(135deg, rgba(0, 212, 255, 0.35), rgba(0, 255, 200, 0.35));
      box-shadow: 0 0 16px rgba(0, 212, 255, 0.3);
    }

    .forge-button:disabled {
      opacity: 0.4;
      cursor: not-allowed;
      box-shadow: none;
    }

    .forge-button.forging {
      background: linear-gradient(135deg, rgba(255, 170, 0, 0.2), rgba(255, 100, 0, 0.2));
      border-color: #ffaa00;
      color: #ffaa00;
      animation: pulse-glow 2s ease-in-out infinite;
    }

    @keyframes pulse-glow {
      0%, 100% { box-shadow: 0 0 8px rgba(255, 170, 0, 0.2); }
      50% { box-shadow: 0 0 20px rgba(255, 170, 0, 0.4); }
    }

    .forge-button-fill {
      position: absolute;
      top: 0;
      left: 0;
      height: 100%;
      opacity: 0.3;
      transition: width 0.5s ease, background 0.8s ease;
    }

    .forge-button-label {
      position: relative;
      z-index: 1;
    }

    .export-btn {
      align-self: stretch;
      padding: 6px 14px;
      font-size: 11px;
      font-weight: 600;
      border: 1px solid var(--border-color, rgba(255,255,255,0.15));
      border-radius: 4px;
      background: rgba(255,255,255,0.05);
      color: var(--content-primary, #e0e6ed);
      cursor: pointer;
      transition: all 0.15s;
    }

    .export-btn:hover {
      background: rgba(0, 212, 255, 0.15);
      border-color: var(--accent-primary, #00d4ff);
      color: var(--accent-primary, #00d4ff);
    }
  `];

  protected override render(): TemplateResult {
    const hue = 185 - this.progressPct * 0.4;

    return html`
      <div class="controls">
        <div class="button-row">
          <button class="forge-button ${this.forging ? 'forging' : ''}"
            ?disabled=${this.starting}
            @click=${this.onStartForge}>
            ${this.forging ? html`
              <span class="forge-button-fill"
                style="width:${this.progressPct}%;background:linear-gradient(90deg,hsl(${hue},100%,50%),hsl(${hue - 10},100%,60%))"></span>
            ` : nothing}
            <span class="forge-button-label">
              ${this.forging ? this.progressLabel : this.starting ? 'STARTING...' : 'START FORGE'}
            </span>
          </button>
          <button class="export-btn" @click=${this.onExportAlloy}
            title="Export current settings as .alloy.json recipe">Export Alloy</button>
        </div>
        <div class="controls-grid">
          <div class="control-group">
            <span class="control-label">Base Model</span>
            <select class="control-select"
              .value=${this._model}
              @change=${(e: Event) => this._model = (e.target as HTMLSelectElement).value}>
              <option value="Qwen/Qwen3.5-4B">Qwen3.5-4B (8GB fp16)</option>
              <option value="Qwen/Qwen3.5-14B">Qwen3.5-14B (28GB fp16)</option>
              <option value="Qwen/Qwen3.5-27B">Qwen3.5-27B (54GB, 4-bit)</option>
              <option value="Qwen/Qwen3.5-35B-A3B">Qwen3.5-35B-A3B MoE (49GB)</option>
            </select>
          </div>
          <div class="control-group">
            <span class="control-label">Domain</span>
            <select class="control-select"
              .value=${this._domain}
              @change=${(e: Event) => this._domain = (e.target as HTMLSelectElement).value}>
              <option value="code">Code</option>
              <option value="reasoning">Reasoning</option>
              <option value="general">General</option>
              <option value="chat">Chat</option>
            </select>
          </div>
          ${this._isMoe ? html`
          <div class="control-group">
            <span class="control-label">Experts (MoE)</span>
            <div class="slider-row">
              <input type="range" min="16" max="128" step="16"
                .value=${String(this._experts || 64)}
                @input=${(e: Event) => this._experts = parseInt((e.target as HTMLInputElement).value)}>
              <span class="slider-value">${this._experts || 64}</span>
            </div>
          </div>
          ` : nothing}
          <div class="control-group">
            <span class="control-label">Pruning Level</span>
            <div class="slider-row">
              <input type="range" min="0" max="70" step="5"
                .value=${String(this._pruneLevel)}
                @input=${(e: Event) => this._pruneLevel = parseInt((e.target as HTMLInputElement).value)}>
              <span class="slider-value">${this._pruneLevel}%</span>
            </div>
          </div>
          <div class="control-group">
            <span class="control-label">Prune Strategy</span>
            <select class="control-select"
              .value=${this._pruneStrategy}
              @change=${(e: Event) => this._pruneStrategy = (e.target as HTMLSelectElement).value as 'entropy' | 'gradient' | 'combined'}>
              <option value="entropy">Entropy (recommended)</option>
              <option value="gradient">Gradient</option>
              <option value="combined">Combined</option>
            </select>
          </div>
          <div class="control-group">
            <span class="control-label">Forge Cycles</span>
            <div class="slider-row">
              <input type="range" min="1" max="10" step="1"
                .value=${String(this._cycles)}
                @input=${(e: Event) => this._cycles = parseInt((e.target as HTMLInputElement).value)}>
              <span class="slider-value">${this._cycles}</span>
            </div>
          </div>
          <div class="control-group">
            <span class="control-label">Training Steps</span>
            <div class="slider-row">
              <input type="range" min="100" max="5000" step="100"
                .value=${String(this._steps)}
                @input=${(e: Event) => this._steps = parseInt((e.target as HTMLInputElement).value)}>
              <span class="slider-value">${this._steps}</span>
            </div>
          </div>
          <div class="control-group">
            <span class="control-label">Learning Rate</span>
            <select class="control-select"
              .value=${this._learningRate}
              @change=${(e: Event) => this._learningRate = (e.target as HTMLSelectElement).value}>
              <option value="1e-5">1e-5 (very slow)</option>
              <option value="5e-5">5e-5</option>
              <option value="1e-4">1e-4 (conservative)</option>
              <option value="2e-4">2e-4 (balanced)</option>
              <option value="5e-4">5e-4 (aggressive)</option>
              <option value="1e-3">1e-3 (YOLO)</option>
            </select>
          </div>
          <div class="control-group">
            <span class="control-label">Profile</span>
            <div class="profile-row">
              ${Object.entries(FORGE_PROFILES).map(([key, p]) => html`
                <button class="profile-btn" title=${p.risk}
                  @click=${() => this.applyProfile(key)}>${p.label}</button>
              `)}
            </div>
          </div>
        </div>
        <pipeline-composer></pipeline-composer>
      </div>
    `;
  }
}

if (!customElements.get('forge-controls-element')) {
  customElements.define('forge-controls-element', ForgeControlsElement);
}
