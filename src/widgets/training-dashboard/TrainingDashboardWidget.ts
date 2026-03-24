/**
 * TrainingDashboardWidget — Full-tab TensorBoard replacement
 *
 * Real-time training visualization:
 *   - Loss curve (multi-series: train loss, optionally validation)
 *   - Token accuracy over time
 *   - Learning rate schedule
 *   - GPU memory usage
 *   - Active training cards with live progress
 *   - Historical training runs table
 *
 * All charts use <continuum-chart>. All data via Events + Commands.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import { Events } from '../../system/core/shared/Events';
import {
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingStepEventData,
  type AITrainingCompleteEventData,
  type AITrainingErrorEventData,
} from '../../system/events/shared/AILearningEvents';
import '../shared/ContinuumChart';
import type { ContinuumChartSeries } from '../shared/ContinuumChart';

// ── Types ───────────────────────────────────────────────────────────────────

/** Per-step data point for charting */
interface StepDataPoint {
  step: number;
  loss: number;
  learningRate: number;
  tokenAccuracy?: number;
  memoryMb?: number;
  epoch?: number;
}

/** Active training session */
interface ActiveSession {
  personaId: string;
  personaName: string;
  domain: string;
  provider: string;
  startedAt: number;
  exampleCount: number;
  steps: StepDataPoint[];
  latestLoss: number;
  latestEpoch?: number;
}

/** Historical training run from genome/layers */
interface HistoricalRun {
  name: string;
  domain: string;
  baseModel: string;
  finalLoss: number;
  epochs: number;
  examplesProcessed: number;
  maturity: number;
  createdAt: string;
  lossHistory: number[];
  /** Node that trained this adapter (undefined = local) */
  nodeName?: string;
  personaName?: string;
}

/** Grid node for remote data fetching */
interface GridNodeInfo {
  nodeId: string;
  nodeName: string;
}

/** Active Academy session from genome/academy-session-list */
interface AcademySession {
  id: string;
  personaId: string;
  personaName: string;
  skill: string;
  status: string;
  baseModel: string;
  mode: string;
  createdAt: string;
  teacherHandle: string;
  studentHandle: string;
  nodeName?: string;
  /** Detail fields (loaded on demand) */
  curricula?: any[];
  examinations?: any[];
  adapterIds?: string[];
}

// ── Chart series configs ────────────────────────────────────────────────────

const LOSS_SERIES: ContinuumChartSeries[] = [
  { key: 'loss', color: 'rgba(0, 255, 200, 0.9)', label: 'Loss' },
];

const LR_SERIES: ContinuumChartSeries[] = [
  { key: 'learningRate', color: 'rgba(0, 212, 255, 0.9)', label: 'Learning Rate' },
];

const ACCURACY_SERIES: ContinuumChartSeries[] = [
  { key: 'tokenAccuracy', color: '#4ade80', label: 'Token Accuracy' },
];

const MEMORY_SERIES: ContinuumChartSeries[] = [
  { key: 'memoryMb', color: '#ff6b6b', label: 'GPU Memory (MB)' },
];

// ── Component ───────────────────────────────────────────────────────────────

