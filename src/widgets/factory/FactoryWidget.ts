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

        ${this.renderActiveForge()}
        ${this.renderOutputLog()}
        ${this.renderPublishedModels()}
      </div>
    `;
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
