/**
 * TrainingDashboardWidget — Dense grid of training charts
 *
 * ONE command (genome/training-overview) gets ALL data.
 * Main area is a responsive grid of charts. Nothing else.
 * Session status goes in the right panel (TrainingStatusSection).
 */

import {
  ReactiveWidget,
  html,
  svg,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { nothing } from 'lit';
import '../shared/ContinuumChart';
import type { ContinuumChartSeries } from '../shared/ContinuumChart';

// ── Types ───────────────────────────────────────────────────────────────

interface OverviewData {
  adapters: any[];
  sessions: any[];
  nodes: any[];
  summary: {
    totalAdapters: number;
    totalSessions: number;
    activeSessions: number;
    bestLoss: number;
    avgMaturity: number;
  };
}

const LOSS_SERIES: ContinuumChartSeries[] = [
  { key: 'loss', color: 'rgba(0, 255, 200, 0.9)', label: 'Loss' },
];

const MATURITY_SERIES: ContinuumChartSeries[] = [
  { key: 'maturity', color: 'rgba(0, 212, 255, 0.9)', label: 'Maturity' },
];

// ── Component ───────────────────────────────────────────────────────────

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
      }

      .dashboard {
        padding: 20px 24px;
      }

      .header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
        margin-bottom: 20px;
      }

      .title { font-size: 20px; font-weight: 700; }

      .stats {
        display: flex;
        gap: 20px;
        font-size: 12px;
        color: var(--content-secondary, #8a92a5);
      }

      .stat-value {
        font-family: var(--font-mono, monospace);
        font-weight: 700;
        color: rgba(0, 255, 200, 0.9);
      }

      /* Chart grid */
      .chart-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 14px;
        margin-bottom: 20px;
      }

      .chart-cell {
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
        border-radius: 6px;
        padding: 12px;
        min-height: 140px;
        cursor: pointer;
        transition: border-color 0.15s ease;
      }

      .chart-cell:hover {
        border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      }

      .chart-cell.expanded {
        grid-column: 1 / -1;
        min-height: 250px;
      }

      .cell-title {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        margin-bottom: 4px;
      }

      .cell-value {
        font-size: 20px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        color: rgba(0, 255, 200, 0.9);
        margin-bottom: 6px;
      }

      .cell-value.cyan { color: rgba(0, 212, 255, 0.9); }
      .cell-value.dim { color: var(--content-secondary, #8a92a5); font-size: 14px; }

      .cell-sub {
        font-size: 10px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 8px;
      }

      /* Runs table */
      .section-label {
        font-size: 10px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.8px;
        margin: 24px 0 10px;
      }

      .runs-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 11px;
      }

      .runs-table th {
        text-align: left;
        padding: 6px 8px;
        font-weight: 700;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        font-size: 9px;
        border-bottom: 1px solid var(--border-subtle, rgba(255, 255, 255, 0.1));
      }

      .runs-table td {
        padding: 6px 8px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.04);
      }

      .runs-table tr:hover td { background: rgba(0, 212, 255, 0.04); }

      .loss-badge {
        font-family: var(--font-mono, monospace);
        font-weight: 600;
        color: rgba(0, 255, 200, 0.9);
      }

      .node-badge { font-size: 10px; color: rgba(0, 212, 255, 0.8); }

      .sparkline-cell { width: 100px; height: 28px; }

      .empty-state {
        text-align: center;
        padding: 60px 20px;
        color: var(--content-secondary, #8a92a5);
      }

      .empty-state .title { font-size: 16px; font-weight: 600; margin-bottom: 8px; color: var(--content-secondary); }
      .empty-state .hint { font-size: 12px; font-style: italic; }
    `,
  ] as CSSResultGroup;

  @reactive() private _data: OverviewData | null = null;
  @reactive() private _expandedChart: string | null = null;
  @reactive() private _loading = true;

  constructor() {
    super({ widgetName: 'TrainingDashboardWidget' });
  }

  protected override onFirstRender(): void {
    super.onFirstRender();
    this._load();
  }

  private async _load(): Promise<void> {
    this._loading = true;
    try {
      const result = await this.executeCommand<any, any>('genome/training-overview', {});
      if (result.success) {
        this._data = {
          adapters: result.adapters ?? [],
          sessions: result.sessions ?? [],
          nodes: result.nodes ?? [],
          summary: result.summary ?? { totalAdapters: 0, totalSessions: 0, activeSessions: 0, bestLoss: 0, avgMaturity: 0 },
        };
      }
    } catch (err) {
      console.warn('[TrainingDashboard] Failed to load:', err);
    }
    this._loading = false;
  }

  protected override renderContent(): TemplateResult {
    if (this._loading) {
      return html`<div class="dashboard"><div class="empty-state"><div class="hint">Loading training data from grid...</div></div></div>`;
    }

    const d = this._data;
    if (!d || (d.adapters.length === 0 && d.sessions.length === 0)) {
      return html`
        <div class="dashboard">
          <div class="empty-state">
            <div class="title">No Training Data</div>
            <div class="hint">Start an Academy session or run genome/train. Data from all grid nodes appears automatically.</div>
          </div>
        </div>
      `;
    }

    return html`
      <div class="dashboard">
        <div class="header">
          <span class="title">Training</span>
          <div class="stats">
            <span><span class="stat-value">${d.summary.totalAdapters}</span> adapters</span>
            <span><span class="stat-value">${d.summary.activeSessions}</span> active</span>
            <span>best loss <span class="stat-value">${d.summary.bestLoss > 0 ? d.summary.bestLoss.toFixed(4) : '--'}</span></span>
            <span>avg maturity <span class="stat-value">${(d.summary.avgMaturity * 100).toFixed(0)}%</span></span>
            <span><span class="stat-value">${d.nodes.length}</span> nodes</span>
          </div>
        </div>

        <div class="chart-grid">
          ${this._renderLossOverview(d)}
          ${this._renderBestAdapters(d)}
          ${this._renderMaturityOverview(d)}
          ${this._renderTrainingTime(d)}
          ${this._renderAdaptersByNode(d)}
          ${this._renderAdaptersByDomain(d)}
          ${this._renderEpochsDistribution(d)}
          ${this._renderExamplesProcessed(d)}
        </div>

        ${d.adapters.length > 0 ? this._renderRunsTable(d) : nothing}
      </div>
    `;
  }

  // ── Chart cells ─────────────────────────────────────────────────────────

  private _renderLossOverview(d: OverviewData): TemplateResult {
    // All adapters' loss histories overlaid
    const withHistory = d.adapters.filter(a => a.lossHistory?.length > 2);
    if (withHistory.length === 0) return this._emptyCell('Loss Curves', 'No loss data yet');

    const expanded = this._expandedChart === 'loss';
    const best = withHistory.reduce((a, b) => a.finalLoss < b.finalLoss ? a : b);

    return html`
      <div class="chart-cell ${expanded ? 'expanded' : ''}" @click=${() => this._toggleExpand('loss')}>
        <div class="cell-title">Loss Curves (${withHistory.length} adapters)</div>
        <div class="cell-value">${best.finalLoss.toFixed(4)}</div>
        <div class="cell-sub">Best: ${best.name} (${best.nodeName})</div>
        <continuum-chart
          .data=${best.lossHistory.map((loss: number, i: number) => ({ step: i, loss }))}
          .series=${LOSS_SERIES}
          .xKey=${'step'}
          .size=${expanded ? 'full' : 'medium'}
          .yRange=${[0, 'auto'] as [number, 'auto']}
          .formatY=${(v: number) => v.toFixed(2)}
          .formatX=${(v: number) => `${Math.round(v)}`}
        ></continuum-chart>
      </div>
    `;
  }

  private _renderBestAdapters(d: OverviewData): TemplateResult {
    const sorted = [...d.adapters].filter(a => a.finalLoss > 0).sort((a, b) => a.finalLoss - b.finalLoss);
    if (sorted.length === 0) return this._emptyCell('Best Adapters', 'No trained adapters');

    return html`
      <div class="chart-cell">
        <div class="cell-title">Top Adapters by Loss</div>
        ${sorted.slice(0, 5).map((a, i) => html`
          <div style="display: flex; justify-content: space-between; padding: 3px 0; font-size: 11px; ${i === 0 ? 'font-weight: 700;' : ''}">
            <span style="color: ${i === 0 ? 'rgba(0, 255, 200, 0.9)' : 'var(--content-primary)'};">${a.name}</span>
            <span class="loss-badge">${a.finalLoss.toFixed(4)}</span>
          </div>
        `)}
      </div>
    `;
  }

  private _renderMaturityOverview(d: OverviewData): TemplateResult {
    const withMaturity = d.adapters.filter(a => a.maturity > 0);
    if (withMaturity.length === 0) return this._emptyCell('Maturity', 'No maturity data');

    const data = withMaturity.map((a, i) => ({ step: i, maturity: a.maturity }));
    return html`
      <div class="chart-cell">
        <div class="cell-title">Adapter Maturity</div>
        <div class="cell-value cyan">${(d.summary.avgMaturity * 100).toFixed(0)}%</div>
        <div class="cell-sub">Average across ${withMaturity.length} adapters</div>
        <continuum-chart
          .data=${data}
          .series=${MATURITY_SERIES}
          .xKey=${'step'}
          .size=${'medium'}
          .yRange=${[0, 1] as [number, number]}
          .formatY=${(v: number) => `${(v * 100).toFixed(0)}%`}
        ></continuum-chart>
      </div>
    `;
  }

  private _renderTrainingTime(d: OverviewData): TemplateResult {
    const withTime = d.adapters.filter(a => a.trainingDurationMs > 0);
    if (withTime.length === 0) return this._emptyCell('Training Time', 'No timing data');

    const totalMs = withTime.reduce((s, a) => s + a.trainingDurationMs, 0);
    const totalMin = totalMs / 60000;

    return html`
      <div class="chart-cell">
        <div class="cell-title">Training Time</div>
        <div class="cell-value cyan">${totalMin > 60 ? `${(totalMin / 60).toFixed(1)}h` : `${totalMin.toFixed(0)}m`}</div>
        <div class="cell-sub">Total across ${withTime.length} runs</div>
        ${withTime.slice(0, 4).map(a => html`
          <div style="display: flex; justify-content: space-between; padding: 2px 0; font-size: 10px;">
            <span>${a.name}</span>
            <span style="font-family: var(--font-mono); color: var(--content-primary);">${(a.trainingDurationMs / 1000).toFixed(0)}s</span>
          </div>
        `)}
      </div>
    `;
  }

  private _renderAdaptersByNode(d: OverviewData): TemplateResult {
    if (d.nodes.length <= 1) return this._emptyCell('Grid Nodes', 'Single node');

    return html`
      <div class="chart-cell">
        <div class="cell-title">Adapters by Node</div>
        ${d.nodes.filter(n => n.adapterCount > 0).map(n => html`
          <div style="display: flex; justify-content: space-between; padding: 3px 0; font-size: 11px;">
            <span>
              <span class="node-badge">${n.nodeName}</span>
              ${n.gpu ? html`<span style="font-size: 9px; color: var(--content-secondary);"> ${n.gpu}</span>` : nothing}
            </span>
            <span class="stat-value" style="font-size: 13px;">${n.adapterCount}</span>
          </div>
        `)}
      </div>
    `;
  }

  private _renderAdaptersByDomain(d: OverviewData): TemplateResult {
    const domains = new Map<string, number>();
    for (const a of d.adapters) {
      domains.set(a.domain, (domains.get(a.domain) ?? 0) + 1);
    }
    if (domains.size === 0) return this._emptyCell('Domains', 'No adapters');

    return html`
      <div class="chart-cell">
        <div class="cell-title">Adapters by Domain</div>
        ${[...domains.entries()].sort((a, b) => b[1] - a[1]).map(([domain, count]) => html`
          <div style="display: flex; justify-content: space-between; padding: 3px 0; font-size: 11px;">
            <span style="color: rgba(0, 255, 200, 0.8);">${domain}</span>
            <span class="stat-value" style="font-size: 13px;">${count}</span>
          </div>
        `)}
      </div>
    `;
  }

  private _renderEpochsDistribution(d: OverviewData): TemplateResult {
    const withEpochs = d.adapters.filter(a => a.epochs > 0);
    if (withEpochs.length === 0) return this._emptyCell('Epochs', 'No data');

    const totalEpochs = withEpochs.reduce((s, a) => s + a.epochs, 0);
    const avgEpochs = totalEpochs / withEpochs.length;

    return html`
      <div class="chart-cell">
        <div class="cell-title">Epochs</div>
        <div class="cell-value cyan">${avgEpochs.toFixed(1)}</div>
        <div class="cell-sub">Average · ${totalEpochs} total across ${withEpochs.length} runs</div>
      </div>
    `;
  }

  private _renderExamplesProcessed(d: OverviewData): TemplateResult {
    const withExamples = d.adapters.filter(a => a.examplesProcessed > 0);
    if (withExamples.length === 0) return this._emptyCell('Examples', 'No data');

    const total = withExamples.reduce((s, a) => s + a.examplesProcessed, 0);

    return html`
      <div class="chart-cell">
        <div class="cell-title">Examples Processed</div>
        <div class="cell-value">${total}</div>
        <div class="cell-sub">Across ${withExamples.length} training runs</div>
      </div>
    `;
  }

  private _emptyCell(title: string, message: string): TemplateResult {
    return html`
      <div class="chart-cell">
        <div class="cell-title">${title}</div>
        <div class="cell-value dim">${message}</div>
      </div>
    `;
  }

  // ── Runs table ──────────────────────────────────────────────────────────

  private _renderRunsTable(d: OverviewData): TemplateResult {
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
            <th>Examples</th>
            <th>Loss Curve</th>
          </tr>
        </thead>
        <tbody>
          ${d.adapters.map(a => html`
            <tr>
              <td>${a.name}</td>
              <td style="font-size: 10px;">${a.personaName ?? '--'}</td>
              <td><span class="node-badge">${a.nodeName}</span></td>
              <td>${a.domain}</td>
              <td style="font-size: 10px; font-family: var(--font-mono);">${this._shortModel(a.baseModel)}</td>
              <td><span class="loss-badge">${a.finalLoss > 0 ? a.finalLoss.toFixed(4) : '--'}</span></td>
              <td>${a.epochs || '--'}</td>
              <td>${a.examplesProcessed || '--'}</td>
              <td>
                ${a.lossHistory?.length > 2 ? html`
                  <continuum-chart
                    class="sparkline-cell"
                    .data=${a.lossHistory.map((loss: number, i: number) => ({ step: i, loss }))}
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

  private _toggleExpand(id: string): void {
    this._expandedChart = this._expandedChart === id ? null : id;
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
