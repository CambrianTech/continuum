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
  unsafeCSS,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { styles as FORGE_CONTROLS_STYLES } from './public/forge-controls.styles';
import './stages/PipelineComposer';
import './DeviceTargetElement';

/** Forge profiles — presets for common configurations */
const FORGE_PROFILES: Record<string, { prune: number; cycles: number; lr: string; steps: number; label: string; risk: string }> = {
  conservative: { prune: 10, cycles: 5, lr: '1e-4', steps: 2000, label: 'Conservative', risk: 'Low — safe improvement' },
  balanced:     { prune: 30, cycles: 3, lr: '2e-4', steps: 1000, label: 'Balanced', risk: 'Medium — best tradeoff' },
  aggressive:   { prune: 50, cycles: 2, lr: '5e-4', steps: 500,  label: 'Aggressive', risk: 'High — maximum compression' },
  yolo:         { prune: 70, cycles: 1, lr: '1e-3', steps: 250,  label: 'YOLO', risk: 'Extreme — might break the model' },
};

/** Mutagen — random pipeline mutations within safe bounds.
 *  Each axis that's been proven in a successful forge can be rolled. */
interface MutagenAxis {
  label: string;
  apply: (stages: Record<string, unknown>[]) => Record<string, unknown>[];
  weight: number; // probability weight (0-1)
}

