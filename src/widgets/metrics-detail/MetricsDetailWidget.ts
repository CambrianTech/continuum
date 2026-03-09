/**
 * MetricsDetailWidget - Full-tab system & AI metrics dashboard
 *
 * Opened by clicking the sidebar ContinuumMetricsWidget sparkline.
 * Displays large interactive charts, provider/model breakdowns,
 * latency percentiles, and token usage analytics.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { renderLargeChart, type LargeChartSeries } from '../shared/SparklineChart';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';
import { AIGenerationEntity } from '../../system/data/entities/AIGenerationEntity';
import { SystemMetrics, type SystemMetricsPoint } from '../../commands/system/metrics/shared/SystemMetricsTypes';
import { AICost, type AICostResult } from '../../commands/ai/cost/shared/AICostTypes';

// ── Types ────────────────────────────────────────────────────────────────────

type DetailTab = 'sys' | 'ai';


export class MetricsDetailWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow-y: auto;
        color: var(--content-primary, #e8eaed);
        font-family: var(--font-mono, monospace);
      }

      .detail-panel {
        padding: 24px;
        max-width: 1200px;
        margin: 0 auto;
      }

      .detail-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 24px;
        padding-bottom: 12px;
        border-bottom: 1px solid var(--border-primary, #2a2d35);
      }

      .detail-header h2 {
        margin: 0;
        font-size: 18px;
        font-weight: 600;
        letter-spacing: .5px;
      }

      .controls {
        display: flex;
        gap: 8px;
        align-items: center;
      }

      .tab-bar { display: flex; gap: 4px; }
      .tab-btn {
        padding: 6px 16px;
        font-size: 12px;
        font-weight: 600;
        font-family: var(--font-mono, monospace);
        text-transform: uppercase;
        letter-spacing: .5px;
        color: var(--content-tertiary, #6a7280);
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
        cursor: pointer;
        transition: all .15s ease;
      }
      .tab-btn:hover { color: var(--content-secondary, #8a92a5); }
      .tab-btn.active {
        color: var(--accent-primary, #4a9eff);
        border-color: var(--accent-primary, #4a9eff);
        background: rgba(74,158,255,.08);
      }

      .range-select {
        padding: 6px 12px;
        font-size: 12px;
        font-family: var(--font-mono, monospace);
        background: var(--surface-primary, #1a1d24);
        color: var(--content-primary, #e8eaed);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
        cursor: pointer;
      }

      /* ── Summary cards ────────────────────────────────────── */
      .summary-cards {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
        gap: 12px;
        margin-bottom: 24px;
      }

      .summary-card {
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 6px;
        padding: 12px 16px;
      }
      .summary-card .card-label {
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: .5px;
        color: var(--content-tertiary, #6a7280);
        margin-bottom: 4px;
      }
      .summary-card .card-value {
        font-size: 22px;
        font-weight: 700;
      }
      .summary-card .card-sub {
        font-size: 10px;
        color: var(--content-tertiary, #6a7280);
        margin-top: 2px;
      }

      /* ── Chart sections ────────────────────────────────────── */
      .chart-section {
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 6px;
        padding: 16px;
        margin-bottom: 16px;
      }
      .chart-section-title {
        font-size: 12px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: .5px;
        color: var(--content-secondary, #8a92a5);
        margin-bottom: 12px;
      }
      .chart-area {
        height: 160px;
        position: relative;
      }
      .chart-area svg { width: 100%; height: 100%; }

      .axis-label {
        position: absolute;
        font-size: 8px;
        color: rgba(255,255,255,0.35);
        pointer-events: none;
      }
      .axis-y {
        right: calc(100% - 40px);
        transform: translateY(-50%);
        text-align: right;
        white-space: nowrap;
      }
      .axis-x {
        bottom: 0;
        transform: translateX(-50%);
        white-space: nowrap;
      }

      .chart-legend {
        display: flex;
        gap: 16px;
        margin-top: 10px;
        flex-wrap: wrap;
      }
      .chart-legend-item {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 11px;
      }
      .chart-legend-dot {
        width: 10px;
        height: 10px;
        border-radius: 3px;
      }

      /* ── Breakdown tables ────────────────────────────────── */
      .breakdown-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 16px;
        margin-bottom: 16px;
      }
      @media (max-width: 800px) {
        .breakdown-grid { grid-template-columns: 1fr; }
      }

      .breakdown-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 12px;
      }
      .breakdown-table th {
        text-align: left;
        padding: 6px 8px;
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: .5px;
        color: var(--content-tertiary, #6a7280);
        border-bottom: 1px solid var(--border-secondary, #383b44);
        font-weight: 600;
      }
      .breakdown-table td {
        padding: 6px 8px;
        border-bottom: 1px solid rgba(42,45,53,0.5);
      }
      .breakdown-table td.num { text-align: right; font-variant-numeric: tabular-nums; }

      .pct-bar {
        height: 4px;
        border-radius: 2px;
        background: var(--accent-primary, #4a9eff);
        opacity: 0.6;
        margin-top: 2px;
      }

      /* ── Latency panel ────────────────────────────────────── */
      .latency-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(100px, 1fr));
        gap: 8px;
      }
      .latency-stat {
        text-align: center;
        padding: 10px;
        background: var(--surface-secondary, #0f1117);
        border-radius: 4px;
      }
      .latency-stat .stat-label {
        font-size: 10px;
        text-transform: uppercase;
        color: var(--content-tertiary, #6a7280);
      }
      .latency-stat .stat-value {
        font-size: 18px;
        font-weight: 700;
        margin-top: 4px;
      }

      .empty-detail {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 200px;
        color: var(--content-tertiary, #6a7280);
        font-size: 14px;
      }
    `
  ] as CSSResultGroup;

  @reactive() private _tab: DetailTab = 'sys';
  @reactive() private _timeRange: string = '1h';

  // System metrics
  @reactive() private _sysTimeSeries: SystemMetricsPoint[] = [];
  @reactive() private _sysCurrent = {
    cpuUsage: 0, memoryPressure: 0, memoryUsedMb: 0,
    memoryTotalMb: 0, gpuPressure: 0, gpuUsedMb: 0, gpuTotalMb: 0,
  };

  // AI metrics
  @reactive() private _aiResult: AICostResult | null = null;

  private _refreshTimer: ReturnType<typeof setInterval> | null = null;

  /** Set by content system when opening this widget via ContentService.open() */
  _metadata?: { initialTab?: DetailTab; timeRange?: string };

  constructor() {
    super({ widgetName: 'MetricsDetailWidget' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Read initial config from metadata if available
    if (this._metadata?.initialTab) this._tab = this._metadata.initialTab;
    if (this._metadata?.timeRange) this._timeRange = this._metadata.timeRange;

    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_DECISION_EVENTS.POSTED, () => {
          if (this._tab === 'ai') this._fetchAIData();
        }),
        Events.subscribe(`data:${AIGenerationEntity.collection}:created`, () => {
          if (this._tab === 'ai') this._fetchAIData();
        }),
      ];

      this._refreshTimer = setInterval(() => {
        if (this._tab === 'sys') this._fetchSysData();
        else this._fetchAIData();
      }, 30_000);

      return () => {
        unsubs.forEach(u => u());
        if (this._refreshTimer) clearInterval(this._refreshTimer);
      };
    });

    await Promise.all([this._fetchSysData(), this._fetchAIData()]);
  }

  // ── Main render ────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    return html`
      <div class="detail-panel">
        <div class="detail-header">
          <h2>${this._tab === 'sys' ? 'System Metrics' : 'AI Metrics'}</h2>
          <div class="controls">
            <div class="tab-bar">
              <button class="tab-btn ${this._tab === 'sys' ? 'active' : ''}"
                @click=${() => this._switchTab('sys')}>System</button>
              <button class="tab-btn ${this._tab === 'ai' ? 'active' : ''}"
                @click=${() => this._switchTab('ai')}>AI</button>
            </div>
            <select class="range-select" @change=${this._onTimeChange}>
              ${['1h', '6h', '24h', '7d', '30d'].map(t => html`
                <option value="${t}" ?selected=${this._timeRange === t}>${t}</option>
              `)}
            </select>
          </div>
        </div>

        ${this._tab === 'sys' ? this._renderSysDetail() : this._renderAIDetail()}
      </div>
    `;
  }

  // ── System detail ──────────────────────────────────────────────────────────

  private _renderSysDetail(): TemplateResult {
    const c = this._sysCurrent;

    return html`
      <!-- Summary cards -->
      <div class="summary-cards">
        ${this._card('CPU', `${(c.cpuUsage * 100).toFixed(0)}%`, '#ff6b6b')}
        ${this._card('Memory',
          c.memoryTotalMb > 0
            ? `${(c.memoryUsedMb / 1024).toFixed(1)}G`
            : `${(c.memoryPressure * 100).toFixed(0)}%`,
          '#4ade80',
          c.memoryTotalMb > 0 ? `of ${(c.memoryTotalMb / 1024).toFixed(0)}G` : undefined
        )}
        ${this._card('GPU',
          c.gpuTotalMb > 0
            ? `${(c.gpuUsedMb / 1024).toFixed(1)}G`
            : `${(c.gpuPressure * 100).toFixed(0)}%`,
          '#a78bfa',
          c.gpuTotalMb > 0 ? `of ${(c.gpuTotalMb / 1024).toFixed(0)}G` : undefined
        )}
        ${this._card('Samples', this._sysTimeSeries.length.toString(), '#6a7280')}
      </div>

      <!-- CPU / Memory / GPU chart -->
      ${this._renderLargeChart(
        'Resource Usage',
        this._sysTimeSeries,
        [
          { label: 'CPU', color: '#ff6b6b', getValue: p => p.cpuUsage, format: v => `${(v * 100).toFixed(0)}%` },
          { label: 'Memory', color: '#4ade80', getValue: p => p.memoryPressure, format: v => `${(v * 100).toFixed(0)}%` },
          { label: 'GPU', color: '#a78bfa', getValue: p => p.gpuPressure, format: v => `${(v * 100).toFixed(0)}%` },
        ],
        true
      )}

      ${this._sysTimeSeries.length > 0 && (c.memoryTotalMb > 0 || c.gpuTotalMb > 0) ? html`
        ${this._renderLargeChart(
          'Memory Usage (GB)',
          this._sysTimeSeries,
          [
            ...(c.memoryTotalMb > 0 ? [{ label: 'RAM', color: '#4ade80', getValue: (p: SystemMetricsPoint) => p.memoryUsedMb / 1024, format: (v: number) => `${v.toFixed(1)}G` }] : []),
            ...(c.gpuTotalMb > 0 ? [{ label: 'VRAM', color: '#a78bfa', getValue: (p: SystemMetricsPoint) => p.gpuUsedMb / 1024, format: (v: number) => `${v.toFixed(1)}G` }] : []),
          ],
          false
        )}
      ` : ''}
    `;
  }

  // ── AI detail ──────────────────────────────────────────────────────────────

  private _renderAIDetail(): TemplateResult {
    const r = this._aiResult;
    if (!r?.success) {
      return html`<div class="empty-detail">No AI generation data available</div>`;
    }

    const s = r.summary;
    const ts = r.timeSeries ?? [];

    return html`
      <!-- Summary cards -->
      <div class="summary-cards">
        ${this._card('Generations', s.totalGenerations.toString(), '#00d4ff')}
        ${this._card('Tokens', this._formatNumber(s.totalTokens), '#ff6b6b',
          `${this._formatNumber(s.inputTokens)} in / ${this._formatNumber(s.outputTokens)} out`
        )}
        ${this._card('Avg Latency', `${(s.avgResponseTime / 1000).toFixed(1)}s`, '#ffd700')}
        ${this._card('Cost', `$${s.totalCost.toFixed(4)}`, '#4ade80',
          s.totalGenerations > 0 ? `$${s.avgCostPerGeneration.toFixed(4)}/gen` : undefined
        )}
      </div>

      <!-- Time series charts -->
      ${ts.length > 0 ? html`
        ${this._renderLargeChart(
          'Generations Over Time',
          ts,
          [{ label: 'Generations', color: '#00d4ff', getValue: p => p.generations, format: v => v.toFixed(0) }],
          false
        )}

        ${this._renderLargeChart(
          'Token Usage Over Time',
          ts,
          [{ label: 'Tokens', color: '#ff6b6b', getValue: p => p.tokens, format: v => this._formatNumber(v) }],
          false
        )}

        ${this._renderLargeChart(
          'Latency & Cost Over Time',
          ts,
          [
            { label: 'Latency (s)', color: '#ffd700', getValue: p => p.avgResponseTime / 1000, format: v => `${v.toFixed(1)}s` },
            { label: 'Cost ($)', color: '#4ade80', getValue: p => p.cost, format: v => `$${v.toFixed(4)}` },
          ],
          false
        )}
      ` : ''}

      <!-- Breakdowns -->
      <div class="breakdown-grid">
        ${r.costByProvider ? this._renderProviderBreakdown(r.costByProvider) : ''}
        ${r.topModels?.length ? this._renderModelBreakdown(r.topModels) : ''}
      </div>

      <!-- Latency percentiles -->
      ${r.latency ? this._renderLatencyPanel(r.latency) : ''}
    `;
  }

  // ── Large chart renderer ───────────────────────────────────────────────────

  private _renderLargeChart<T extends { timestamp: string | number }>(
    title: string,
    data: T[],
    series: LargeChartSeries<T>[],
    percentScale: boolean
  ): TemplateResult {
    if (!data.length) return html``;

    const chart = renderLargeChart(
      data, series, percentScale,
      (ts) => this._formatTimeLabel(ts),
      (v) => this._formatAxisValue(v)
    );

    return html`
      <div class="chart-section">
        <div class="chart-section-title">${title}</div>
        <div class="chart-area">
          ${chart.yLabels.map(l => html`
            <span class="axis-label axis-y" style="top:${l.pct}%">${l.text}</span>
          `)}
          ${chart.xLabels.map(l => html`
            <span class="axis-label axis-x" style="left:${l.pct}%">${l.text}</span>
          `)}

          <svg viewBox="0 0 600 160" preserveAspectRatio="none">
            ${chart.svgContent}
          </svg>
        </div>
        <div class="chart-legend">
          ${series.map(s => {
            const values = data.map(s.getValue);
            const latest = values[values.length - 1] ?? 0;
            return html`
              <div class="chart-legend-item">
                <span class="chart-legend-dot" style="background:${s.color}"></span>
                <span>${s.label}: ${s.format(latest)}</span>
              </div>
            `;
          })}
        </div>
      </div>
    `;
  }

  // ── Breakdown panels ───────────────────────────────────────────────────────

  private _renderProviderBreakdown(byProvider: NonNullable<AICostResult['costByProvider']>): TemplateResult {
    const entries = Object.entries(byProvider).sort(
      (a, b) => b[1].cost - a[1].cost
    );

    return html`
      <div class="chart-section">
        <div class="chart-section-title">Cost by Provider</div>
        <table class="breakdown-table">
          <thead>
            <tr><th>Provider</th><th>Cost</th><th>Gens</th><th>%</th></tr>
          </thead>
          <tbody>
            ${entries.map(([name, stats]) => html`
              <tr>
                <td>${name}</td>
                <td class="num">$${stats.cost.toFixed(4)}</td>
                <td class="num">${stats.generations}</td>
                <td class="num">
                  ${stats.percentage.toFixed(1)}%
                  <div class="pct-bar" style="width:${Math.max(stats.percentage, 2)}%"></div>
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  private _renderModelBreakdown(topModels: NonNullable<AICostResult['topModels']>): TemplateResult {
    return html`
      <div class="chart-section">
        <div class="chart-section-title">Top Models by Cost</div>
        <table class="breakdown-table">
          <thead>
            <tr><th>Model</th><th>Provider</th><th>Cost</th><th>%</th></tr>
          </thead>
          <tbody>
            ${topModels.map((m) => html`
              <tr>
                <td>${this._truncateModel(m.model)}</td>
                <td>${m.provider}</td>
                <td class="num">$${m.cost.toFixed(4)}</td>
                <td class="num">
                  ${m.percentage.toFixed(1)}%
                  <div class="pct-bar" style="width:${Math.max(m.percentage, 2)}%"></div>
                </td>
              </tr>
            `)}
          </tbody>
        </table>
      </div>
    `;
  }

  private _renderLatencyPanel(lat: NonNullable<AICostResult['latency']>): TemplateResult {
    return html`
      <div class="chart-section">
        <div class="chart-section-title">Latency Distribution</div>
        <div class="latency-grid">
          ${this._latencyStat('Avg', lat.avgLatency)}
          ${this._latencyStat('P50', lat.p50)}
          ${this._latencyStat('P95', lat.p95)}
          ${this._latencyStat('P99', lat.p99)}
          ${this._latencyStat('Min', lat.minLatency)}
          ${this._latencyStat('Max', lat.maxLatency)}
        </div>
      </div>
    `;
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  private _card(label: string, value: string, color: string, sub?: string): TemplateResult {
    return html`
      <div class="summary-card">
        <div class="card-label">${label}</div>
        <div class="card-value" style="color:${color}">${value}</div>
        ${sub ? html`<div class="card-sub">${sub}</div>` : ''}
      </div>
    `;
  }

  private _latencyStat(label: string, ms: number): TemplateResult {
    const display = ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms.toFixed(0)}ms`;
    return html`
      <div class="latency-stat">
        <div class="stat-label">${label}</div>
        <div class="stat-value" style="color:#ffd700">${display}</div>
      </div>
    `;
  }

  private _formatNumber(n: number): string {
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
    if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
    return n.toString();
  }

  private _formatAxisValue(v: number): string {
    if (v >= 1e6) return `${(v / 1e6).toFixed(0)}M`;
    if (v >= 1e3) return `${(v / 1e3).toFixed(0)}K`;
    if (v >= 1) return v.toFixed(0);
    if (v >= 0.01) return v.toFixed(2);
    return v.toFixed(4);
  }

  private _formatTimeLabel(ts: string | number): string {
    if (typeof ts === 'number') {
      const d = new Date(ts);
      return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
    }
    if (typeof ts === 'string') {
      const d = new Date(ts);
      if (isNaN(d.getTime())) return '';
      return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`;
    }
    return '';
  }

  private _truncateModel(model: string): string {
    if (model.length <= 24) return model;
    return model.slice(0, 22) + '..';
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  private _switchTab(tab: DetailTab): void {
    this._tab = tab;
    if (tab === 'sys') this._fetchSysData();
    else this._fetchAIData();
  }

  private _onTimeChange = async (e: Event) => {
    this._timeRange = (e.target as HTMLSelectElement).value;
    if (this._tab === 'sys') await this._fetchSysData();
    else await this._fetchAIData();
  };

  // ── Data fetching ──────────────────────────────────────────────────────────

  private async _fetchSysData(): Promise<void> {
    try {
      const result = await SystemMetrics.execute({
        range: this._timeRange,
        maxPoints: 200,
      });

      if (result?.success) {
        this._sysTimeSeries = result.timeSeries ?? [];
        this._sysCurrent = result.current ?? this._sysCurrent;
        this.requestUpdate();
      }
    } catch {
      // Not available yet
    }
  }

  private async _fetchAIData(): Promise<void> {
    try {
      const intervalMap: Record<string, string> = {
        '1h': '5m', '6h': '30m', '24h': '1h', '7d': '6h', '30d': '1d'
      };

      const result = await AICost.execute({
        startTime: this._timeRange,
        includeTimeSeries: true,
        interval: intervalMap[this._timeRange] ?? '1h',
        includeBreakdown: true,
        includeTopModels: 10,
        includeLatency: true,
      });

      if (result?.success) {
        this._aiResult = result;
        this.requestUpdate();
      }
    } catch {
      // Not available yet
    }
  }
}
