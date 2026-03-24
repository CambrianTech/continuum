/**
 * TrainingDashboardWidget — Full-tab training view
 *
 * Two sections:
 *   1. TensorBoard embed — real charts from real tfevents data
 *   2. Historical runs table — adapter inventory with loss, maturity, node info
 *
 * TensorBoard runs on the training node (BigMama:6006). This widget
 * embeds it via iframe. If TensorBoard isn't running, shows instructions.
 *
 * Academy session status lives in TrainingStatusSection (right panel sidebar),
 * not here — that's the contextual at-a-glance view.
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

interface GridNodeInfo {
  nodeId: string;
  nodeName: string;
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
        padding: 16px 20px;
        max-width: 1200px;
        margin: 0 auto;
      }

      .dashboard-title {
        font-size: 18px;
        font-weight: 700;
        margin-bottom: 4px;
      }

      .dashboard-subtitle {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 20px;
      }

      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin-bottom: 10px;
        margin-top: 24px;
      }

      /* TensorBoard embed */
      .tb-container {
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        overflow: hidden;
        margin-bottom: 20px;
      }

      .tb-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 8px 12px;
        background: rgba(15, 20, 25, 0.8);
        border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      }

      .tb-label {
        font-size: 11px;
        font-weight: 700;
        color: rgba(0, 212, 255, 0.9);
      }

      .tb-url-input {
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        border: 1px solid var(--input-border, rgba(255, 255, 255, 0.15));
        border-radius: 3px;
        color: var(--input-text, #fff);
        font-family: var(--font-mono, monospace);
        font-size: 10px;
        padding: 3px 6px;
        width: 250px;
      }

      .tb-connect-btn {
        padding: 3px 10px;
        font-size: 10px;
        font-weight: 600;
        border-radius: 3px;
        border: 1px solid rgba(0, 212, 255, 0.3);
        background: rgba(0, 212, 255, 0.08);
        color: rgba(0, 212, 255, 0.9);
        cursor: pointer;
      }

      .tb-connect-btn:hover {
        background: rgba(0, 212, 255, 0.15);
      }

      iframe.tb-frame {
        width: 100%;
        height: 500px;
        border: none;
        background: #1a1a2e;
      }

      .tb-placeholder {
        padding: 30px 20px;
        text-align: center;
        color: var(--content-secondary, #8a92a5);
        font-size: 12px;
      }

      .tb-placeholder code {
        display: block;
        margin: 8px auto;
        padding: 6px 12px;
        background: rgba(0, 0, 0, 0.3);
        border-radius: 4px;
        font-family: var(--font-mono, monospace);
        font-size: 11px;
        color: rgba(0, 255, 200, 0.8);
        max-width: 500px;
      }

      /* Historical runs table */
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
        padding: 20px;
        color: var(--content-secondary, #8a92a5);
        font-style: italic;
        font-size: 12px;
      }
    `,
  ] as CSSResultGroup;

  // ── State ───────────────────────────────────────────────────────────────

  @reactive() private _historicalRuns: HistoricalRun[] = [];
  @reactive() private _tbUrl: string = 'http://localhost:6006';
  @reactive() private _tbConnected: boolean = false;

  constructor() {
    super({ widgetName: 'TrainingDashboardWidget' });
  }

  protected override onFirstRender(): void {
    super.onFirstRender();
    this._loadHistoricalRuns();
  }

  // ── Data loading ────────────────────────────────────────────────────────

  private async _loadHistoricalRuns(): Promise<void> {
    const allRuns: HistoricalRun[] = [];
    await this._loadRunsFromLocal(allRuns);
    await this._loadRunsFromGrid(allRuns);
    allRuns.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    this._historicalRuns = allRuns;
  }

  private async _loadRunsFromLocal(allRuns: HistoricalRun[]): Promise<void> {
    try {
      const usersResult = await this.executeCommand<any, any>('data/list', {
        collection: 'users', filter: { type: 'ai' }, limit: 50,
      });
      if (!usersResult.success || !usersResult.items) return;
      for (const user of usersResult.items) {
        await this._loadPersonaLayers(allRuns, user.id, user.uniqueId ?? user.displayName, undefined);
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load local runs:', err);
    }
  }

  private async _loadRunsFromGrid(allRuns: HistoricalRun[]): Promise<void> {
    try {
      const nodesResult = await this.executeCommand<any, any>('grid/nodes', {});
      const nodes: GridNodeInfo[] = (nodesResult.nodes ?? []).map((n: any) => ({
        nodeId: n.node_id ?? n.nodeId,
        nodeName: n.node_name ?? n.nodeName ?? n.node_id ?? n.nodeId,
      }));
      for (const node of nodes) {
        try {
          const result = await this.executeCommand<any, any>('grid/send', {
            nodeId: node.nodeId, remoteCommand: 'genome/adapter-list', params: {},
          });
          // adapter-list gives us basic info; for full training metrics we need genome/layers per persona
          // but adapter-list is one call vs N calls per persona
          const remote = result?.remoteResult;
          if (remote?.adapters) {
            for (const a of remote.adapters) {
              if (a.loss != null) {
                allRuns.push({
                  name: a.name,
                  domain: a.domain,
                  baseModel: a.baseModel ?? '',
                  finalLoss: a.loss ?? 0,
                  epochs: a.epochs ?? 0,
                  examplesProcessed: 0,
                  maturity: 0,
                  createdAt: a.createdAt ?? '',
                  lossHistory: [],
                  nodeName: node.nodeName,
                  personaName: a.personaName,
                });
              }
            }
          }
        } catch (err) {
          console.warn(`[TrainingDashboard] Failed to load from ${node.nodeName}:`, err);
        }
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Grid not available:', err);
    }
  }

  private async _loadPersonaLayers(
    allRuns: HistoricalRun[], personaId: string, personaName: string, nodeName: string | undefined,
  ): Promise<void> {
    try {
      const result = await this.executeCommand<any, any>('genome/layers', { personaId, personaName });
      if (result.success && result.layers) {
        for (const layer of result.layers) {
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
    } catch (err) {
      console.warn(`[TrainingDashboard] Failed to load layers for ${personaName}:`, err);
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    return html`
      <div class="dashboard">
        <div class="dashboard-title">Training Dashboard</div>
        <div class="dashboard-subtitle">${this._historicalRuns.length} historical runs</div>

        ${this._renderTensorBoard()}
        ${this._historicalRuns.length > 0 ? this._renderHistoricalTable() : html`
          <div class="empty-state">No completed training runs yet.</div>
        `}
      </div>
    `;
  }

  // ── TensorBoard embed ───────────────────────────────────────────────────

  private _renderTensorBoard(): TemplateResult {
    return html`
      <div class="section-label">TensorBoard</div>
      <div class="tb-container">
        <div class="tb-header">
          <span class="tb-label">TensorBoard</span>
          <div style="display: flex; gap: 6px; align-items: center;">
            <input class="tb-url-input"
              .value=${this._tbUrl}
              @input=${(e: Event) => { this._tbUrl = (e.target as HTMLInputElement).value; }}
              placeholder="http://host:6006" />
            <button class="tb-connect-btn" @click=${() => { this._tbConnected = true; }}>
              ${this._tbConnected ? 'Refresh' : 'Connect'}
            </button>
          </div>
        </div>
        ${this._tbConnected ? html`
          <iframe class="tb-frame"
            src="${this._tbUrl}"
            sandbox="allow-scripts allow-same-origin allow-popups"
          ></iframe>
        ` : html`
          <div class="tb-placeholder">
            Start TensorBoard on your training node to see live charts:
            <code>tensorboard --logdir ~/.continuum/training/runs --bind_all --port 6006</code>
            Then click Connect above. Training data is written as tfevents by peft-train.py.
          </div>
        `}
      </div>
    `;
  }

  // ── Historical runs table ───────────────────────────────────────────────

  private _renderHistoricalTable(): TemplateResult {
    return html`
      <div class="section-label">Historical Runs</div>
      <table class="history-table">
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
            <tr>
              <td>${run.name}</td>
              <td style="font-size: 10px;">${run.personaName ?? '--'}</td>
              <td style="font-size: 10px; color: ${run.nodeName ? 'rgba(0, 212, 255, 0.8)' : 'var(--content-secondary)'};">
                ${run.nodeName ?? 'local'}
              </td>
              <td>${run.domain}</td>
              <td style="font-size: 10px; font-family: var(--font-mono);">${this._shortModel(run.baseModel)}</td>
              <td><span class="loss-badge">${run.finalLoss.toFixed(4)}</span></td>
              <td>${run.epochs}</td>
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
    `;
  }

  private _shortModel(model: string): string {
    const parts = model.split('/');
    const name = parts[parts.length - 1];
    return name.replace(/-Instruct$/, '').replace(/-Chat$/, '');
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('training-dashboard-widget')) {
  customElements.define('training-dashboard-widget', TrainingDashboardWidget);
}