const MUTAGEN_AXES: MutagenAxis[] = [
  {
    label: 'Context 128K',
    weight: 0.5,
    apply: (stages) => {
      const methods = ['yarn', 'ntk', 'linear', 'dynamic-ntk'] as const;
      const lengths = [65536, 131072, 262144];
      return [
        { type: 'context-extend', targetLength: lengths[Math.floor(Math.random() * lengths.length)], method: methods[Math.floor(Math.random() * methods.length)], trainingSteps: 100 + Math.floor(Math.random() * 400) },
        ...stages,
      ];
    },
  },
  {
    label: 'Add Vision',
    weight: 0.3,
    apply: (stages) => [
      { type: 'modality', modality: 'vision', encoderModel: 'openai/clip-vit-large-patch14', projectionArch: 'mlp', freezeBase: true, freezeEncoder: true, trainingSteps: 1000 },
      ...stages,
    ],
  },
  {
    label: 'Add Audio',
    weight: 0.2,
    apply: (stages) => [
      { type: 'modality', modality: 'audio', encoderModel: 'openai/whisper-large-v3', projectionArch: 'mlp', freezeBase: true, freezeEncoder: true, trainingSteps: 1000 },
      ...stages,
    ],
  },
  {
    label: 'LoRA Rank Boost',
    weight: 0.5,
    apply: (stages) => {
      const ranks = [16, 32, 64, 128];
      const r = ranks[Math.floor(Math.random() * ranks.length)];
      return [...stages, { type: 'lora', rank: r, alpha: r * 2, dropout: 0.05, targetModules: ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj'], quantize: true, quantizeBits: 4, epochs: 2, learningRate: '1e-4', mergeAfter: true }];
    },
  },
  {
    label: 'Compaction',
    weight: 0.4,
    apply: (stages) => [...stages, { type: 'compact', deadThreshold: 0.05 + Math.random() * 0.15, dormantThreshold: 0.15 + Math.random() * 0.15, lowThreshold: 0.25 + Math.random() * 0.15, mediumThreshold: 0.4 + Math.random() * 0.2, highThreshold: 0.6 + Math.random() * 0.2, enableQuantization: true }],
  },
  {
    label: 'Aggressive Prune',
    weight: 0.4,
    apply: (stages) => {
      const strategies = ['entropy', 'magnitude', 'gradient'] as const;
      return stages.map(s => s.type === 'prune'
        ? { ...s, level: 0.3 + Math.random() * 0.4, strategy: strategies[Math.floor(Math.random() * strategies.length)] }
        : s);
    },
  },
];

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
  @reactive() private _pipelineStages: Record<string, unknown>[] = [];
  @reactive() private _modelValid: 'unknown' | 'checking' | 'valid' | 'invalid' = 'unknown';
  private _validateTimer: ReturnType<typeof setTimeout> | null = null;

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

  /** Current settings as an alloy recipe — uses pipeline composer stages if configured */
  get alloyRecipe(): Record<string, unknown> {
    const base = this._model.split('/').pop()?.toLowerCase() ?? 'model';

    // Use pipeline composer stages if user configured them, otherwise default prune+train
    const stages = this._pipelineStages.length > 0
      ? this._pipelineStages
      : [
          { type: 'prune', strategy: this._pruneStrategy, level: this._pruneLevel / 100 },
          { type: 'train', domain: this._domain, steps: this._steps, learningRate: this._learningRate },
        ];

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
      stages,
      cycles: this._cycles,
    };
  }

  /** Estimate total forge time in minutes based on model + stages + cycles */
  private get _estimatedMinutes(): number {
    // Steps per minute by model size (rough, based on 5090 benchmarks)
    const model = this._model.toLowerCase();
    let stepsPerMin: number;
    if (model.includes('0.5b') || model.includes('0.8b')) stepsPerMin = 20;
    else if (model.includes('1.5b') || model.includes('3b') || model.includes('4b')) stepsPerMin = 10;
    else if (model.includes('7b') || model.includes('8b') || model.includes('9b')) stepsPerMin = 5;
    else if (model.includes('14b')) stepsPerMin = 2.5;
    else if (model.includes('27b') || model.includes('32b')) stepsPerMin = 1;
    else if (model.includes('35b')) stepsPerMin = 0.8;
    else stepsPerMin = 3;

    // Training time: steps * cycles / stepsPerMin
    const trainMin = (this._steps * this._cycles) / stepsPerMin;

    // Prune/eval overhead: ~2 min per cycle
    const pruneMin = this._cycles * 2;

    // Modality training (if present in pipeline — rough estimate)
    // Context extension is cheap (config change)
    // Quant/eval/publish are post-forge

    // Loading time (model download if not cached + load into VRAM)
    const loadMin = model.includes('27b') || model.includes('35b') ? 5 : 2;

    return Math.round(trainMin + pruneMin + loadMin);
  }

  private get _estimateLabel(): string {
    const min = this._estimatedMinutes;
    if (min < 60) return `~${min}m`;
    const h = Math.floor(min / 60);
    const m = min % 60;
    return m > 0 ? `~${h}h${m}m` : `~${h}h`;
  }

  /** Called by parent when user selects a model from the right panel browser */
  setBaseModel(modelId: string): void {
    this._model = modelId;
    this._modelValid = 'valid'; // Selected from our published models — known good
    this.requestUpdate();
  }

  /** Debounced HF model validation — checks if model exists on HuggingFace */
  private onModelInput(value: string): void {
    this._model = value;
    this._modelValid = 'unknown';

    if (this._validateTimer) clearTimeout(this._validateTimer);

    if (!value || !value.includes('/')) {
      this._modelValid = value ? 'invalid' : 'unknown';
      return;
    }

    this._modelValid = 'checking';
    this._validateTimer = setTimeout(() => this.validateModel(value), 600);
  }

  private async validateModel(modelId: string): Promise<void> {
    try {
      const response = await fetch(`https://huggingface.co/api/models/${modelId}`, { method: 'HEAD' });
      this._modelValid = response.ok ? 'valid' : 'invalid';
    } catch {
      this._modelValid = 'unknown'; // Network error — don't block
    }
    this.requestUpdate();
  }

  private applyProfile(name: string): void {
    const p = FORGE_PROFILES[name];
    if (!p) return;
    this._pruneLevel = p.prune;
    this._cycles = p.cycles;
    this._learningRate = p.lr;
    this._steps = p.steps;
  }

  /** Mutagen: roll random mutations onto the current pipeline.
   *  Each axis rolls independently based on its weight. At least one fires. */
  private mutate(): void {
    // Start from default pipeline
    let stages: Record<string, unknown>[] = [
      { type: 'source-config', contextLength: 4096, inputModalities: ['text'], targetDevices: [] },
      { type: 'prune', strategy: 'entropy', level: 0.3, minHeadsPerLayer: 4, minKvHeadsPerLayer: 2, analysisSteps: 200 },
      { type: 'train', domain: this._domain, steps: 500 + Math.floor(Math.random() * 1500), learningRate: '2e-4', batchSize: 2, gradientAccumulation: 4, scheduler: 'cosine', precision: 'bf16', sequenceLength: 2048, optimizations: ['flash_attention', 'gradient_checkpointing'] },
      { type: 'quant', format: 'gguf', quantTypes: ['Q4_K_M', 'Q8_0'], deviceTargets: ['MacBook Air 16GB', 'MacBook Pro 16GB'] },
      { type: 'eval', benchmarks: [{ name: 'humaneval', submitToLeaderboard: true }], passingThreshold: 40, compareToBase: true },
      { type: 'publish', org: 'continuum-ai', repoNameTemplate: '{base}-{domain}-mutant', includeAlloy: true, cardFromBenchmarks: true, tags: ['continuum', 'forged', 'mutagen'], private: false },
    ];

    // Roll mutations — each axis fires independently
    let anyFired = false;
    for (const axis of MUTAGEN_AXES) {
      if (Math.random() < axis.weight) {
        stages = axis.apply(stages);
        anyFired = true;
      }
    }

    // Guarantee at least one mutation
    if (!anyFired) {
      const axis = MUTAGEN_AXES[Math.floor(Math.random() * MUTAGEN_AXES.length)];
      stages = axis.apply(stages);
    }

    // Randomize cycles
    this._cycles = 1 + Math.floor(Math.random() * 4);

    this._pipelineStages = stages;
    this.dispatchEvent(new CustomEvent('mutagen-roll', {
      detail: { stages, cycles: this._cycles },
      bubbles: true,
      composed: true,
    }));
  }

  private onStartForge(): void {
    this.dispatchEvent(new CustomEvent('forge-start', {
      detail: { params: this.forgeParams, alloy: this.alloyRecipe },
      bubbles: true,
      composed: true,
    }));
  }

  private onExportAlloy(): void {
    this.dispatchEvent(new CustomEvent('forge-export', { detail: this.alloyRecipe, bubbles: true, composed: true }));
  }

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    unsafeCSS(FORGE_CONTROLS_STYLES),
  ];

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
            ${!this.forging ? html`<span class="forge-estimate">${this._estimateLabel}</span>` : nothing}
          </button>
          <button class="export-btn" @click=${this.onExportAlloy}
            title="Export current settings as .alloy.json recipe">Export Alloy</button>
        </div>
        <div class="controls-grid">
          <div class="control-group">
            <span class="control-label">Base Model
              ${this._modelValid === 'valid' ? html`<span class="model-status valid">&#10003;</span>` :
                this._modelValid === 'invalid' ? html`<span class="model-status invalid">&#10007;</span>` :
                this._modelValid === 'checking' ? html`<span class="model-status checking">...</span>` : nothing}
            </span>
            <input class="control-select ${this._modelValid === 'valid' ? 'validated' : this._modelValid === 'invalid' ? 'invalid-input' : ''}"
              type="text" list="model-list"
              placeholder="org/model-name (e.g. Qwen/Qwen3.5-4B)"
              .value=${this._model}
              @input=${(e: Event) => this.onModelInput((e.target as HTMLInputElement).value)}>
            <datalist id="model-list">
              <option value="Qwen/Qwen3.5-4B">Qwen3.5-4B (8GB fp16)</option>
              <option value="Qwen/Qwen3.5-14B">Qwen3.5-14B (28GB fp16)</option>
              <option value="Qwen/Qwen3.5-27B">Qwen3.5-27B (54GB, 4-bit)</option>
              <option value="Qwen/Qwen3.5-35B-A3B">Qwen3.5-35B-A3B MoE (49GB)</option>
              <option value="meta-llama/Llama-3.1-8B">Llama 3.1 8B</option>
              <option value="meta-llama/Llama-3.1-70B">Llama 3.1 70B</option>
              <option value="mistralai/Mistral-7B-v0.3">Mistral 7B v0.3</option>
              <option value="google/gemma-2-9b">Gemma 2 9B</option>
              <option value="microsoft/phi-3-mini-4k-instruct">Phi 3 Mini 4K</option>
            </datalist>
            <span class="control-hint">Any HuggingFace model — validates on typing</span>
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
              <button class="profile-btn mutagen-btn" title="Roll random mutations: context, vision, audio, LoRA, compaction..."
                @click=${this.mutate}>MUTAGEN</button>
            </div>
          </div>
        </div>
        <pipeline-composer
          @pipeline-change=${(e: CustomEvent) => this._pipelineStages = e.detail as Record<string, unknown>[]}
        ></pipeline-composer>
        <device-target-element></device-target-element>
      </div>
    `;
  }
}

if (!customElements.get('forge-controls-element')) {
  customElements.define('forge-controls-element', ForgeControlsElement);
}
