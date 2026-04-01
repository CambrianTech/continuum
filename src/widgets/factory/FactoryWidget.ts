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
  unsafeCSS,
  reactive,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { styles as FACTORY_STYLES } from './public/factory-widget.styles';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '@shared/generated-command-constants';
import { ContentService } from '../../system/state/ContentService';

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

  protected override get cacheKey(): string { return 'factory'; }
  protected override get cacheableProperties(): string[] {
    return ['_models', '_totalDownloads', '_gridJobs', '_gridJobSummary', '_gridNodeOnline', '_targetNodeId'];
  }

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
    this.startGridPolling();
    this.configureRightPanel();
    this.listenForModelSelection();
  }

  /** Listen for model selection and alloy loading from right panel */
  private listenForModelSelection(): void {
    this._factoryUnsubs = [
      Events.subscribe('factory:model:select', (detail: { modelId: string; name?: string; domain?: string }) => {
        if (detail?.modelId) {
          const controls = this.shadowRoot?.querySelector('forge-controls-element') as HTMLElement & { setBaseModel?: (id: string) => void };
          if (controls?.setBaseModel) {
            controls.setBaseModel(detail.modelId);
          }
        }
      }),
      Events.subscribe('factory:alloy:load', (detail: { alloy: Record<string, unknown>; draftId?: string }) => {
        if (detail?.alloy) {
          // Load alloy into forge controls — set base model and pipeline stages
          const controls = this.shadowRoot?.querySelector('forge-controls-element') as HTMLElement & { setBaseModel?: (id: string) => void };
          const source = detail.alloy.source as Record<string, unknown> | undefined;
          if (controls?.setBaseModel && source?.baseModel) {
            controls.setBaseModel(source.baseModel as string);
          }
          // Pipeline stages will be loaded when pipeline composer supports it
        }
      }),
    ];
  }

  private _factoryUnsubs: Array<() => void> = [];

  /** Tell the right panel what widget to show for the factory */
  private configureRightPanel(): void {
    // Emit immediately AND after a short delay — the right panel may
    // not be subscribed yet on first mount, but we also need to re-emit
    // when navigating back to Factory after visiting another tab.
    this.emitRightPanelConfig();
    setTimeout(() => this.emitRightPanelConfig(), 200);
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
  // Forge status is now derived from grid/job-queue polling (see pollGridJobQueue).
  // No separate forge-status polling needed.

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

        // Derive forge status from the first running grid job
        const runningJob = this._gridJobs.find(j => j.state === 'running');
        if (runningJob?.progress) {
          const p = runningJob.progress;
          this._forgeStatus = {
            phase: 'training',
            detail: `${runningJob.alloyName} on ${runningJob.nodeId}`,
            vramGb: 0,
            timestamp: new Date().toISOString(),
            step: p.step,
            totalSteps: p.totalSteps,
            cycle: p.cycle,
            totalCycles: p.totalCycles,
          };
        } else if (!this._gridJobs.some(j => j.state === 'running') && this._forgeStatus?.phase === 'training') {
          // No running jobs but we had forge status — check if completed
          const completedJob = this._gridJobs.find(j => j.state === 'completed');
          if (completedJob) {
            this._forgeStatus = {
              phase: 'complete',
              detail: `${completedJob.alloyName} complete`,
              vramGb: 0,
              timestamp: new Date().toISOString(),
            };
            this.loadPublishedModels();
          } else {
            this._forgeStatus = null;
          }
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
      const result = await this.executeCommand<any, any>(COMMANDS.MODEL_LIST_PUBLISHED, { includeGguf: true });
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

    const { alloy } = e.detail as { params: Record<string, unknown>; alloy: Record<string, unknown> };
    if (!alloy) {
      console.error('Forge start: no alloy recipe in event detail');
      return;
    }

    // Must have a target node — discovered via grid/nodes on mount
    if (!this._targetNodeId) {
      console.error('Forge start: no grid node available. Pair a node with jtag grid/pair first.');
      return;
    }

    this._forgeStarting = true;
    try {
      const result = await this.executeCommand<any, any>(COMMANDS.GRID_JOB_SUBMIT, {
        nodeId: this._targetNodeId,
        alloy,
        priority: 5,
      });

      if (result?.success) {
        // Job queued — grid/job-queue polling will pick up progress
        console.log(`Forge job ${result.jobId} queued on ${result.nodeId} at position ${result.position}`);
        // Immediately refresh job list
        await this.pollGridJobQueue();
      }
    } catch (err) {
      console.error('Forge job submit failed:', err);
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
    unsafeCSS(FACTORY_STYLES),
  ];

  protected override render(): TemplateResult {
    return html`
      <div class="factory">
        <div class="header">
          <span class="title">Model Factory</span>
          <span class="subtitle">continuum-ai</span>
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

      </div>
    `;
  }

  // ── Grid Rendering ─────────────────────────────────────────────────

  private openGridTab(): void {
    ContentService.open('grid-overview', undefined, { title: 'Grid' });
  }

  private renderNodeStatusBar(): TemplateResult {
    if (this._gridNodes.length === 0 && !this._targetNodeId) {
      return html`
        <div class="node-bar offline node-bar-link" @click=${() => this.openGridTab()}>
          <div class="node-dot"></div>
          <span class="offline-msg">No grid nodes discovered</span>
          <span class="grid-link">Manage Grid →</span>
        </div>
      `;
    }

    const s = this._gridNodeStatus;
    const gpu = s?.gpu;
    const memPct = gpu?.memoryTotalMb ? Math.round((gpu.memoryUsedMb / gpu.memoryTotalMb) * 100) : 0;
    const displayName = this._gridNodes.find(n => n.node_id === this._targetNodeId)?.node_name ?? this._targetNodeId;

    return html`
      <div class="node-bar ${this._gridNodeOnline ? 'online' : 'offline'}">
        ${this.renderForemanBadge()}
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
        <span class="grid-link" @click=${() => this.openGridTab()}>Grid →</span>
      </div>
    `;
  }

  /** Render Foreman identity badge — the persona responsible for this node.
   *  Shows avatar + name if assigned, placeholder if not. Click to DM/call. */
  private renderForemanBadge(): TemplateResult {
    // TODO: Wire to actual Foreman PersonaUser once #671 is implemented
    // For now, show a placeholder that indicates the concept
    const hasForeman = false; // Will be: this._foremanUser !== null
    const foremanName = 'Foreman'; // Will be: this._foremanUser?.entity.displayName
    const nodeOnline = this._gridNodeOnline;

    if (!hasForeman) {
      return html`
        <div class="foreman-badge vacant" title="No foreman assigned to this node">
          <div class="foreman-avatar vacant-avatar">?</div>
          <div class="foreman-info">
            <span class="foreman-role">FOREMAN</span>
            <span class="foreman-name">Unassigned</span>
          </div>
        </div>
      `;
    }

    return html`
      <div class="foreman-badge ${nodeOnline ? 'online' : 'offline'}" title="Contact ${foremanName}">
        <div class="foreman-avatar">
          <div class="foreman-status-dot ${nodeOnline ? 'online' : 'offline'}"></div>
        </div>
        <div class="foreman-info">
          <span class="foreman-role">FOREMAN</span>
          <span class="foreman-name">${foremanName}</span>
        </div>
        <div class="foreman-actions">
          <button class="foreman-action-btn" title="DM Foreman">DM</button>
          <button class="foreman-action-btn call" title="Call Foreman">Call</button>
        </div>
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

    const deadJobs = this._gridJobs.filter(j => j.state === 'failed' || j.state === 'completed' || j.state === 'cancelled');
    const liveJobs = this._gridJobs.filter(j => j.state === 'running' || j.state === 'queued' || j.state === 'paused');

    return html`
      <div class="grid-jobs">
        <div class="jobs-header">
          ${this._gridJobSummary.running > 0 ? html`<span class="jc running">${this._gridJobSummary.running} running</span>` : nothing}
          ${this._gridJobSummary.queued > 0 ? html`<span class="jc queued">${this._gridJobSummary.queued} queued</span>` : nothing}
          ${this._gridJobSummary.paused > 0 ? html`<span class="jc paused">${this._gridJobSummary.paused} paused</span>` : nothing}
          ${this._gridJobSummary.completed > 0 ? html`<span class="jc completed">${this._gridJobSummary.completed} done</span>` : nothing}
          ${deadJobs.length > 0 ? html`
            <button class="jc-clear" @click=${() => this.clearDeadJobs()}>Clear ${deadJobs.length} finished</button>
          ` : nothing}
        </div>
        ${liveJobs.map(job => this.renderJobRow(job))}
        ${deadJobs.map(job => this.renderJobRow(job))}
      </div>
    `;
  }

  private renderJobRow(job: GridJobEntry): TemplateResult {
    return html`
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
          ${job.state === 'failed' || job.state === 'completed' || job.state === 'cancelled' ? html`
            <button class="job-btn cancel" @click=${() => this.dismissJob(job.jobId)} title="Dismiss">&#10005;</button>
          ` : nothing}
        </div>
      </div>
    `;
  }

  private dismissJob(jobId: string): void {
    this._gridJobs = this._gridJobs.filter(j => j.jobId !== jobId);
  }

  private clearDeadJobs(): void {
    this._gridJobs = this._gridJobs.filter(j =>
      j.state === 'running' || j.state === 'queued' || j.state === 'paused'
    );
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
