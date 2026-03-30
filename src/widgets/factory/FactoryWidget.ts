/**
 * FactoryWidget — Model forge production floor
 *
 * Shows:
 * - Active forges with progress, loss curves, VRAM gauges
 * - Published models with download counts and improvement scores
 * - Hardware resources across grid nodes
 *
 * Data sources:
 * - status.json from forge nodes (polled)
 * - HuggingFace API for published model stats
 * - Grid node health from grid commands
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
import { Events } from '../../system/core/shared/Events';
import type { ModelListPublishedResult } from '../../commands/model/list-published/shared/ModelList-publishedTypes';
import type { ForgeJobStatus } from '../../commands/model/forge-status/shared/ModelForge-statusTypes';
// ── Types ───────────────────────────────────────────────────────────────

interface ForgeStatus {
  phase: string;
  detail: string;
  vram_gb: number;
  timestamp: string;
  step?: number;
  total_steps?: number;
  loss?: number;
  it_per_sec?: number;
  eta_seconds?: number;
  cycle?: number;
  perplexity?: number;
  improvement_pct?: number;
}

interface PublishedModel {
  id: string;
  name: string;
  downloads: number;
  likes: number;
  domain: string;
  improvement: string;
  size: string;
  tags: string[];
}

interface GridNode {
  name: string;
  ip: string;
  gpu: string;
  vram_total_gb: number;
  vram_used_gb: number;
  status: 'forging' | 'idle' | 'offline';
}

interface ForgeSample {
  step: number;
  prompt: string;
  output: string;
  timestamp: string;
}

// ── Component ───────────────────────────────────────────────────────────

export class FactoryWidget extends ReactiveWidget {

  @reactive() private forgeStatus: ForgeStatus | null = null;
  @reactive() private models: PublishedModel[] = [];
  @reactive() private nodes: GridNode[] = [];
  @reactive() private lossHistory: number[] = [];
  @reactive() private outputSamples: ForgeSample[] = [];
  @reactive() private _isLoading = true;

  // ── Forge Controls State ────────────────────────────────────────────
  @reactive() private _selectedModel = 'Qwen/Qwen3.5-4B';
  @reactive() private _selectedDomain = 'code';
  @reactive() private _selectedExperts = 0;  // 0 = dense (no expert pruning)
  @reactive() private _selectedSteps = 2000;
  @reactive() private _selectedPruneLevel = 30;  // % of heads to prune
  @reactive() private _selectedPruneStrategy: 'entropy' | 'gradient' | 'combined' = 'entropy';
  @reactive() private _selectedCycles = 3;
  @reactive() private _selectedLearningRate = '2e-4';
  @reactive() private _forgeStarting = false;

  /** Forge profiles — presets for common configurations */
  private static readonly FORGE_PROFILES: Record<string, { prune: number; cycles: number; lr: string; steps: number; label: string; risk: string }> = {
    conservative: { prune: 10, cycles: 5, lr: '1e-4', steps: 2000, label: 'Conservative', risk: 'Low — safe improvement' },
    balanced:     { prune: 30, cycles: 3, lr: '2e-4', steps: 1000, label: 'Balanced', risk: 'Medium — best tradeoff' },
    aggressive:   { prune: 50, cycles: 2, lr: '5e-4', steps: 500,  label: 'Aggressive', risk: 'High — maximum compression' },
    yolo:         { prune: 70, cycles: 1, lr: '1e-3', steps: 250,  label: 'YOLO', risk: 'Extreme — might break the model' },
  };

  private applyProfile(profileName: string): void {
    const profile = FactoryWidget.FORGE_PROFILES[profileName];
    if (!profile) return;
    this._selectedPruneLevel = profile.prune;
    this._selectedCycles = profile.cycles;
    this._selectedLearningRate = profile.lr;
    this._selectedSteps = profile.steps;
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────

  override connectedCallback(): void {
    super.connectedCallback();
    this.subscribeToForgeEvents();
    this.loadPublishedModels();
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
  }

  // ── Event Subscriptions (no polling) ──────────────────────────────────

  private subscribeToForgeEvents(): void {
    // Subscribe to forge lifecycle events — emitted by the forge daemon/grid node
    Events.subscribe('model:forge:step', (data: any) => {
      this.forgeStatus = {
        phase: 'training',
        detail: data.detail ?? '',
        vram_gb: data.vramGb ?? 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        step: data.step,
        total_steps: data.totalSteps,
        loss: data.loss,
        it_per_sec: data.itPerSec,
        eta_seconds: data.etaSeconds,
      };
      if (data.loss !== undefined) {
        this.lossHistory = [...this.lossHistory.slice(-50), data.loss];
      }
    });

    Events.subscribe('model:forge:phase', (data: any) => {
      this.forgeStatus = {
        ...this.forgeStatus,
        phase: data.phase,
        detail: data.detail ?? '',
        timestamp: data.timestamp ?? new Date().toISOString(),
      } as ForgeStatus;
    });

    Events.subscribe('model:forge:sample', (data: any) => {
      this.outputSamples = [...this.outputSamples, {
        step: data.step ?? 0,
        prompt: data.prompt ?? '',
        output: data.output ?? '',
        timestamp: data.timestamp ?? new Date().toISOString(),
      }];
    });

    Events.subscribe('model:forge:complete', (data: any) => {
      this.forgeStatus = {
        phase: 'complete',
        detail: data.detail ?? 'Forge complete',
        vram_gb: 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        improvement_pct: data.improvementPct,
        perplexity: data.perplexity,
      };
      // Refresh published models after a forge completes
      this.loadPublishedModels();
    });
  }

  // ── Data Loading (one-shot, not polling) ──────────────────────────────

  private async loadPublishedModels(): Promise<void> {
    this._isLoading = true;
    try {
      const result = await this.executeCommand<any, any>('model/list-published', {});
      if (result?.models) {
        this.models = result.models;
      }
    } catch {
      // Command not available yet
      this.models = [];
    }
    this._isLoading = false;
  }

  // ── Rendering ─────────────────────────────────────────────────────────

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
      }

      .factory {
        padding: 20px 24px;
        max-width: 1200px;
      }

      .header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: 24px;
      }

      .title {
        font-size: 20px;
        font-weight: 700;
      }

      .subtitle {
        font-size: 12px;
        color: var(--content-secondary, #8a92a5);
      }

      /* ── Section Layout ──────────────────────────────── */

      .section {
        margin-bottom: 28px;
      }

      .section-title {
        font-size: 14px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 12px;
        padding-bottom: 6px;
        border-bottom: 1px solid var(--border-color, rgba(255,255,255,0.08));
      }

      /* ── Active Forge Card ───────────────────────────── */

      .forge-card {
        background: var(--surface-elevated, rgba(255,255,255,0.04));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 8px;
        padding: 16px 20px;
      }

      .forge-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 12px;
      }

      .forge-phase {
        font-size: 13px;
        font-weight: 600;
        color: var(--accent-primary, #00d4ff);
      }

      .forge-detail {
        font-size: 12px;
        color: var(--content-secondary, #8a92a5);
      }

      .forge-metrics {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
        gap: 12px;
        margin-top: 12px;
      }

      .metric {
        text-align: center;
      }

      .metric-value {
        font-size: 24px;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
      }

      .metric-label {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.04em;
      }

      .metric-value.good { color: #00ffc8; }
      .metric-value.warn { color: #ffaa00; }
      .metric-value.bad { color: #ff4444; }
      .metric-value.neutral { color: var(--content-primary, #e0e6ed); }

      /* ── Progress Bar ────────────────────────────────── */

      .progress-bar {
        height: 4px;
        background: rgba(255,255,255,0.06);
        border-radius: 2px;
        overflow: hidden;
        margin-top: 12px;
      }

      .progress-fill {
        height: 100%;
        background: linear-gradient(90deg, #00d4ff, #00ffc8);
        border-radius: 2px;
        transition: width 0.3s ease;
      }

      /* ── Model Grid ──────────────────────────────────── */

      .model-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 12px;
      }

      .model-card {
        background: var(--surface-elevated, rgba(255,255,255,0.04));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 8px;
        padding: 14px 16px;
        transition: border-color 0.2s;
      }

      .model-card:hover {
        border-color: var(--accent-primary, #00d4ff);
      }

      .model-name {
        font-size: 13px;
        font-weight: 600;
        margin-bottom: 4px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .model-meta {
        display: flex;
        gap: 12px;
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 8px;
      }

      .model-improvement {
        font-size: 16px;
        font-weight: 700;
        color: #00ffc8;
      }

      .model-improvement.negative {
        color: #ff6666;
      }

      .model-tags {
        display: flex;
        flex-wrap: wrap;
        gap: 4px;
        margin-top: 8px;
      }

      .tag {
        font-size: 10px;
        padding: 2px 6px;
        border-radius: 3px;
        background: rgba(0, 212, 255, 0.1);
        color: var(--accent-primary, #00d4ff);
      }

      /* ── Empty State ─────────────────────────────────── */

      .empty-state {
        text-align: center;
        padding: 60px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-state .icon {
        font-size: 48px;
        margin-bottom: 12px;
      }

      .empty-state .message {
        font-size: 14px;
        margin-bottom: 8px;
      }

      .empty-state .hint {
        font-size: 12px;
        opacity: 0.7;
      }

      /* ── Loss Sparkline ──────────────────────────────── */

      .sparkline {
        margin-top: 8px;
      }

      .sparkline svg {
        width: 100%;
        height: 40px;
      }

      .sparkline path {
        fill: none;
        stroke: #00ffc8;
        stroke-width: 1.5;
      }

      /* ── Forge Controls ────────────────────────────────── */

      .forge-controls {
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

      .control-select,
      .control-input {
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

      .control-select:focus,
      .control-input:focus {
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

      .forge-button {
        width: 100%;
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

      .forge-button:active {
        transform: scale(0.98);
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

      /* ── Output Log ──────────────────────────────────── */

      .output-log {
        display: flex;
        flex-direction: column;
        gap: 4px;
      }

      .log-entry {
        background: var(--surface-elevated, rgba(255,255,255,0.04));
        border: 1px solid var(--border-color, rgba(255,255,255,0.08));
        border-radius: 6px;
        overflow: hidden;
        transition: border-color 0.2s;
      }

      .log-entry:hover {
        border-color: rgba(255,255,255,0.15);
      }

      .log-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 8px 12px;
        cursor: pointer;
        user-select: none;
      }

      .log-header:hover {
        background: rgba(255,255,255,0.02);
      }

      .log-step {
        font-size: 11px;
        font-weight: 600;
        color: var(--accent-primary, #00d4ff);
        font-variant-numeric: tabular-nums;
      }

      .log-preview {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        flex: 1;
        margin: 0 12px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        font-family: 'SF Mono', 'Fira Code', monospace;
      }

      .log-time {
        font-size: 10px;
        color: var(--content-tertiary, #5a6070);
        white-space: nowrap;
      }

      .log-expand {
        font-size: 10px;
        color: var(--content-tertiary, #5a6070);
        margin-left: 8px;
      }

      .log-body {
        padding: 0 12px 12px;
        border-top: 1px solid var(--border-color, rgba(255,255,255,0.06));
      }

      .log-prompt {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 6px;
        font-style: italic;
      }

      .log-output {
        font-size: 12px;
        font-family: 'SF Mono', 'Fira Code', monospace;
        line-height: 1.5;
        white-space: pre-wrap;
        word-break: break-word;
        color: var(--content-primary, #e0e6ed);
        background: rgba(0,0,0,0.2);
        padding: 10px;
        border-radius: 4px;
        max-height: 400px;
        overflow-y: auto;
      }
    `,
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="factory">
        <div class="header">
          <span class="title">Model Factory</span>
          <span class="subtitle">continuum-ai</span>
        </div>

        ${this.renderForgeControls()}
        ${this.renderActiveForge()}
        ${this.renderOutputLog()}
        ${this.renderPublishedModels()}
      </div>
    `;
  }

  // ── Forge Controls ─────────────────────────────────────────────────────

  private get _isMoe(): boolean {
    return this._selectedModel.includes('35B') || this._selectedModel.includes('MoE');
  }

  private get _isForging(): boolean {
    return this.forgeStatus?.phase === 'training' || this.forgeStatus?.phase === 'loading';
  }

  private renderForgeControls(): TemplateResult {
    return html`
      <div class="section">
        <div class="section-title">Forge</div>
        <div class="forge-controls">
          <div class="controls-grid">
            <div class="control-group">
              <span class="control-label">Base Model</span>
              <select class="control-select"
                .value=${this._selectedModel}
                @change=${(e: Event) => this._selectedModel = (e.target as HTMLSelectElement).value}>
                <option value="Qwen/Qwen3.5-4B">Qwen3.5-4B (8GB fp16)</option>
                <option value="Qwen/Qwen3.5-14B">Qwen3.5-14B (28GB fp16)</option>
                <option value="Qwen/Qwen3.5-27B">Qwen3.5-27B (54GB, 4-bit)</option>
                <option value="Qwen/Qwen3.5-35B-A3B">Qwen3.5-35B-A3B MoE (49GB)</option>
              </select>
            </div>
            <div class="control-group">
              <span class="control-label">Domain</span>
              <select class="control-select"
                .value=${this._selectedDomain}
                @change=${(e: Event) => this._selectedDomain = (e.target as HTMLSelectElement).value}>
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
                  .value=${String(this._selectedExperts || 64)}
                  @input=${(e: Event) => this._selectedExperts = parseInt((e.target as HTMLInputElement).value)}>
                <span class="slider-value">${this._selectedExperts || 64}</span>
              </div>
            </div>
            ` : nothing}
            <div class="control-group">
              <span class="control-label">Pruning Level</span>
              <div class="slider-row">
                <input type="range" min="0" max="70" step="5"
                  .value=${String(this._selectedPruneLevel)}
                  @input=${(e: Event) => this._selectedPruneLevel = parseInt((e.target as HTMLInputElement).value)}>
                <span class="slider-value">${this._selectedPruneLevel}%${this._selectedPruneLevel > 50 ? ' ⚠️' : ''}</span>
              </div>
            </div>
            <div class="control-group">
              <span class="control-label">Prune Strategy</span>
              <select class="control-select"
                .value=${this._selectedPruneStrategy}
                @change=${(e: Event) => this._selectedPruneStrategy = (e.target as HTMLSelectElement).value as 'entropy' | 'gradient' | 'combined'}>
                <option value="entropy">Entropy (recommended)</option>
                <option value="gradient">Gradient</option>
                <option value="combined">Combined</option>
              </select>
            </div>
            <div class="control-group">
              <span class="control-label">Forge Cycles</span>
              <div class="slider-row">
                <input type="range" min="1" max="10" step="1"
                  .value=${String(this._selectedCycles)}
                  @input=${(e: Event) => this._selectedCycles = parseInt((e.target as HTMLInputElement).value)}>
                <span class="slider-value">${this._selectedCycles}</span>
              </div>
            </div>
            <div class="control-group">
              <span class="control-label">Training Steps</span>
              <div class="slider-row">
                <input type="range" min="100" max="5000" step="100"
                  .value=${String(this._selectedSteps)}
                  @input=${(e: Event) => this._selectedSteps = parseInt((e.target as HTMLInputElement).value)}>
                <span class="slider-value">${this._selectedSteps}</span>
              </div>
            </div>
            <div class="control-group">
              <span class="control-label">Learning Rate</span>
              <select class="control-select"
                .value=${this._selectedLearningRate}
                @change=${(e: Event) => this._selectedLearningRate = (e.target as HTMLSelectElement).value}>
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
              <div style="display:flex;gap:6px;flex-wrap:wrap">
                ${Object.entries(FactoryWidget.FORGE_PROFILES).map(([key, p]) => html`
                  <button class="profile-btn" title=${p.risk}
                    @click=${() => this.applyProfile(key)}>${p.label}</button>
                `)}
              </div>
            </div>
          </div>
          <button class="forge-button ${this._isForging ? 'forging' : ''}"
            ?disabled=${this._forgeStarting}
            @click=${this.startForge}>
            ${this._isForging ? 'FORGING...' : this._forgeStarting ? 'STARTING...' : 'START FORGE'}
          </button>
        </div>
      </div>
    `;
  }

  private async startForge(): Promise<void> {
    if (this._isForging || this._forgeStarting) return;
    this._forgeStarting = true;

    try {
      // Fire forge command — routes to a grid node with GPU
      await this.executeCommand<any, any>('model/forge', {
        model: this._selectedModel,
        domain: this._selectedDomain,
        experts: this._isMoe ? (this._selectedExperts || 64) : 0,
        steps: this._selectedSteps,
        pruneLevel: this._selectedPruneLevel / 100,  // 0.0-0.7
        pruneStrategy: this._selectedPruneStrategy,
        cycles: this._selectedCycles,
        learningRate: this._selectedLearningRate,
      });
    } catch (e) {
      console.error('Forge start failed:', e);
      // TODO: show error in UI
    } finally {
      this._forgeStarting = false;
    }
  }

  // ── Active Forge Section ──────────────────────────────────────────────

  private renderActiveForge(): TemplateResult {
    const status = this.forgeStatus;

    return html`
      <div class="section">
        <div class="section-title">Active Forge</div>
        ${status ? this.renderForgeCard(status) : this.renderNoForge()}
      </div>
    `;
  }

  private renderForgeCard(s: ForgeStatus): TemplateResult {
    const progress = s.step && s.total_steps ? (s.step / s.total_steps) * 100 : 0;
    const eta = s.eta_seconds ? this.formatETA(s.eta_seconds) : '--';
    const lossClass = s.loss !== undefined ? (s.loss < 2.5 ? 'good' : s.loss < 3.0 ? 'warn' : 'neutral') : 'neutral';

    return html`
      <div class="forge-card">
        <div class="forge-header">
          <span class="forge-phase">${s.phase.toUpperCase()}</span>
          <span class="forge-detail">${s.detail}</span>
        </div>

        <div class="forge-metrics">
          <div class="metric">
            <div class="metric-value ${lossClass}">${s.loss?.toFixed(2) ?? '--'}</div>
            <div class="metric-label">Loss</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${s.step ?? '--'}/${s.total_steps ?? '--'}</div>
            <div class="metric-label">Step</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${eta}</div>
            <div class="metric-label">ETA</div>
          </div>
          <div class="metric">
            <div class="metric-value ${s.vram_gb > 28 ? 'warn' : 'neutral'}">${s.vram_gb?.toFixed(1) ?? '--'}GB</div>
            <div class="metric-label">VRAM</div>
          </div>
          <div class="metric">
            <div class="metric-value neutral">${s.it_per_sec?.toFixed(1) ?? '--'}</div>
            <div class="metric-label">it/s</div>
          </div>
          ${s.improvement_pct !== undefined ? html`
          <div class="metric">
            <div class="metric-value good">${s.improvement_pct > 0 ? '+' : ''}${s.improvement_pct.toFixed(1)}%</div>
            <div class="metric-label">Improvement</div>
          </div>
          ` : nothing}
        </div>

        ${progress > 0 ? html`
          <div class="progress-bar">
            <div class="progress-fill" style="width: ${progress}%"></div>
          </div>
        ` : nothing}

        ${this.lossHistory.length > 2 ? this.renderSparkline() : nothing}
      </div>
    `;
  }

  private renderNoForge(): TemplateResult {
    return html`
      <div class="empty-state">
        <div class="icon">&#9881;</div>
        <div class="message">No active forges</div>
        <div class="hint">Start one: python scripts/forge_model.py Qwen/Qwen3.5-4B --domain code</div>
      </div>
    `;
  }

  // ── Output Log Section ─────────────────────────────────────────────────

  @reactive() private _expandedSample: number = -1;

  private renderOutputLog(): TemplateResult {
    if (this.outputSamples.length === 0) {
      return html``;
    }

    // Show newest first
    const samples = [...this.outputSamples].reverse();

    return html`
      <div class="section">
        <div class="section-title">Output Log (${samples.length})</div>
        <div class="output-log">
          ${samples.map((s, i) => this.renderLogEntry(s, i))}
        </div>
      </div>
    `;
  }

  private renderLogEntry(sample: ForgeSample, index: number): TemplateResult {
    const expanded = this._expandedSample === index;
    const preview = sample.output.split('\n')[0].slice(0, 80);
    const time = new Date(sample.timestamp).toLocaleTimeString();

    return html`
      <div class="log-entry">
        <div class="log-header" @click=${() => this.toggleSample(index)}>
          <span class="log-step">Step ${sample.step}</span>
          <span class="log-preview">${preview}</span>
          <span class="log-time">${time}</span>
          <span class="log-expand">${expanded ? '\u25B2' : '\u25BC'}</span>
        </div>
        ${expanded ? html`
          <div class="log-body">
            <div class="log-prompt">${sample.prompt}</div>
            <div class="log-output">${sample.output}</div>
          </div>
        ` : nothing}
      </div>
    `;
  }

  private toggleSample(index: number): void {
    this._expandedSample = this._expandedSample === index ? -1 : index;
  }

  // ── Published Models Section ──────────────────────────────────────────

  private renderPublishedModels(): TemplateResult {
    if (this.models.length === 0 && !this._isLoading) {
      return html`
        <div class="section">
          <div class="section-title">Published Models</div>
          <div class="empty-state">
            <div class="message">No models loaded</div>
            <div class="hint">Published models will appear here once model/list-published is wired</div>
          </div>
        </div>
      `;
    }

    return html`
      <div class="section">
        <div class="section-title">Published Models (${this.models.length})</div>
        <div class="model-grid">
          ${this.models.map(m => this.renderModelCard(m))}
        </div>
      </div>
    `;
  }

  private renderModelCard(m: PublishedModel): TemplateResult {
    const impNum = parseFloat(m.improvement);
    const impClass = isNaN(impNum) ? '' : (impNum < 0 ? 'negative' : '');

    return html`
      <div class="model-card">
        <div class="model-name">${m.name}</div>
        <div class="model-meta">
          <span>${m.domain}</span>
          <span>${m.size}</span>
          <span>${m.downloads.toLocaleString()} downloads</span>
        </div>
        <div class="model-improvement ${impClass}">${m.improvement}</div>
        <div class="model-tags">
          ${m.tags.map(t => html`<span class="tag">${t}</span>`)}
        </div>
      </div>
    `;
  }

  // ── Sparkline ─────────────────────────────────────────────────────────

  private renderSparkline(): TemplateResult {
    const data = this.lossHistory;
    if (data.length < 2) return html``;

    const min = Math.min(...data);
    const max = Math.max(...data);
    const range = max - min || 1;
    const w = 100;
    const h = 40;

    const points = data.map((v, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = h - ((v - min) / range) * (h - 4) - 2;
      return `${x},${y}`;
    });

    const d = `M ${points.join(' L ')}`;

    return html`
      <div class="sparkline">
        <svg viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
          <path d="${d}" />
        </svg>
      </div>
    `;
  }

  // ── Helpers ───────────────────────────────────────────────────────────

  private formatETA(seconds: number): string {
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return `${h}h${m}m`;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
