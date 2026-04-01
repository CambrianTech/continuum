/**
 * FactoryWidget — Model forge production floor (composition root)
 *
 * Thin orchestrator that:
 * - Loads data (published models, forge status)
 * - Subscribes to forge events
 * - Composes child components via Lit composition
 *
 * Child components (each owns its own styles and display logic):
 * - forge-controls-element: Forge parameter form + start button
 * - active-forge-element: Live forge status with metrics and sparkline
 * - published-models-element: Leaderboard-style model list
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
import { COMMANDS } from '@shared/generated-command-constants';

// Import child components (self-registering)
import './ForgeControlsElement';
import './ForgeDeltaElement';
import './ActiveForgeElement';
import './PublishedModelsElement';

import type { ForgeStatusData } from './ActiveForgeElement';
import type { PublishedModelData } from './PublishedModelsElement';

interface GridJobEntry {
  jobId: string;
  alloyName: string;
  state: string;
  progress: { cycle: number; totalCycles: number; step: number; totalSteps: number };
  startedAt: string;
  estimatedCompletion: string;
  nodeId: string;
}

interface GridNodeStatusData {
  state: string;
  gpu: { name: string; utilization: number; memoryUsedMb: number; memoryTotalMb: number; temperatureC: number };
  jobs: Array<{ pid: number; type: string; detail: string; cpu: string; mem: string }>;
  nodeId: string;
  timestamp: string;
}

// ── Component ───────────────────────────────────────────────────────────

export class FactoryWidget extends ReactiveWidget {

  // ── State ──────────────────────────────────────────────────────────
  @reactive() private _forgeStatus: ForgeStatusData | null = null;
  @reactive() private _models: PublishedModelData[] = [];
  @reactive() private _lossHistory: number[] = [];
  @reactive() private _isLoading = true;
  @reactive() private _totalDownloads = 0;
  @reactive() private _forgeStarting = false;

  // ── Grid state ────────────────────────────────────────────────────
  @reactive() private _gridJobs: GridJobEntry[] = [];
  @reactive() private _gridJobSummary = { queued: 0, running: 0, paused: 0, completed: 0, failed: 0 };
  @reactive() private _gridNodeStatus: GridNodeStatusData | null = null;
  @reactive() private _gridNodeOnline = false;
  @reactive() private _targetNodeId = '';
  @reactive() private _gridNodes: Array<{ node_id: string; node_name: string | null; capabilities: unknown[] }> = [];

  private _statusPollInterval: ReturnType<typeof setInterval> | null = null;
  private _gridPollInterval: ReturnType<typeof setInterval> | null = null;

  // ── Forge progress (derived) ───────────────────────────────────────

  private get _progressPct(): number {
    const s = this._forgeStatus;
    if (!s) return 0;
    const totalSteps = (s.totalSteps ?? 1000) * (s.totalCycles ?? 1);
    const currentStep = ((s.cycle ?? 1) - 1) * (s.totalSteps ?? 1000) + (s.step ?? 0);
    return Math.min(100, Math.round((currentStep / totalSteps) * 100));
  }

  private get _progressLabel(): string {
    const s = this._forgeStatus;
    if (!s) return 'FORGING...';
    const pct = this._progressPct;
    const loss = s.loss && s.loss > 0 ? ` · ${s.loss.toFixed(3)}` : '';
    const eta = s.etaSeconds ? ` · ${this.formatETA(s.etaSeconds)}` : '';
    if (s.phase === 'loading' || s.phase === 'loading_data') return 'Loading...';
    if (s.phase === 'baseline_eval') return 'Baseline...';
    if (s.phase === 'complete') return 'Done';
    return `${pct}%${loss}${eta}`;
  }

  private get _isForging(): boolean {
    const phase = this._forgeStatus?.phase;
    return phase === 'training' || phase === 'loading' || phase === 'loading_data'
      || phase === 'baseline_eval' || phase === 'pruning' || phase === 'running'
      || phase === 'post_train_eval' || phase === 'post_prune_eval' || phase === 'defrag';
  }

  // ── Lifecycle ──────────────────────────────────────────────────────

  override connectedCallback(): void {
    super.connectedCallback();
    this.subscribeToForgeEvents();
    this.loadPublishedModels();
    this.startStatusPolling();
    this.startGridPolling();
    this.configureRightPanel();
  }

  /** Tell the right panel what widget to show for the factory */
  private configureRightPanel(): void {
    // Small delay to ensure right panel widget is mounted and listening
    setTimeout(() => this.emitRightPanelConfig(), 500);
  }

  private emitRightPanelConfig(): void {
    Events.emit('layout:rightpanel:configure', {
      widget: 'factory-stats-widget',
      contentType: 'factory',
      sections: [{
        id: 'factory-stats',
        title: 'Models',
        icon: '🏭',
        widgetTag: 'factory-stats-widget',
        flexWeight: 1,
      }],
    });
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this._statusPollInterval) { clearInterval(this._statusPollInterval); this._statusPollInterval = null; }
    if (this._gridPollInterval) { clearInterval(this._gridPollInterval); this._gridPollInterval = null; }
  }

  // ── Event Subscriptions ────────────────────────────────────────────

  private subscribeToForgeEvents(): void {
    Events.subscribe('model:forge:step', (data: any) => {
      this._forgeStatus = {
        phase: 'training',
        detail: data.detail ?? '',
        vramGb: data.vramGb ?? 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        step: data.step,
        totalSteps: data.totalSteps,
        loss: data.loss,
        itPerSec: data.itPerSec,
        etaSeconds: data.etaSeconds,
        cycle: data.cycle,
        totalCycles: data.totalCycles,
      };
      if (data.loss !== undefined) {
        this._lossHistory = [...this._lossHistory.slice(-50), data.loss];
      }
    });

    Events.subscribe('model:forge:phase', (data: any) => {
      if (this._forgeStatus) {
        this._forgeStatus = { ...this._forgeStatus, phase: data.phase, detail: data.detail ?? '' };
      }
    });

    Events.subscribe('model:forge:complete', (data: any) => {
      this._forgeStatus = {
        phase: 'complete',
        detail: data.detail ?? 'Forge complete',
        vramGb: 0,
        timestamp: data.timestamp ?? new Date().toISOString(),
        improvementPct: data.improvementPct,
        perplexity: data.perplexity,
      };
      this.loadPublishedModels();
    });
  }

  // ── Status Polling ─────────────────────────────────────────────────

  private startStatusPolling(): void {
    this.pollForgeStatus();
    this._statusPollInterval = setInterval(() => this.pollForgeStatus(), 15_000);
  }

  private async pollForgeStatus(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('model/forge-status', {});
      if (!result?.forges?.length) {
        // No active forges — clear stale status
        if (this._forgeStatus && this._forgeStatus.phase !== 'complete') {
          this._forgeStatus = null;
        }
        return;
      }
      if (result?.forges?.length > 0) {
        const f = result.forges[0];
        this._forgeStatus = {
          phase: f.phase ?? 'unknown',
          detail: f.detail ?? '',
          vramGb: f.vramGb ?? 0,
          timestamp: f.timestamp ?? new Date().toISOString(),
          step: f.step,
          totalSteps: f.totalSteps,
          loss: f.loss,
          itPerSec: f.itPerSec,
          etaSeconds: f.etaSeconds,
          cycle: f.cycle,
          totalCycles: f.totalCycles,
        };
        if (f.loss && f.loss > 0) {
          this._lossHistory = [...this._lossHistory.slice(-50), f.loss];
        }
        if (f.phase === 'complete' || f.phase === 'error') {
          if (this._statusPollInterval) {
            clearInterval(this._statusPollInterval);
            this._statusPollInterval = null;
          }
          this.loadPublishedModels();
        }
      }
    } catch {
      // Node unreachable
    }
  }

  // ── Grid Polling ───────────────────────────────────────────────────

  private async startGridPolling(): Promise<void> {
    await this.discoverGridNodes();
    if (this._targetNodeId) {
      this.pollGridNodeStatus();
      this.pollGridJobQueue();
      this._gridPollInterval = setInterval(() => {
        this.pollGridNodeStatus();
        this.pollGridJobQueue();
      }, 10_000);
    }
  }

  private async discoverGridNodes(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>(COMMANDS.GRID_NODES, {});
      const nodes = (result?.nodes ?? []) as Array<{ node_id: string; node_name: string | null; capabilities: unknown[] }>;
      this._gridNodes = nodes;
      // Auto-select first node with compute capability, or first node
      if (!this._targetNodeId && nodes.length > 0) {
        const gpuNode = nodes.find(n =>
          Array.isArray(n.capabilities) && n.capabilities.some((c: any) => c.type === 'compute')
        );
        this._targetNodeId = (gpuNode ?? nodes[0]).node_id;
      }
    } catch {
      this._gridNodes = [];
    }
  }

  private async pollGridNodeStatus(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>(COMMANDS.GRID_NODE_STATUS, {
        nodeId: this._targetNodeId,
      });
      if (result?.success) {
        this._gridNodeStatus = result as GridNodeStatusData;
        this._gridNodeOnline = result.state !== 'offline' && result.state !== 'error';
      }
    } catch {
      this._gridNodeOnline = false;
      this._gridNodeStatus = null;
    }
  }

  private async pollGridJobQueue(): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>(COMMANDS.GRID_JOB_QUEUE, {
        nodeId: this._targetNodeId,
        state: 'all',
        limit: 10,
      });
      if (result?.success) {
        this._gridJobs = (result.jobs ?? []) as GridJobEntry[];
        const summary = result.summary as Record<string, number> | undefined;
        if (summary) {
          this._gridJobSummary = {
            queued: summary.queued ?? 0,
            running: summary.running ?? 0,
            paused: summary.paused ?? 0,
            completed: summary.completed ?? 0,
            failed: summary.failed ?? 0,
          };
        }
      }
    } catch {
      // Keep existing state
    }
  }

  private async controlGridJob(jobId: string, action: string): Promise<void> {
    try {
      await this.executeCommand<any, any>(COMMANDS.GRID_JOB_CONTROL, {
        jobId,
        action,
        nodeId: this._targetNodeId,
      });
      await this.pollGridJobQueue();
    } catch (e) {
      console.error(`Job ${action} failed:`, e);
    }
  }

  // ── Data Loading ───────────────────────────────────────────────────

  private async loadPublishedModels(): Promise<void> {
    this._isLoading = true;
    try {
      const result = await this.executeCommand<any, any>('model/list-published', { includeGguf: true });
      if (result?.models) {
        this._models = result.models.sort((a: any, b: any) => b.downloads - a.downloads);
        this._totalDownloads = result.totalDownloads ?? 0;
      }
    } catch {
      this._models = [];
    }
    this._isLoading = false;
  }

  // ── Event Handlers (from child components) ─────────────────────────

  private async onForgeStart(e: CustomEvent): Promise<void> {
    if (this._isForging || this._forgeStarting) return;
    this._forgeStarting = true;
    try {
      await this.executeCommand<any, any>('model/forge', e.detail);
    } catch (err) {
      console.error('Forge start failed:', err);
    } finally {
      this._forgeStarting = false;
    }
  }

  private onForgeExport(e: CustomEvent): void {
    const alloy = e.detail;
    const blob = new Blob([JSON.stringify(alloy, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${alloy.name}.alloy.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  // ── Rendering ──────────────────────────────────────────────────────

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

      .section-stats {
        font-size: 12px;
        font-weight: 400;
        color: var(--content-secondary, #8a92a5);
        margin-left: 12px;
      }

      /* ── Node Status Bar ───────────────── */

      .node-bar {
        display: flex;
        align-items: center;
        gap: 10px;
        padding: 10px 14px;
        margin-bottom: 16px;
        border-radius: 8px;
        background: rgba(0,0,0,0.3);
        border: 1px solid rgba(255,255,255,0.06);
        font-size: 12px;
      }

      .node-dot {
        width: 8px; height: 8px;
        border-radius: 50%;
        background: #ff4444;
        flex-shrink: 0;
      }

      .node-bar.online .node-dot {
        background: #00ff88;
        box-shadow: 0 0 8px rgba(0, 255, 136, 0.5);
      }

      .node-name {
        font-weight: 700;
        color: var(--content-primary, #e0e6ed);
      }

      .gpu-label {
        color: var(--content-secondary, #8a92a5);
        font-size: 11px;
        margin-right: auto;
      }

      .bar-group { display: flex; align-items: center; gap: 4px; }

      .bar-label {
        font-size: 9px; font-weight: 600;
        color: var(--content-tertiary, #5a6070);
        width: 26px;
      }

      .usage-bar {
        width: 60px; height: 6px;
        background: rgba(255,255,255,0.06);
        border-radius: 3px; overflow: hidden;
      }

      .usage-fill {
        height: 100%; border-radius: 3px;
        transition: width 0.5s ease;
      }

      .gpu-fill { background: linear-gradient(90deg, #00d4ff, #00ffc8); }
      .mem-fill { background: linear-gradient(90deg, #c864ff, #ff6464); }

      .bar-val {
        font-size: 10px; font-variant-numeric: tabular-nums;
        color: var(--content-secondary, #8a92a5);
        min-width: 36px; text-align: right;
      }

      .temp-label {
        font-size: 11px;
        color: var(--content-tertiary, #5a6070);
      }

      .offline-msg {
        color: var(--content-tertiary, #5a6070);
        font-style: italic; flex: 1;
      }

      /* ── Empty Floor ───────────────────── */

      .empty-floor {
        text-align: center;
        padding: 40px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-floor-icon { font-size: 36px; margin-bottom: 8px; }
      .empty-floor-msg { font-size: 14px; margin-bottom: 4px; }
      .empty-floor-hint { font-size: 12px; opacity: 0.7; }

      /* ── Grid Jobs ─────────────────────── */

      .grid-jobs { margin-top: 12px; }

      .jobs-header {
        display: flex; gap: 8px;
        margin-bottom: 8px;
      }

      .jc {
        font-size: 10px; padding: 2px 8px;
        border-radius: 4px; font-weight: 600;
      }

      .jc.running { background: rgba(0, 212, 255, 0.12); color: #00d4ff; }
      .jc.queued { background: rgba(255, 255, 100, 0.1); color: #ffff64; }
      .jc.paused { background: rgba(255, 150, 100, 0.1); color: #ff9664; }
      .jc.completed { background: rgba(0, 255, 136, 0.1); color: #00ff88; }

      .job-row {
        display: flex; align-items: center;
        justify-content: space-between;
        padding: 8px 12px; border-radius: 6px;
        background: rgba(0,0,0,0.15);
        margin-bottom: 4px;
        border-left: 3px solid transparent;
        transition: all 0.15s;
      }

      .job-row.running { border-left-color: #00d4ff; }
      .job-row.paused { border-left-color: #ff9664; }
      .job-row.queued { border-left-color: #ffff64; }
      .job-row.completed { border-left-color: #00ff88; opacity: 0.5; }
      .job-row.failed { border-left-color: #ff4444; opacity: 0.5; }

      .job-info { display: flex; align-items: center; gap: 10px; }

      .job-name {
        font-size: 12px; font-weight: 600;
        color: var(--content-primary, #e0e6ed);
      }

      .job-state-badge {
        font-size: 9px; font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--content-tertiary, #5a6070);
      }

      .job-progress {
        font-size: 11px; font-variant-numeric: tabular-nums;
        color: var(--accent-primary, #00d4ff);
      }

      .job-time {
        font-size: 10px;
        color: var(--content-tertiary, #5a6070);
      }

      .job-actions { display: flex; gap: 4px; }

      .job-btn {
        width: 24px; height: 24px;
        border: 1px solid rgba(255,255,255,0.1);
        border-radius: 4px;
        background: transparent;
        color: var(--content-secondary, #8a92a5);
        font-size: 11px; cursor: pointer;
        display: flex; align-items: center;
        justify-content: center;
        transition: all 0.15s;
      }

      .job-btn:hover {
        background: rgba(0, 212, 255, 0.15);
        border-color: var(--accent-primary, #00d4ff);
        color: var(--accent-primary, #00d4ff);
      }

      .job-btn.cancel:hover {
        background: rgba(255, 68, 68, 0.15);
        border-color: #ff4444; color: #ff4444;
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

        <div class="section">
          <div class="section-title">Console</div>
          <forge-controls-element
            .forging=${this._isForging}
            .starting=${this._forgeStarting}
            .progressPct=${this._progressPct}
            .progressLabel=${this._progressLabel}
            @forge-start=${this.onForgeStart}
            @forge-export=${this.onForgeExport}
          ></forge-controls-element>
        </div>

        <div class="section">
          <div class="section-title">Factory Floor</div>
          ${this.renderNodeStatusBar()}
          ${this._forgeStatus ? html`
            <active-forge-element
              .status=${this._forgeStatus}
              .lossHistory=${this._lossHistory}
            ></active-forge-element>
          ` : nothing}
          ${this.renderGridJobList()}
        </div>

      </div>
    `;
  }

  // ── Grid Rendering ─────────────────────────────────────────────────

  private renderNodeStatusBar(): TemplateResult {
    if (this._gridNodes.length === 0 && !this._targetNodeId) {
      return html`
        <div class="node-bar offline">
          <div class="node-dot"></div>
          <span class="offline-msg">No grid nodes discovered. Pair a node with <code>jtag grid/pair</code></span>
        </div>
      `;
    }

    const s = this._gridNodeStatus;
    const gpu = s?.gpu;
    const memPct = gpu?.memoryTotalMb ? Math.round((gpu.memoryUsedMb / gpu.memoryTotalMb) * 100) : 0;
    const displayName = this._gridNodes.find(n => n.node_id === this._targetNodeId)?.node_name ?? this._targetNodeId;

    return html`
      <div class="node-bar ${this._gridNodeOnline ? 'online' : 'offline'}">
        <div class="node-dot"></div>
        ${this._gridNodes.length > 1 ? html`
          <select class="node-select"
            .value=${this._targetNodeId}
            @change=${(e: Event) => {
              this._targetNodeId = (e.target as HTMLSelectElement).value;
              this._gridNodeStatus = null;
              this._gridNodeOnline = false;
              this.pollGridNodeStatus();
              this.pollGridJobQueue();
            }}>
            ${this._gridNodes.map(n => html`
              <option value=${n.node_id}>${n.node_name ?? n.node_id}</option>
            `)}
          </select>
        ` : html`
          <span class="node-name">${displayName}</span>
        `}
        ${this._gridNodeOnline && gpu?.name ? html`
          <span class="gpu-label">${gpu.name}</span>
          <div class="bar-group">
            <span class="bar-label">GPU</span>
            <div class="usage-bar"><div class="usage-fill gpu-fill" style="width:${gpu.utilization}%"></div></div>
            <span class="bar-val">${gpu.utilization}%</span>
          </div>
          <div class="bar-group">
            <span class="bar-label">MEM</span>
            <div class="usage-bar"><div class="usage-fill mem-fill" style="width:${memPct}%"></div></div>
            <span class="bar-val">${Math.round(gpu.memoryUsedMb / 1024)}/${Math.round(gpu.memoryTotalMb / 1024)}GB</span>
          </div>
          <span class="temp-label">${gpu.temperatureC}°C</span>
        ` : html`
          <span class="offline-msg">${s ? s.state : 'connecting...'}</span>
        `}
      </div>
    `;
  }

  private renderGridJobList(): TemplateResult {
    const hasJobs = this._gridJobs.length > 0;
    const hasForge = !!this._forgeStatus;

    if (!hasJobs && !hasForge) {
      return html`
        <div class="empty-floor">
          <div class="empty-floor-icon">&#9881;</div>
          <div class="empty-floor-msg">No active forges</div>
          <div class="empty-floor-hint">Configure a forge above and hit START FORGE</div>
        </div>
      `;
    }

    if (!hasJobs) return html``;

    return html`
      <div class="grid-jobs">
        <div class="jobs-header">
          ${this._gridJobSummary.running > 0 ? html`<span class="jc running">${this._gridJobSummary.running} running</span>` : nothing}
          ${this._gridJobSummary.queued > 0 ? html`<span class="jc queued">${this._gridJobSummary.queued} queued</span>` : nothing}
          ${this._gridJobSummary.paused > 0 ? html`<span class="jc paused">${this._gridJobSummary.paused} paused</span>` : nothing}
          ${this._gridJobSummary.completed > 0 ? html`<span class="jc completed">${this._gridJobSummary.completed} done</span>` : nothing}
        </div>
        ${this._gridJobs.map(job => html`
          <div class="job-row ${job.state}">
            <div class="job-info">
              <span class="job-name">${job.alloyName}</span>
              <span class="job-state-badge">${job.state}</span>
              ${job.progress?.totalSteps > 0 ? html`
                <span class="job-progress">${job.progress.step}/${job.progress.totalSteps}</span>
              ` : nothing}
              ${job.startedAt ? html`<span class="job-time">${this.formatRelativeTime(job.startedAt)}</span>` : nothing}
            </div>
            <div class="job-actions">
              ${job.state === 'running' ? html`
                <button class="job-btn" @click=${() => this.controlGridJob(job.jobId, 'pause')} title="Pause">&#9208;</button>
                <button class="job-btn cancel" @click=${() => this.controlGridJob(job.jobId, 'cancel')} title="Cancel">&#10005;</button>
              ` : nothing}
              ${job.state === 'paused' ? html`
                <button class="job-btn" @click=${() => this.controlGridJob(job.jobId, 'resume')} title="Resume">&#9654;</button>
                <button class="job-btn cancel" @click=${() => this.controlGridJob(job.jobId, 'cancel')} title="Cancel">&#10005;</button>
              ` : nothing}
            </div>
          </div>
        `)}
      </div>
    `;
  }

  private formatRelativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }

  // ── Helpers ────────────────────────────────────────────────────────

  private formatETA(seconds: number): string {
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return `${h}h${m}m`;
  }
}

// Self-register
if (!customElements.get('factory-widget')) {
  customElements.define('factory-widget', FactoryWidget);
}
