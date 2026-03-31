/**
 * ForgeDeltaElement — Delta-first forge UI
 *
 * Load a model → see what it IS → modify what you want → the diff IS the work.
 * No manual stage building. The pipeline emerges from the delta.
 *
 * Principles:
 * - Everything starts at "no change" — forge does NOTHING by default
 * - Edit a value → it highlights as a delta → a stage is auto-derived
 * - Reset any change → delta disappears → stage removed → cost drops
 * - The forge button shows delta count + estimated cost
 * - Chain of custody: source model → this forge (fork)
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

interface ModelCapabilities {
  architecture: string;
  parameters: string;
  fp16SizeGb: number;
  q4SizeGb: number;
  layers: number;
  heads: number;
  kvHeads: number;
  hiddenSize: number;
  contextLength: number;
  modalities: string[];
  isMoE: boolean;
  totalExperts: number | null;
  hasRoPE: boolean;
}

interface DeltaField {
  key: string;
  label: string;
  current: unknown;
  target: unknown;
  changed: boolean;
  stageType: string;  // What stage this delta produces
  costMinutes: number;
}

export class ForgeDeltaElement extends ReactiveWidget {

  // ── Source (introspected, read-only) ───────────────────────
  @reactive() private _modelId = '';
  @reactive() private _capabilities: ModelCapabilities | null = null;
  @reactive() private _loading = false;
  @reactive() private _loaded = false;

  // ── Target (user modifies these) ──────────────────────────
  @reactive() private _targetDomain = '';
  @reactive() private _targetContextLength = 0;
  @reactive() private _targetModalities: string[] = [];
  @reactive() private _targetPruneRatio = 0;
  @reactive() private _targetExperts = 0;
  @reactive() private _targetFormat = '';
  @reactive() private _targetQuantTypes: string[] = [];
  @reactive() private _targetDevices: string[] = [];
  @reactive() private _targetBenchmarks: string[] = [];
  @reactive() private _targetDeploy = '';
  @reactive() private _targetPublish = false;
  @reactive() private _forging = false;

  // ── Derived ───────────────────────────────────────────────

  private get _deltas(): DeltaField[] {
    if (!this._capabilities) return [];
    const c = this._capabilities;
    const deltas: DeltaField[] = [];

    if (this._targetDomain && this._targetDomain !== 'general') {
      deltas.push({ key: 'domain', label: 'Domain', current: 'general', target: this._targetDomain, changed: true, stageType: 'train', costMinutes: this._trainCostMinutes });
    }
    if (this._targetContextLength > 0 && this._targetContextLength !== c.contextLength) {
      deltas.push({ key: 'context', label: 'Context', current: `${(c.contextLength/1024).toFixed(0)}K`, target: `${(this._targetContextLength/1024).toFixed(0)}K`, changed: true, stageType: 'context-extend', costMinutes: 1 });
    }
    const newMods = this._targetModalities.filter(m => !c.modalities.includes(m));
    if (newMods.length > 0) {
      deltas.push({ key: 'modalities', label: 'Modalities', current: c.modalities.join(', '), target: this._targetModalities.join(', '), changed: true, stageType: 'modality', costMinutes: newMods.length * 50 });
    }
    if (this._targetPruneRatio > 0) {
      deltas.push({ key: 'prune', label: 'Prune', current: 'none', target: `${(this._targetPruneRatio * 100).toFixed(0)}%`, changed: true, stageType: 'prune', costMinutes: 2 });
    }
    if (c.isMoE && this._targetExperts > 0 && this._targetExperts < (c.totalExperts ?? 0)) {
      deltas.push({ key: 'experts', label: 'Experts', current: `${c.totalExperts}`, target: `${this._targetExperts}`, changed: true, stageType: 'expert-prune', costMinutes: 5 });
    }
    if (this._targetFormat) {
      deltas.push({ key: 'format', label: 'Format', current: 'safetensors', target: `${this._targetFormat} ${this._targetQuantTypes.join('/')}`, changed: true, stageType: 'quant', costMinutes: 5 });
    }
    if (this._targetBenchmarks.length > 0) {
      deltas.push({ key: 'eval', label: 'Benchmarks', current: 'none', target: this._targetBenchmarks.join(', '), changed: true, stageType: 'eval', costMinutes: this._targetBenchmarks.length * 10 });
    }
    if (this._targetPublish) {
      deltas.push({ key: 'publish', label: 'Publish', current: 'no', target: 'HuggingFace', changed: true, stageType: 'publish', costMinutes: 2 });
    }
    if (this._targetDeploy) {
      deltas.push({ key: 'deploy', label: 'Deploy', current: 'nowhere', target: this._targetDeploy, changed: true, stageType: 'deploy', costMinutes: 1 });
    }

    return deltas;
  }

  private get _totalCostMinutes(): number {
    const base = this._deltas.reduce((sum, d) => sum + d.costMinutes, 0);
    // Training cost scales with cycles
    const hasTrain = this._deltas.some(d => d.stageType === 'train');
    const hasPrune = this._deltas.some(d => d.stageType === 'prune');
    const cycles = (hasTrain && hasPrune) ? 3 : 1;
    return base * (hasTrain ? cycles : 1);
  }

  private get _trainCostMinutes(): number {
    if (!this._capabilities) return 100;
    const gb = this._capabilities.fp16SizeGb;
    if (gb <= 4) return 100;
    if (gb <= 10) return 200;
    if (gb <= 20) return 500;
    if (gb <= 50) return 1000;
    return 2000;
  }

  private get _costLabel(): string {
    const m = this._totalCostMinutes;
    if (m === 0) return '';
    if (m < 60) return `~${m}m`;
    const h = Math.floor(m / 60);
    const r = m % 60;
    return r > 0 ? `~${h}h${r}m` : `~${h}h`;
  }

  // ── Actions ───────────────────────────────────────────────

  private async loadModel(): Promise<void> {
    if (!this._modelId) return;
    this._loading = true;
    try {
      const result = await this.executeCommand<any, any>('model/introspect', { model: this._modelId });
      if (result?.currentCapabilities) {
        this._capabilities = result.currentCapabilities;
        // Set targets to current values (no delta = no work)
        this._targetContextLength = result.currentCapabilities.contextLength ?? 0;
        this._targetModalities = [...(result.currentCapabilities.modalities ?? ['text'])];
        this._targetExperts = result.currentCapabilities.totalExperts ?? 0;
        this._loaded = true;
      }
    } catch (e) {
      console.error('Introspect failed:', e);
    }
    this._loading = false;
  }

  private resetAll(): void {
    if (!this._capabilities) return;
    this._targetDomain = '';
    this._targetContextLength = this._capabilities.contextLength ?? 0;
    this._targetModalities = [...(this._capabilities.modalities ?? ['text'])];
    this._targetPruneRatio = 0;
    this._targetExperts = this._capabilities.totalExperts ?? 0;
    this._targetFormat = '';
    this._targetQuantTypes = [];
    this._targetDevices = [];
    this._targetBenchmarks = [];
    this._targetDeploy = '';
    this._targetPublish = false;
  }

  private resetField(key: string): void {
    if (!this._capabilities) return;
    const c = this._capabilities;
    switch (key) {
      case 'domain': this._targetDomain = ''; break;
      case 'context': this._targetContextLength = c.contextLength ?? 0; break;
      case 'modalities': this._targetModalities = [...(c.modalities ?? ['text'])]; break;
      case 'prune': this._targetPruneRatio = 0; break;
      case 'experts': this._targetExperts = c.totalExperts ?? 0; break;
      case 'format': this._targetFormat = ''; this._targetQuantTypes = []; break;
      case 'eval': this._targetBenchmarks = []; break;
      case 'publish': this._targetPublish = false; break;
      case 'deploy': this._targetDeploy = ''; break;
    }
  }

  private async startForge(): Promise<void> {
    if (this._deltas.length === 0 || this._forging) return;
    this._forging = true;

    const target: Record<string, unknown> = {};
    if (this._targetDomain) target.domain = this._targetDomain;
    if (this._targetContextLength !== (this._capabilities?.contextLength ?? 0)) target.contextLength = this._targetContextLength;
    if (this._targetModalities.join(',') !== (this._capabilities?.modalities ?? []).join(',')) target.modalities = this._targetModalities;
    if (this._targetPruneRatio > 0) target.pruneRatio = this._targetPruneRatio;
    if (this._targetFormat) { target.outputFormats = [this._targetFormat]; target.quantTypes = this._targetQuantTypes; }
    if (this._targetBenchmarks.length > 0) target.benchmarks = this._targetBenchmarks;
    if (this._targetPublish) target.publish = true;
    if (this._targetDeploy) target.deployTo = this._targetDeploy;
    if (this._targetDevices.length > 0) target.targetDevices = this._targetDevices;

    this.dispatchEvent(new CustomEvent('forge-delta', {
      detail: { model: this._modelId, target },
      bubbles: true, composed: true,
    }));

    this._forging = false;
  }

  private exportAlloy(): void {
    const target: Record<string, unknown> = {};
    if (this._targetDomain) target.domain = this._targetDomain;
    if (this._targetPruneRatio > 0) target.pruneRatio = this._targetPruneRatio;
    if (this._targetFormat) { target.outputFormats = [this._targetFormat]; target.quantTypes = this._targetQuantTypes; }
    if (this._targetBenchmarks.length > 0) target.benchmarks = this._targetBenchmarks;
    if (this._targetPublish) target.publish = true;
    if (this._targetDeploy) target.deployTo = this._targetDeploy;

    const alloy = {
      name: `${this._modelId.split('/').pop()?.toLowerCase()}-${this._targetDomain || 'forged'}`,
      version: '1.0.0',
      source: { baseModel: this._modelId, architecture: this._capabilities?.architecture ?? 'unknown' },
      target,
      stages: this._deltas.map(d => ({ type: d.stageType })),
      cycles: this._deltas.some(d => d.stageType === 'train') ? 3 : 1,
    };

    const blob = new Blob([JSON.stringify(alloy, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${alloy.name}.alloy.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // ── Styles ────────────────────────────────────────────────

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host { display: block; }

      .delta-forge {
        background: var(--surface-elevated, rgba(255,255,255,0.04));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 8px;
        padding: 16px 20px;
      }

      .model-row {
        display: flex;
        gap: 8px;
        margin-bottom: 16px;
      }

      .model-input {
        flex: 1;
        background: rgba(0,0,0,0.3);
        border: 1px solid var(--border-color, rgba(255,255,255,0.12));
        border-radius: 4px;
        color: var(--content-primary, #e0e6ed);
        font-size: 13px;
        padding: 8px 10px;
        font-family: inherit;
        outline: none;
      }

      .model-input:focus { border-color: var(--accent-primary, #00d4ff); }

      .load-btn, .reset-btn {
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

      .load-btn:hover, .reset-btn:hover {
        background: rgba(0, 212, 255, 0.15);
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }

      .model-info {
        display: flex;
        gap: 12px;
        flex-wrap: wrap;
        margin-bottom: 16px;
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
      }

      .model-info b { color: var(--content-primary, #e0e6ed); }

      /* ── Delta Grid ─────────────────────── */

      .delta-grid {
        display: flex;
        flex-direction: column;
        gap: 6px;
        margin-bottom: 16px;
      }

      .delta-row {
        display: grid;
        grid-template-columns: 100px 1fr 20px;
        gap: 8px;
        align-items: center;
        padding: 8px 10px;
        border-radius: 6px;
        background: rgba(0,0,0,0.2);
        border: 1px solid transparent;
        transition: all 0.15s;
      }

      .delta-row.changed {
        border-color: rgba(0, 255, 200, 0.3);
        background: rgba(0, 255, 200, 0.03);
      }

      .delta-label {
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--content-secondary, #8a92a5);
      }

      .delta-row.changed .delta-label {
        color: #00ffc8;
      }

      .delta-control {
        display: flex;
        align-items: center;
        gap: 6px;
      }

      .delta-select, .delta-input {
        flex: 1;
        background: rgba(0,0,0,0.3);
        border: 1px solid var(--border-color, rgba(255,255,255,0.1));
        border-radius: 3px;
        color: var(--content-primary, #e0e6ed);
        font-size: 12px;
        padding: 4px 6px;
        font-family: inherit;
        outline: none;
      }

      .delta-select option { background: #0a1520; color: #e0e6ed; }

      .delta-reset {
        width: 18px;
        height: 18px;
        border: none;
        border-radius: 50%;
        background: transparent;
        color: var(--content-tertiary, #5a6070);
        font-size: 11px;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        transition: all 0.15s;
        visibility: hidden;
      }

      .delta-row.changed .delta-reset {
        visibility: visible;
      }

      .delta-reset:hover {
        background: rgba(255, 68, 68, 0.15);
        color: #ff4444;
      }

      .toggle-chips {
        display: flex;
        gap: 3px;
        flex-wrap: wrap;
      }

      .chip {
        font-size: 9px;
        padding: 2px 6px;
        border-radius: 3px;
        border: 1px solid var(--border-color, rgba(255,255,255,0.1));
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        transition: all 0.15s;
      }

      .chip.active {
        background: rgba(0, 212, 255, 0.12);
        border-color: rgba(0, 212, 255, 0.4);
        color: #00d4ff;
      }

      .chip.new {
        background: rgba(0, 255, 200, 0.12);
        border-color: rgba(0, 255, 200, 0.4);
        color: #00ffc8;
      }

      .slider-row {
        display: flex;
        align-items: center;
        gap: 6px;
        flex: 1;
      }

      .slider-row input[type="range"] {
        flex: 1;
        accent-color: var(--accent-primary, #00d4ff);
        height: 3px;
      }

      .slider-val {
        font-size: 11px;
        font-weight: 600;
        font-variant-numeric: tabular-nums;
        min-width: 35px;
        text-align: right;
        color: var(--accent-primary, #00d4ff);
      }

      /* ── Derived Pipeline ───────────────── */

      .pipeline-summary {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 12px;
        padding: 6px 8px;
        background: rgba(0,0,0,0.2);
        border-radius: 4px;
      }

      .pipeline-stages {
        display: flex;
        gap: 4px;
        flex-wrap: wrap;
      }

      .pipeline-stage {
        font-size: 9px;
        font-weight: 700;
        padding: 2px 6px;
        border-radius: 3px;
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }

      /* ── Forge Button ───────────────────── */

      .button-row {
        display: flex;
        gap: 8px;
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
      }

      .forge-button .estimate {
        position: absolute;
        top: 3px;
        right: 8px;
        font-size: 9px;
        font-weight: 500;
        color: rgba(255,255,255,0.4);
      }

      .export-btn {
        padding: 6px 14px;
        font-size: 11px;
        font-weight: 600;
        border: 1px solid var(--border-color, rgba(255,255,255,0.15));
        border-radius: 4px;
        background: rgba(255,255,255,0.05);
        color: var(--content-primary, #e0e6ed);
        cursor: pointer;
      }

      .no-delta {
        text-align: center;
        font-size: 12px;
        color: var(--content-tertiary, #5a6070);
        padding: 8px;
      }
    `,
  ];

  // ── Render ────────────────────────────────────────────────

  protected override render(): TemplateResult {
    return html`
      <div class="delta-forge">
        ${this.renderModelSelector()}
        ${this._loaded ? this.renderModelInfo() : nothing}
        ${this._loaded ? this.renderDeltaGrid() : nothing}
        ${this._loaded ? this.renderPipelineSummary() : nothing}
        ${this._loaded ? this.renderForgeButton() : nothing}
      </div>
    `;
  }

  private renderModelSelector(): TemplateResult {
    return html`
      <div class="model-row">
        <input class="model-input" type="text"
          placeholder="HuggingFace model ID (e.g., Qwen/Qwen3.5-4B)"
          .value=${this._modelId}
          @input=${(e: Event) => this._modelId = (e.target as HTMLInputElement).value}
          @keydown=${(e: KeyboardEvent) => { if (e.key === 'Enter') this.loadModel(); }}>
        <button class="load-btn" @click=${this.loadModel}
          ?disabled=${this._loading}>
          ${this._loading ? 'Loading...' : 'Load'}
        </button>
        ${this._loaded ? html`<button class="reset-btn" @click=${this.resetAll}>Reset All</button>` : nothing}
      </div>
    `;
  }

  private renderModelInfo(): TemplateResult {
    const c = this._capabilities!;
    return html`
      <div class="model-info">
        <span><b>${c.parameters}</b> params</span>
        <span><b>${c.layers}</b> layers</span>
        <span><b>${c.heads}</b> heads</span>
        <span><b>${(c.contextLength / 1024).toFixed(0)}K</b> context</span>
        <span><b>${c.modalities.join('+')}</b></span>
        <span><b>${c.fp16SizeGb}GB</b> fp16</span>
        ${c.isMoE ? html`<span><b>${c.totalExperts}</b> experts</span>` : nothing}
      </div>
    `;
  }

  private renderDeltaGrid(): TemplateResult {
    return html`
      <div class="delta-grid">
        ${this.renderDomainDelta()}
        ${this.renderPruneDelta()}
        ${this.renderModalityDelta()}
        ${this.renderFormatDelta()}
        ${this.renderBenchmarkDelta()}
        ${this.renderDeployDelta()}
        ${this.renderPublishDelta()}
      </div>
    `;
  }

  private renderDomainDelta(): TemplateResult {
    const changed = !!this._targetDomain;
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Domain</span>
        <div class="delta-control">
          <select class="delta-select"
            .value=${this._targetDomain}
            @change=${(e: Event) => this._targetDomain = (e.target as HTMLSelectElement).value}>
            <option value="">— no change —</option>
            <option value="code">Code</option>
            <option value="reasoning">Reasoning</option>
            <option value="chat">Chat</option>
            <option value="science">Science</option>
          </select>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('domain')}>&#10005;</button>
      </div>
    `;
  }

  private renderPruneDelta(): TemplateResult {
    const changed = this._targetPruneRatio > 0;
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Prune</span>
        <div class="delta-control">
          <div class="slider-row">
            <input type="range" min="0" max="70" step="5"
              .value=${String(Math.round(this._targetPruneRatio * 100))}
              @input=${(e: Event) => this._targetPruneRatio = parseInt((e.target as HTMLInputElement).value) / 100}>
            <span class="slider-val">${this._targetPruneRatio > 0 ? `${(this._targetPruneRatio * 100).toFixed(0)}%` : '—'}</span>
          </div>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('prune')}>&#10005;</button>
      </div>
    `;
  }

  private renderModalityDelta(): TemplateResult {
    const currentMods = new Set(this._capabilities?.modalities ?? ['text']);
    const allMods = ['text', 'vision', 'audio', 'video'];
    const changed = this._targetModalities.some(m => !currentMods.has(m));

    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Modalities</span>
        <div class="delta-control">
          <div class="toggle-chips">
            ${allMods.map(m => {
              const isCurrent = currentMods.has(m);
              const isTarget = this._targetModalities.includes(m);
              const isNew = isTarget && !isCurrent;
              return html`
                <button class="chip ${isTarget ? (isNew ? 'new' : 'active') : ''}"
                  @click=${() => {
                    if (isCurrent) return; // Can't remove existing modalities
                    if (isTarget) {
                      this._targetModalities = this._targetModalities.filter(x => x !== m);
                    } else {
                      this._targetModalities = [...this._targetModalities, m];
                    }
                  }}>${m}${isNew ? ' +' : ''}</button>
              `;
            })}
          </div>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('modalities')}>&#10005;</button>
      </div>
    `;
  }

  private renderFormatDelta(): TemplateResult {
    const changed = !!this._targetFormat;
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Output</span>
        <div class="delta-control">
          <select class="delta-select"
            .value=${this._targetFormat}
            @change=${(e: Event) => {
              this._targetFormat = (e.target as HTMLSelectElement).value;
              if (this._targetFormat === 'gguf' && this._targetQuantTypes.length === 0) {
                this._targetQuantTypes = ['Q4_K_M'];
              }
            }}>
            <option value="">— no change —</option>
            <option value="gguf">GGUF (llama.cpp)</option>
            <option value="mlx">MLX (Apple Silicon)</option>
            <option value="onnx">ONNX (cross-platform)</option>
          </select>
          ${this._targetFormat === 'gguf' ? html`
            <div class="toggle-chips">
              ${['Q4_K_M', 'Q8_0', 'Q5_K_M', 'Q3_K_M'].map(q => html`
                <button class="chip ${this._targetQuantTypes.includes(q) ? 'active' : ''}"
                  @click=${() => {
                    if (this._targetQuantTypes.includes(q)) {
                      this._targetQuantTypes = this._targetQuantTypes.filter(x => x !== q);
                    } else {
                      this._targetQuantTypes = [...this._targetQuantTypes, q];
                    }
                  }}>${q}</button>
              `)}
            </div>
          ` : nothing}
        </div>
        <button class="delta-reset" @click=${() => this.resetField('format')}>&#10005;</button>
      </div>
    `;
  }

  private renderBenchmarkDelta(): TemplateResult {
    const changed = this._targetBenchmarks.length > 0;
    const benchmarks = ['humaneval', 'mmlu', 'gsm8k', 'arc', 'hellaswag'];
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Benchmarks</span>
        <div class="delta-control">
          <div class="toggle-chips">
            ${benchmarks.map(b => html`
              <button class="chip ${this._targetBenchmarks.includes(b) ? 'active' : ''}"
                @click=${() => {
                  if (this._targetBenchmarks.includes(b)) {
                    this._targetBenchmarks = this._targetBenchmarks.filter(x => x !== b);
                  } else {
                    this._targetBenchmarks = [...this._targetBenchmarks, b];
                  }
                }}>${b}</button>
            `)}
          </div>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('eval')}>&#10005;</button>
      </div>
    `;
  }

  private renderDeployDelta(): TemplateResult {
    const changed = !!this._targetDeploy;
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Deploy</span>
        <div class="delta-control">
          <select class="delta-select"
            .value=${this._targetDeploy}
            @change=${(e: Event) => this._targetDeploy = (e.target as HTMLSelectElement).value}>
            <option value="">— no change —</option>
            <option value="bigmama">BigMama (RTX 5090)</option>
            <option value="local">Local</option>
            <option value="grid">Grid (auto-select)</option>
          </select>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('deploy')}>&#10005;</button>
      </div>
    `;
  }

  private renderPublishDelta(): TemplateResult {
    const changed = this._targetPublish;
    return html`
      <div class="delta-row ${changed ? 'changed' : ''}">
        <span class="delta-label">Publish</span>
        <div class="delta-control">
          <label style="font-size:11px;color:var(--content-secondary,#8a92a5);cursor:pointer">
            <input type="checkbox"
              .checked=${this._targetPublish}
              @change=${(e: Event) => this._targetPublish = (e.target as HTMLInputElement).checked}
              style="accent-color:#00d4ff">
            Publish to HuggingFace with alloy
          </label>
        </div>
        <button class="delta-reset" @click=${() => this.resetField('publish')}>&#10005;</button>
      </div>
    `;
  }

  private renderPipelineSummary(): TemplateResult {
    const deltas = this._deltas;
    if (deltas.length === 0) {
      return html`<div class="no-delta">No modifications — model stays as-is</div>`;
    }

    const stageColors: Record<string, string> = {
      'prune': '#ff6464', 'train': '#00d4ff', 'modality': '#64ffc8',
      'context-extend': '#c864ff', 'quant': '#00ffc8', 'eval': '#ffff64',
      'publish': '#64c8ff', 'deploy': '#64ffc8', 'expert-prune': '#ff9664',
    };

    return html`
      <div class="pipeline-summary">
        <div class="pipeline-stages">
          ${deltas.map(d => html`
            <span class="pipeline-stage"
              style="background:${stageColors[d.stageType] ?? '#888'}22;color:${stageColors[d.stageType] ?? '#888'}">${d.stageType}</span>
          `)}
        </div>
      </div>
    `;
  }

  private renderForgeButton(): TemplateResult {
    const deltaCount = this._deltas.length;
    const cost = this._costLabel;

    return html`
      <div class="button-row">
        <button class="forge-button"
          ?disabled=${deltaCount === 0 || this._forging}
          @click=${this.startForge}>
          ${deltaCount === 0 ? 'NO CHANGES' : `FORGE DELTA`}
          ${deltaCount > 0 ? html`<span class="estimate">${cost} · ${deltaCount} changes</span>` : nothing}
        </button>
        <button class="export-btn" @click=${this.exportAlloy}
          ?disabled=${deltaCount === 0}>Export Alloy</button>
      </div>
    `;
  }
}

if (!customElements.get('forge-delta-element')) {
  customElements.define('forge-delta-element', ForgeDeltaElement);
}
