/**
 * TrainingDashboardWidget — Full-tab training visibility
 *
 * Automatically discovers training data from ALL grid nodes.
 * No configuration, no URLs, no external tools. Just open it and see
 * what's training, what trained, and how well.
 *
 * Three sections:
 *   1. Active Academy sessions (from grid nodes, 30s poll)
 *   2. Loss curves for completed adapters (continuum-chart sparklines + large view)
 *   3. Historical runs table (adapter inventory across all nodes)
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
import '../shared/ContinuumChart';
import type { ContinuumChartSeries } from '../shared/ContinuumChart';

// ── Types ───────────────────────────────────────────────────────────────────

interface AcademySession {
  id: string;
  skill: string;
  status: string;
  personaName: string;
  baseModel: string;
  mode: string;
  createdAt: string;
  nodeName?: string;
}

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
  nodeName?: string;
  personaName?: string;
}

const LOSS_SERIES: ContinuumChartSeries[] = [
  { key: 'loss', color: 'rgba(0, 255, 200, 0.9)', label: 'Loss' },
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
        padding: 20px 24px;
      }

      .dashboard-title {
        font-size: 20px;
        font-weight: 700;
        margin-bottom: 4px;
      }

      .dashboard-subtitle {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 24px;
      }

      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin-bottom: 10px;
        margin-top: 28px;
      }

      .section-label:first-of-type { margin-top: 0; }

      /* Academy session cards */
      .session-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
        gap: 12px;
      }

      .session-card {
        background: rgba(0, 255, 200, 0.04);
        border: 1px solid rgba(0, 255, 200, 0.15);
        border-radius: 6px;
        padding: 12px 14px;
      }

      .session-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 6px;
      }

      .session-skill {
        font-weight: 700;
        font-size: 14px;
        color: rgba(0, 255, 200, 0.9);
      }

      .session-status {
        font-size: 10px;
        font-weight: 600;
        padding: 2px 8px;
        border-radius: 3px;
      }

      .session-meta {
        display: flex;
        gap: 16px;
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
      }

      .meta-value {
        color: rgba(0, 212, 255, 0.9);
        font-family: var(--font-mono, monospace);
        font-weight: 600;
      }

      /* Selected run detail */
      .detail-panel {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 16px;
        margin-bottom: 20px;
      }

      .detail-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 12px;
      }

      .detail-name {
        font-size: 16px;
        font-weight: 700;
        color: rgba(0, 255, 200, 0.9);
      }

      .detail-stats {
        display: flex;
        gap: 24px;
        margin-bottom: 16px;
      }

      .stat-block {
        display: flex;
        flex-direction: column;
        gap: 2px;
      }

      .stat-label {
        font-size: 9px;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--content-secondary, #8a92a5);
      }

      .stat-big {
        font-size: 24px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        color: rgba(0, 255, 200, 0.9);
      }

      .stat-big.cyan { color: rgba(0, 212, 255, 0.9); }

      /* Historical runs table */
      .runs-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 11px;
      }

      .runs-table th {
        text-align: left;
        padding: 8px 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        font-size: 9px;
        border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      }

      .runs-table td {
        padding: 8px 10px;
        color: var(--content-primary, #e0e6ed);
        border-bottom: 1px solid rgba(255, 255, 255, 0.04);
        cursor: pointer;
      }

      .runs-table tr:hover td {
        background: rgba(0, 212, 255, 0.04);
      }

      .runs-table tr.selected td {
        background: rgba(0, 255, 200, 0.06);
        border-color: rgba(0, 255, 200, 0.1);
      }

      .loss-badge {
        font-family: var(--font-mono, monospace);
        font-weight: 600;
        color: rgba(0, 255, 200, 0.9);
      }

      .node-badge {
        font-size: 10px;
        color: rgba(0, 212, 255, 0.8);
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

      .sparkline-cell {
        width: 100px;
        height: 30px;
      }

      .empty-state {
        text-align: center;
        padding: 40px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-title {
        font-size: 16px;
        font-weight: 600;
        margin-bottom: 8px;
      }

      .empty-hint {
        font-size: 12px;
        font-style: italic;
      }

      .loading-indicator {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        font-style: italic;
        padding: 8px 0;
      }
    `,
  ] as CSSResultGroup;

  // ── State ───────────────────────────────────────────────────────────────

  @reactive() private _academySessions: AcademySession[] = [];
  @reactive() private _historicalRuns: HistoricalRun[] = [];
  @reactive() private _selectedRun: HistoricalRun | null = null;
  @reactive() private _loading: boolean = true;

  private _pollTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    super({ widgetName: 'TrainingDashboardWidget' });
  }

  protected override onFirstRender(): void {
    super.onFirstRender();
    this._loadAll();

    this._pollTimer = setInterval(() => {
      if (this._academySessions.some(s => !['completed', 'failed', 'cancelled'].includes(s.status))) {
        this._loadAcademySessions();
      }
    }, 30_000);
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
  }

  private async _loadAll(): Promise<void> {
    this._loading = true;
    await Promise.all([
      this._loadAcademySessions(),
      this._loadHistoricalRuns(),
    ]);
    this._loading = false;
  }

  // ── Data loading ────────────────────────────────────────────────────────

  private async _loadAcademySessions(): Promise<void> {
    const sessions: AcademySession[] = [];

    // Local
    try {
      const r = await this.executeCommand<any, any>('genome/academy-session-list', {});
      if (r?.sessions) sessions.push(...r.sessions.map((s: any) => ({ ...s, nodeName: undefined })));
    } catch { /* ok */ }

    // Grid nodes
    try {
      const nodesResult = await this.executeCommand<any, any>('grid/nodes', {});
      for (const n of nodesResult?.nodes ?? []) {
        const nodeId = n.node_id ?? n.nodeId;
        const nodeName = n.node_name ?? n.nodeName ?? nodeId;
        try {
          const r = await this.executeCommand<any, any>('grid/send', {
            nodeId, remoteCommand: 'genome/academy-session-list', params: {},
          });
          if (r?.remoteResult?.sessions) {
            sessions.push(...r.remoteResult.sessions.map((s: any) => ({ ...s, nodeName })));
          }
        } catch { /* node unreachable */ }
      }
    } catch { /* grid unavailable */ }

    this._academySessions = sessions;
  }

  private async _loadHistoricalRuns(): Promise<void> {
    const runs: HistoricalRun[] = [];

    // Local personas
    try {
      const users = await this.executeCommand<any, any>('data/list', {
        collection: 'users', filter: { type: 'ai' }, limit: 50,
      });
      if (users?.items) {
        for (const u of users.items) {
          await this._loadLayers(runs, u.id, u.uniqueId ?? u.displayName, undefined);
        }
      }
    } catch (e) { console.warn('[TrainingDashboard] Local load failed:', e); }

    // Grid nodes — use genome/layers per persona for full loss history
    try {
      const nodesResult = await this.executeCommand<any, any>('grid/nodes', {});
      for (const n of nodesResult?.nodes ?? []) {
        const nodeId = n.node_id ?? n.nodeId;
        const nodeName = n.node_name ?? n.nodeName ?? nodeId;
        try {
          const usersResult = await this.executeCommand<any, any>('grid/send', {
            nodeId, remoteCommand: 'user/list', params: { limit: 50 },
          });
          const users = usersResult?.remoteResult?.users ?? [];
          for (const u of users) {
            if (u.type !== 'ai') continue;
            try {
              const lr = await this.executeCommand<any, any>('grid/send', {
                nodeId, remoteCommand: 'genome/layers',
                params: { personaId: u.id, personaName: u.uniqueId ?? u.displayName },
              });
              const layers = lr?.remoteResult?.layers ?? [];
              for (const l of layers) {
                if (l.trainingMetrics) {
                  runs.push({
                    name: l.name,
                    domain: l.domain,
                    baseModel: l.baseModel,
                    finalLoss: l.trainingMetrics.finalLoss ?? 0,
                    epochs: l.trainingMetrics.epochs ?? 0,
                    examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
                    maturity: l.maturity ?? 0,
                    createdAt: l.createdAt ?? '',
                    lossHistory: l.trainingMetrics.lossHistory ?? [],
                    nodeName,
                    personaName: u.uniqueId ?? u.displayName,
                  });
                }
              }
            } catch { /* skip persona */ }
          }
        } catch (e) { console.warn(`[TrainingDashboard] Grid node ${nodeName} failed:`, e); }
      }
    } catch { /* grid unavailable */ }

    runs.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    this._historicalRuns = runs;

    // Auto-select best run
    if (runs.length > 0 && !this._selectedRun) {
      const best = runs.reduce((a, b) => (a.lossHistory.length > b.lossHistory.length ? a : b));
      if (best.lossHistory.length > 2) this._selectedRun = best;
    }
  }

  private async _loadLayers(runs: HistoricalRun[], personaId: string, personaName: string, nodeName: string | undefined): Promise<void> {
    try {
      const r = await this.executeCommand<any, any>('genome/layers', { personaId, personaName });
      if (r?.layers) {
        for (const l of r.layers) {
          if (l.trainingMetrics) {
            runs.push({
              name: l.name, domain: l.domain, baseModel: l.baseModel,
              finalLoss: l.trainingMetrics.finalLoss ?? 0,
              epochs: l.trainingMetrics.epochs ?? 0,
              examplesProcessed: l.trainingMetrics.examplesProcessed ?? 0,
              maturity: l.maturity ?? 0,
              createdAt: l.createdAt ?? '',
              lossHistory: l.trainingMetrics.lossHistory ?? [],
              nodeName, personaName,
            });
          }
        }
      }
    } catch { /* skip */ }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    const activeSessions = this._academySessions.filter(s => !['completed', 'failed', 'cancelled'].includes(s.status));
    const hasAnything = this._academySessions.length > 0 || this._historicalRuns.length > 0;

    return html`
      <div class="dashboard">
        <div class="dashboard-title">Training</div>
        <div class="dashboard-subtitle">
          ${activeSessions.length > 0 ? `${activeSessions.length} active` : ''}
          ${activeSessions.length > 0 && this._historicalRuns.length > 0 ? ' · ' : ''}
          ${this._historicalRuns.length > 0 ? `${this._historicalRuns.length} completed runs` : ''}
          ${!hasAnything && !this._loading ? 'No training activity' : ''}
          ${this._loading ? 'Loading from grid...' : ''}
        </div>

        ${activeSessions.length > 0 ? this._renderActiveSessions(activeSessions) : nothing}
        ${this._selectedRun ? this._renderSelectedRun(this._selectedRun) : nothing}
        ${this._historicalRuns.length > 0 ? this._renderHistoricalTable() : nothing}
        ${!hasAnything && !this._loading ? this._renderEmpty() : nothing}
      </div>
    `;
  }

  private _renderActiveSessions(sessions: AcademySession[]): TemplateResult {
    const statusColors: Record<string, string> = {
      curriculum: 'rgba(0, 212, 255, 0.9)',
      teaching: 'rgba(0, 255, 200, 0.9)',
      examining: 'rgba(255, 170, 0, 0.9)',
      training: 'rgba(255, 107, 53, 0.9)',
    };

    return html`
      <div class="section-label">Active Sessions</div>
      <div class="session-grid">
        ${sessions.map(s => {
          const color = statusColors[s.status] ?? 'var(--content-secondary)';
          const elapsed = Math.round((Date.now() - new Date(s.createdAt).getTime()) / 1000);
          const elapsedStr = elapsed > 3600 ? `${(elapsed / 3600).toFixed(1)}h` : `${Math.round(elapsed / 60)}m`;
          return html`
            <div class="session-card">
              <div class="session-header">
                <span class="session-skill">${s.skill}</span>
                <span class="session-status" style="background: ${color}22; color: ${color}; border: 1px solid ${color}44;">
                  ${s.status}
                </span>
              </div>
              <div class="session-meta">
                <span>${s.personaName}${s.nodeName ? html` <span class="node-badge">@ ${s.nodeName}</span>` : nothing}</span>
                <span>Model: <span class="meta-value">${this._shortModel(s.baseModel)}</span></span>
                <span><span class="meta-value">${elapsedStr}</span></span>
              </div>
            </div>
          `;
        })}
      </div>
    `;
  }

  private _renderSelectedRun(run: HistoricalRun): TemplateResult {
    return html`
      <div class="section-label">Loss Curve — ${run.name}</div>
      <div class="detail-panel">
        <div class="detail-header">
          <span class="detail-name">${run.name}</span>
          <span class="node-badge">${run.nodeName ?? 'local'} · ${run.personaName ?? ''}</span>
        </div>
        <div class="detail-stats">
          <div class="stat-block">
            <span class="stat-label">Final Loss</span>
            <span class="stat-big">${run.finalLoss.toFixed(4)}</span>
          </div>
          <div class="stat-block">
            <span class="stat-label">Epochs</span>
            <span class="stat-big cyan">${run.epochs}</span>
          </div>
          <div class="stat-block">
            <span class="stat-label">Examples</span>
            <span class="stat-big cyan">${run.examplesProcessed}</span>
          </div>
          <div class="stat-block">
            <span class="stat-label">Model</span>
            <span style="font-size: 13px; font-family: var(--font-mono); color: var(--content-primary);">${this._shortModel(run.baseModel)}</span>
          </div>
        </div>
        <continuum-chart
          .data=${run.lossHistory.map((loss, i) => ({ step: i, loss }))}
          .series=${LOSS_SERIES}
          .xKey=${'step'}
          .size=${'full'}
          .yRange=${[0, 'auto'] as [number, 'auto']}
          .formatY=${(v: number) => v.toFixed(3)}
          .formatX=${(v: number) => `Step ${Math.round(v)}`}
        ></continuum-chart>
      </div>
    `;
  }

  private _renderHistoricalTable(): TemplateResult {
    return html`
      <div class="section-label">All Training Runs</div>
      <table class="runs-table">
        <thead>
          <tr>
            <th>Adapter</th>
            <th>Persona</th>
            <th>Node</th>
            <th>Domain</th>
            <th>Model</th>
            <th>Loss</th>
            <th>Epochs</th>
            <th>Maturity</th>
            <th>Loss Curve</th>
          </tr>
        </thead>
        <tbody>
          ${this._historicalRuns.map(run => html`
            <tr class="${this._selectedRun === run ? 'selected' : ''}"
                @click=${() => { this._selectedRun = run.lossHistory.length > 2 ? run : this._selectedRun; }}>
              <td>${run.name}</td>
              <td style="font-size: 10px;">${run.personaName ?? '--'}</td>
              <td><span class="node-badge">${run.nodeName ?? 'local'}</span></td>
              <td>${run.domain}</td>
              <td style="font-size: 10px; font-family: var(--font-mono);">${this._shortModel(run.baseModel)}</td>
              <td><span class="loss-badge">${run.finalLoss.toFixed(4)}</span></td>
              <td>${run.epochs}</td>
              <td>
                <div class="maturity-bar">
                  <div class="maturity-fill" style="width: ${run.maturity * 100}%;"></div>
                </div>
              </td>
              <td>
                ${run.lossHistory.length > 2 ? html`
                  <continuum-chart
                    class="sparkline-cell"
                    .data=${run.lossHistory.map((loss, i) => ({ step: i, loss }))}
                    .series=${LOSS_SERIES}
                    .xKey=${'step'}
                    .size=${'sparkline'}
                    .yRange=${[0, 'auto'] as [number, 'auto']}
                  ></continuum-chart>
                ` : html`<span style="color: var(--content-secondary); font-size: 10px;">--</span>`}
              </td>
            </tr>
          `)}
        </tbody>
      </table>
    `;
  }

  private _renderEmpty(): TemplateResult {
    return html`
      <div class="empty-state">
        <div class="empty-title">No Training Activity</div>
        <div class="empty-hint">
          Start an Academy session or run genome/train to begin training.
          Data from all grid nodes appears here automatically.
        </div>
      </div>
    `;
  }

  private _shortModel(model: string): string {
    if (!model) return '--';
    const parts = model.split('/');
    return parts[parts.length - 1].replace(/-Instruct$/, '').replace(/-Chat$/, '');
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('training-dashboard-widget')) {
  customElements.define('training-dashboard-widget', TrainingDashboardWidget);
}