export class TrainingDashboardWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e0e6ed);
        font-family: var(--font-primary, sans-serif);
      }

      .dashboard {
        padding: 16px 20px;
        max-width: 1200px;
        margin: 0 auto;
      }

      .dashboard-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 20px;
      }

      .dashboard-title {
        font-size: 18px;
        font-weight: 700;
        color: var(--content-primary, #e0e6ed);
      }

      .dashboard-subtitle {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-top: 2px;
      }

      /* Active training cards */
      .active-section {
        margin-bottom: 24px;
      }

      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin-bottom: 8px;
      }

      .active-card {
        background: rgba(0, 255, 200, 0.04);
        border: 1px solid rgba(0, 255, 200, 0.15);
        border-radius: 6px;
        padding: 12px 14px;
        margin-bottom: 12px;
      }

      .card-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
      }

      .card-title {
        font-weight: 700;
        font-size: 13px;
        color: rgba(0, 255, 200, 0.9);
      }

      .card-persona {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
      }

      .card-stats {
        display: flex;
        gap: 16px;
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 10px;
      }

      .stat-value {
        color: rgba(0, 212, 255, 0.9);
        font-family: var(--font-mono, monospace);
        font-weight: 600;
      }

      /* Chart grid */
      .chart-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 16px;
        margin-bottom: 24px;
      }

      @media (max-width: 700px) {
        .chart-grid {
          grid-template-columns: 1fr;
        }
      }

      .chart-panel {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 12px;
      }

      .chart-panel.wide {
        grid-column: 1 / -1;
      }

      .chart-title {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        margin-bottom: 8px;
      }

      .chart-value-large {
        font-size: 22px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        color: rgba(0, 255, 200, 0.9);
        margin-bottom: 4px;
      }

      .chart-value-sub {
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 8px;
      }

      /* Historical runs table */
      .history-section {
        margin-top: 24px;
      }

      .history-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 11px;
      }

      .history-table th {
        text-align: left;
        padding: 6px 8px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        font-size: 9px;
        border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      }

      .history-table td {
        padding: 6px 8px;
        color: var(--content-primary, #e0e6ed);
        border-bottom: 1px solid rgba(255, 255, 255, 0.04);
      }

      .history-table tr:hover td {
        background: rgba(0, 212, 255, 0.04);
      }

      .loss-badge {
        font-family: var(--font-mono, monospace);
        font-weight: 600;
        color: rgba(0, 255, 200, 0.9);
      }

      .maturity-bar {
        width: 60px;
        height: 4px;
        background: rgba(60, 80, 100, 0.4);
        border-radius: 2px;
        overflow: hidden;
        display: inline-block;
        vertical-align: middle;
      }

      .maturity-fill {
        height: 100%;
        background: linear-gradient(90deg, rgba(0, 255, 200, 0.6), rgba(0, 212, 255, 0.8));
        border-radius: 2px;
      }

      .mini-sparkline {
        width: 80px;
        height: 24px;
      }

      .empty-state {
        text-align: center;
        padding: 40px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-state-title {
        font-size: 16px;
        font-weight: 600;
        margin-bottom: 8px;
      }

      .empty-state-hint {
        font-size: 12px;
        font-style: italic;
      }
    `,
  ] as CSSResultGroup;

  // ── Reactive state ──────────────────────────────────────────────────────

  @reactive() private _activeSessions: Map<string, ActiveSession> = new Map();
  @reactive() private _historicalRuns: HistoricalRun[] = [];
  @reactive() private _selectedSessionId: string | null = null;
  @reactive() private _academySessions: AcademySession[] = [];

  private _cleanups: (() => void)[] = [];
  private _pollTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    super({ widgetName: 'TrainingDashboardWidget' });
  }

  // ── Lifecycle ───────────────────────────────────────────────────────────

  protected override onFirstRender(): void {
    super.onFirstRender();

    // Subscribe to training events
    this._cleanups.push(
      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: AITrainingStartedEventData) => {
        const updated = new Map(this._activeSessions);
        updated.set(data.personaId, {
          personaId: data.personaId,
          personaName: data.personaName,
          domain: data.domain,
          provider: data.provider,
          startedAt: data.timestamp,
          exampleCount: data.exampleCount,
          steps: [],
          latestLoss: 0,
        });
        this._activeSessions = updated;
        // Auto-select first active session
        if (!this._selectedSessionId) {
          this._selectedSessionId = data.personaId;
        }
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_STEP, (data: AITrainingStepEventData) => {
        const session = this._activeSessions.get(data.personaId);
        if (!session) return;

        const point: StepDataPoint = {
          step: data.step,
          loss: data.loss,
          learningRate: data.learningRate,
          tokenAccuracy: data.tokenAccuracy,
          memoryMb: data.memoryMb,
          epoch: data.epoch,
        };

        const updated = new Map(this._activeSessions);
        updated.set(data.personaId, {
          ...session,
          steps: [...session.steps, point],
          latestLoss: data.loss,
          latestEpoch: data.epoch,
        });
        this._activeSessions = updated;
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_COMPLETE, (data: AITrainingCompleteEventData) => {
        const updated = new Map(this._activeSessions);
        updated.delete(data.personaId);
        this._activeSessions = updated;

        if (this._selectedSessionId === data.personaId) {
          this._selectedSessionId = null;
        }

        // Refresh historical data
        this._loadHistoricalRuns();
      }),

      Events.subscribe(AI_LEARNING_EVENTS.TRAINING_ERROR, (data: AITrainingErrorEventData) => {
        const updated = new Map(this._activeSessions);
        updated.delete(data.personaId);
        this._activeSessions = updated;

        if (this._selectedSessionId === data.personaId) {
          this._selectedSessionId = null;
        }
      }),
    );

    // Load all data
    this._loadHistoricalRuns();
    this._loadAcademySessions();

    // Poll for active academy sessions every 30s
    this._pollTimer = setInterval(() => {
      if (this._academySessions.some(s => !['completed', 'failed', 'cancelled'].includes(s.status))) {
        this._loadAcademySessions();
      }
    }, 30_000);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this._cleanups.forEach(fn => fn());
    this._cleanups = [];
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
  }

  // ── Data loading ────────────────────────────────────────────────────────

  private async _loadHistoricalRuns(): Promise<void> {
    const allRuns: HistoricalRun[] = [];

    // 1. Load local training runs
    await this._loadRunsFromLocal(allRuns);

    // 2. Load from remote grid nodes
    await this._loadRunsFromGrid(allRuns);

    // Sort by creation date, newest first
    allRuns.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    this._historicalRuns = allRuns;
  }

  /** Load training runs from the local node */
  private async _loadRunsFromLocal(allRuns: HistoricalRun[]): Promise<void> {
    try {
      const usersResult = await this.executeCommand<any, any>('data/list', {
        collection: 'users',
        filter: { type: 'ai' },
        limit: 50,
      });

      if (!usersResult.success || !usersResult.items) return;

      for (const user of usersResult.items) {
        await this._loadPersonaLayers(allRuns, user.id, user.uniqueId ?? user.displayName, undefined);
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load local training data:', err);
    }
  }

  /** Discover grid nodes and load training runs from each */
  private async _loadRunsFromGrid(allRuns: HistoricalRun[]): Promise<void> {
    try {
      const nodesResult = await this.executeCommand<any, any>('grid/nodes', {});
      if (!nodesResult.success && !nodesResult.nodes) return;

      const nodes: GridNodeInfo[] = (nodesResult.nodes ?? []).map((n: any) => ({
        nodeId: n.node_id ?? n.nodeId,
        nodeName: n.node_name ?? n.nodeName ?? n.node_id ?? n.nodeId,
      }));

      for (const node of nodes) {
        try {
          // Get users from remote node
          const remoteUsers = await this.executeCommand<any, any>('grid/send', {
            nodeId: node.nodeId,
            remoteCommand: 'user/list',
            params: { limit: 50 },
          });

          const users = remoteUsers?.remoteResult?.items ?? [];
          for (const user of users) {
            if (user.type !== 'ai') continue;
            await this._loadRemotePersonaLayers(allRuns, node, user.id, user.uniqueId ?? user.displayName);
          }
        } catch {
          // Node may be unreachable
        }
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load grid training data:', err);
    }
  }

  /** Load genome/layers for a persona on the local node */
  private async _loadPersonaLayers(
    allRuns: HistoricalRun[],
    personaId: string,
    personaName: string,
    nodeName: string | undefined,
  ): Promise<void> {
    try {
      const layersResult = await this.executeCommand<any, any>('genome/layers', {
        personaId,
        personaName,
      });

      if (layersResult.success && layersResult.layers) {
        this._extractRuns(allRuns, layersResult.layers, personaName, nodeName);
      }
    } catch (err) {
      console.warn(`[TrainingDashboard] Failed to load layers for ${personaName}:`, err);
    }
  }

  /** Load genome/layers for a persona on a remote grid node */
  private async _loadRemotePersonaLayers(
    allRuns: HistoricalRun[],
    node: GridNodeInfo,
    personaId: string,
    personaName: string,
  ): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('grid/send', {
        nodeId: node.nodeId,
        remoteCommand: 'genome/layers',
        params: { personaId, personaName },
      });

      const remote = result?.remoteResult;
      if (remote?.success && remote?.layers) {
        this._extractRuns(allRuns, remote.layers, personaName, node.nodeName);
      }
    } catch (err) {
      console.warn(`[TrainingDashboard] Failed to load from grid node ${node.nodeName}:`, err);
    }
  }

  /** Extract HistoricalRun entries from genome layers */
  private _extractRuns(
    allRuns: HistoricalRun[],
    layers: any[],
    personaName: string,
    nodeName: string | undefined,
  ): void {
    for (const layer of layers) {
      if (layer.trainingMetrics) {
        allRuns.push({
          name: layer.name,
          domain: layer.domain,
          baseModel: layer.baseModel,
          finalLoss: layer.trainingMetrics.finalLoss ?? 0,
          epochs: layer.trainingMetrics.epochs ?? 0,
          examplesProcessed: layer.trainingMetrics.examplesProcessed ?? 0,
          maturity: layer.maturity ?? 0,
          createdAt: layer.createdAt ?? '',
          lossHistory: layer.trainingMetrics.lossHistory ?? [],
          nodeName,
          personaName,
        });
      }
    }
  }

  // ── Academy session loading ──────────────────────────────────────────

  private async _loadAcademySessions(): Promise<void> {
    const allSessions: AcademySession[] = [];

    // Local sessions
    try {
      const usersResult = await this.executeCommand<any, any>('data/list', {
        collection: 'users', filter: { type: 'ai' }, limit: 50,
      });
      if (usersResult.success && usersResult.items) {
        for (const user of usersResult.items) {
          await this._loadPersonaAcademySessions(allSessions, user.id, user.uniqueId ?? user.displayName, undefined);
        }
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load local academy sessions:', err);
    }

    // Remote grid nodes — fetch academy sessions directly (skip user list hop)
    try {
      const nodesResult = await this.executeCommand<any, any>('grid/nodes', {});
      const rawNodes = nodesResult.nodes ?? [];
      const nodes: GridNodeInfo[] = rawNodes.map((n: any) => ({
        nodeId: n.node_id ?? n.nodeId,
        nodeName: n.node_name ?? n.nodeName ?? n.node_id ?? n.nodeId,
      }));

      for (const node of nodes) {
        try {
          // Fetch ALL academy sessions from the node in one call (no personaId filter)
          const result = await this.executeCommand<any, any>('grid/send', {
            nodeId: node.nodeId, remoteCommand: 'genome/academy-session-list', params: {},
          });
          const remote = result?.remoteResult;
          if (remote?.sessions) {
            for (const s of remote.sessions) {
              allSessions.push({ ...s, nodeName: node.nodeName });
            }
          }
        } catch (err) {
          console.warn(`[TrainingDashboard] Failed to load academy sessions from ${node.nodeName}:`, err);
        }
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load grid for academy sessions:', err);
    }

    this._academySessions = allSessions;
  }

  private async _loadPersonaAcademySessions(
    allSessions: AcademySession[], personaId: string, personaName: string, nodeName: string | undefined
  ): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('genome/academy-session-list', { personaId });
      if (result.success && result.sessions) {
        for (const s of result.sessions) {
          allSessions.push({ ...s, personaName: s.personaName ?? personaName, nodeName });
        }
      }
    } catch { /* skip */ }
  }

  private async _loadRemoteAcademySessions(
    allSessions: AcademySession[], node: GridNodeInfo, personaId: string, personaName: string
  ): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('grid/send', {
        nodeId: node.nodeId, remoteCommand: 'genome/academy-session-list', params: { personaId },
      });
      const remote = result?.remoteResult;
      if (remote?.success && remote?.sessions) {
        for (const s of remote.sessions) {
          allSessions.push({ ...s, personaName: s.personaName ?? personaName, nodeName: node.nodeName });
        }
      }
    } catch { /* skip */ }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    const activeSessions = [...this._activeSessions.values()];
    const selected = this._selectedSessionId
      ? this._activeSessions.get(this._selectedSessionId)
      : activeSessions[0] ?? null;

    const hasAnything = activeSessions.length > 0 || this._historicalRuns.length > 0 || this._academySessions.length > 0;

    if (!hasAnything) {
      return html`
        <div class="dashboard">
          <div class="empty-state">
            <div class="empty-state-title">No Training Data</div>
            <div class="empty-state-hint">
              Start a training session from the Academy or use genome/train to begin.
              Real-time metrics will appear here automatically.
            </div>
          </div>
        </div>
      `;
    }

    return html`
      <div class="dashboard">
        <div class="dashboard-header">
          <div>
            <div class="dashboard-title">Training Dashboard</div>
            <div class="dashboard-subtitle">
              ${activeSessions.length > 0
                ? `${activeSessions.length} active session${activeSessions.length > 1 ? 's' : ''}`
                : `${this._historicalRuns.length} historical runs`}
            </div>
          </div>
        </div>

        ${this._academySessions.length > 0 ? this._renderAcademySessions() : nothing}
        ${activeSessions.length > 0 ? this._renderActiveSessions(activeSessions) : nothing}
        ${selected && selected.steps.length > 0 ? this._renderCharts(selected) : nothing}
        ${this._historicalRuns.length > 0 ? this._renderHistoricalTable() : nothing}
      </div>
    `;
  }

  // ── Academy sessions ─────────────────────────────────────────────────────

  private _renderAcademySessions(): TemplateResult {
    const active = this._academySessions.filter(s => !['completed', 'failed', 'cancelled'].includes(s.status));
    const completed = this._academySessions.filter(s => ['completed', 'failed', 'cancelled'].includes(s.status));

    return html`
      <div class="active-section">
        <div class="section-label">Academy Sessions${active.length > 0 ? ` (${active.length} active)` : ''}</div>
        ${active.map(s => this._renderAcademyCard(s, true))}
        ${completed.slice(0, 5).map(s => this._renderAcademyCard(s, false))}
      </div>
    `;
  }

  private _renderAcademyCard(session: AcademySession, isActive: boolean): TemplateResult {
    const statusColors: Record<string, string> = {
      curriculum: 'rgba(0, 212, 255, 0.9)',
      teaching: 'rgba(0, 255, 200, 0.9)',
      examining: 'rgba(255, 170, 0, 0.9)',
      training: 'rgba(255, 107, 53, 0.9)',
      completed: 'var(--content-success, #00ff88)',
      failed: 'var(--content-error, #ff5050)',
    };
    const statusColor = statusColors[session.status] ?? 'var(--content-secondary)';
    const elapsed = Math.round((Date.now() - new Date(session.createdAt).getTime()) / 1000);
    const elapsedStr = elapsed > 3600 ? `${(elapsed / 3600).toFixed(1)}h` : elapsed > 60 ? `${Math.round(elapsed / 60)}m` : `${elapsed}s`;

    return html`
      <div class="active-card" style="${isActive ? '' : 'opacity: 0.6;'}">
        <div class="card-header">
          <span class="card-title" style="display: flex; align-items: center; gap: 8px;">
            ${session.skill}
            <span style="font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 3px; background: ${statusColor}22; color: ${statusColor}; border: 1px solid ${statusColor}44;">
              ${session.status}
            </span>
          </span>
          <span class="card-persona">
            ${session.personaName}${session.nodeName ? html` <span style="color: rgba(0, 212, 255, 0.7);">@ ${session.nodeName}</span>` : nothing}
          </span>
        </div>
        <div class="card-stats">
          <span>Model: <span class="stat-value">${this._shortModel(session.baseModel)}</span></span>
          <span>Mode: <span class="stat-value">${session.mode}</span></span>
          <span>Elapsed: <span class="stat-value">${elapsedStr}</span></span>
          ${session.examinations?.length ? html`<span>Exams: <span class="stat-value">${session.examinations.length}</span></span>` : nothing}
          ${session.adapterIds?.length ? html`<span>Adapters: <span class="stat-value">${session.adapterIds.length}</span></span>` : nothing}
        </div>
      </div>
    `;
  }

  // ── Active sessions ─────────────────────────────────────────────────────

  private _renderActiveSessions(sessions: ActiveSession[]): TemplateResult {
    return html`
      <div class="active-section">
        <div class="section-label">Active Training</div>
        ${sessions.map(s => this._renderActiveCard(s))}
      </div>
    `;
  }

  private _renderActiveCard(session: ActiveSession): TemplateResult {
    const elapsed = Math.round((Date.now() - session.startedAt) / 1000);
    const isSelected = this._selectedSessionId === session.personaId;
    const stepCount = session.steps.length;

    return html`
      <div class="active-card"
        style="${isSelected ? 'border-color: rgba(0, 212, 255, 0.5);' : ''} cursor: pointer;"
        @click=${() => { this._selectedSessionId = session.personaId; }}>
        <div class="card-header">
          <span class="card-title">${session.domain}</span>
          <span class="card-persona">${session.personaName} / ${session.provider}</span>
        </div>
        <div class="card-stats">
          <span>Loss: <span class="stat-value">${session.latestLoss.toFixed(4)}</span></span>
          <span>Step: <span class="stat-value">${stepCount}</span></span>
          ${session.latestEpoch != null ? html`
            <span>Epoch: <span class="stat-value">${session.latestEpoch.toFixed(1)}</span></span>
          ` : nothing}
          <span>Time: <span class="stat-value">${elapsed}s</span></span>
          <span>Examples: <span class="stat-value">${session.exampleCount}</span></span>
        </div>
        ${stepCount > 2 ? html`
          <continuum-chart
            .data=${session.steps}
            .series=${LOSS_SERIES}
            .xKey=${'step'}
            .size=${'sparkline'}
            .streaming=${true}
            .yRange=${[0, 'auto'] as [number, 'auto']}
            .formatY=${(v: number) => v.toFixed(3)}
          ></continuum-chart>
        ` : nothing}
      </div>
    `;
  }

  // ── Charts grid ─────────────────────────────────────────────────────────

  private _renderCharts(session: ActiveSession): TemplateResult {
    const steps = session.steps;
    const hasAccuracy = steps.some(s => s.tokenAccuracy != null);
    const hasMemory = steps.some(s => s.memoryMb != null);
    const latestStep = steps[steps.length - 1];

    return html`
      <div class="section-label">Charts — ${session.domain} (${session.personaName})</div>
      <div class="chart-grid">

        <!-- Loss curve (wide) -->
        <div class="chart-panel wide">
          <div class="chart-title">Training Loss</div>
          <div class="chart-value-large">${latestStep.loss.toFixed(4)}</div>
          <div class="chart-value-sub">Step ${latestStep.step}${latestStep.epoch != null ? ` / Epoch ${latestStep.epoch.toFixed(1)}` : ''}</div>
          <continuum-chart
            .data=${steps}
            .series=${LOSS_SERIES}
            .xKey=${'step'}
            .size=${'large'}
            .streaming=${true}
            .yRange=${[0, 'auto'] as [number, 'auto']}
            .formatY=${(v: number) => v.toFixed(3)}
            .formatX=${(v: number) => `${v}`}
          ></continuum-chart>
        </div>

        <!-- Learning rate -->
        <div class="chart-panel">
          <div class="chart-title">Learning Rate</div>
          <div class="chart-value-large" style="color: rgba(0, 212, 255, 0.9);">${latestStep.learningRate.toExponential(2)}</div>
          <continuum-chart
            .data=${steps}
            .series=${LR_SERIES}
            .xKey=${'step'}
            .size=${'medium'}
            .streaming=${true}
            .yRange=${[0, 'auto'] as [number, 'auto']}
            .formatY=${(v: number) => v.toExponential(1)}
            .formatX=${(v: number) => `${v}`}
          ></continuum-chart>
        </div>

        <!-- Token accuracy (if available) -->
        ${hasAccuracy ? html`
          <div class="chart-panel">
            <div class="chart-title">Token Accuracy</div>
            <div class="chart-value-large" style="color: #4ade80;">
              ${((latestStep.tokenAccuracy ?? 0) * 100).toFixed(1)}%
            </div>
            <continuum-chart
              .data=${steps.filter(s => s.tokenAccuracy != null)}
              .series=${ACCURACY_SERIES}
              .xKey=${'step'}
              .size=${'medium'}
              .streaming=${true}
              .yRange=${[0, 1] as [number, number]}
              .formatY=${(v: number) => `${(v * 100).toFixed(0)}%`}
              .formatX=${(v: number) => `${v}`}
            ></continuum-chart>
          </div>
        ` : nothing}

        <!-- GPU memory (if available) -->
        ${hasMemory ? html`
          <div class="chart-panel">
            <div class="chart-title">GPU Memory</div>
            <div class="chart-value-large" style="color: #ff6b6b;">
              ${latestStep.memoryMb != null ? `${(latestStep.memoryMb / 1024).toFixed(1)} GB` : '--'}
            </div>
            <continuum-chart
              .data=${steps.filter(s => s.memoryMb != null)}
              .series=${MEMORY_SERIES}
              .xKey=${'step'}
              .size=${'medium'}
              .streaming=${true}
              .yRange=${[0, 'auto'] as [number, 'auto']}
              .formatY=${(v: number) => `${(v / 1024).toFixed(1)}G`}
              .formatX=${(v: number) => `${v}`}
            ></continuum-chart>
          </div>
        ` : nothing}

      </div>
    `;
  }

  // ── Historical runs table ───────────────────────────────────────────────

  private _renderHistoricalTable(): TemplateResult {
    return html`
      <div class="history-section">
        <div class="section-label">Historical Runs</div>
        <table class="history-table">
          <thead>
            <tr>
              <th>Adapter</th>
              <th>Persona</th>
              <th>Node</th>
              <th>Domain</th>
              <th>Base Model</th>
              <th>Loss</th>
              <th>Epochs</th>
              <th>Examples</th>
              <th>Maturity</th>
              <th>Loss Curve</th>
            </tr>
          </thead>
          <tbody>
            ${this._historicalRuns.map(run => html`
              <tr>
                <td>${run.name}</td>
                <td style="font-size: 10px;">${run.personaName ?? '--'}</td>
                <td style="font-size: 10px; color: ${run.nodeName ? 'rgba(0, 212, 255, 0.8)' : 'var(--content-secondary)'};">
                  ${run.nodeName ?? 'local'}
                </td>
                <td>${run.domain}</td>
                <td style="font-size: 10px; font-family: var(--font-mono, monospace);">${this._shortModel(run.baseModel)}</td>
                <td><span class="loss-badge">${run.finalLoss.toFixed(4)}</span></td>
                <td>${run.epochs}</td>
                <td>${run.examplesProcessed}</td>
                <td>
                  <div class="maturity-bar">
                    <div class="maturity-fill" style="width: ${run.maturity * 100}%;"></div>
                  </div>
                  <span style="font-size: 10px; margin-left: 4px;">${(run.maturity * 100).toFixed(0)}%</span>
                </td>
                <td>
                  ${run.lossHistory.length > 2 ? html`
                    <continuum-chart
                      class="mini-sparkline"
                      .data=${run.lossHistory.map((loss, i) => ({ step: i, loss }))}
                      .series=${LOSS_SERIES}
                      .xKey=${'step'}
                      .size=${'sparkline'}
                      .yRange=${[0, 'auto'] as [number, 'auto']}
                    ></continuum-chart>
                  ` : html`<span style="color: var(--content-secondary);">--</span>`}
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  // ── Utilities ───────────────────────────────────────────────────────────

  private _shortModel(model: string): string {
    // "HuggingFaceTB/SmolLM2-135M-Instruct" → "SmolLM2-135M"
    const parts = model.split('/');
    const name = parts[parts.length - 1];
    return name.replace(/-Instruct$/, '').replace(/-Chat$/, '');
  }
}

// ── Register ────────────────────────────────────────────────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('training-dashboard-widget')) {
  customElements.define('training-dashboard-widget', TrainingDashboardWidget);
}
