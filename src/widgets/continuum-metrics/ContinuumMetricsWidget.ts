/**
 * ContinuumMetrics Widget - System & AI Performance Dashboard
 *
 * Two tabs:
 *   SYS — CPU, GPU, Memory sparklines from system_metrics database
 *   AI  — Generations, Tokens, Latency, Cost from ai_generations
 *
 * Each tab has its own time range selector and auto-refreshes.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';
import { AIGenerationEntity } from '../../system/data/entities/AIGenerationEntity';
import { styles } from './public/continuum-metrics.styles';
import { SystemMetrics, type SystemMetricsPoint } from '../../commands/system/metrics/shared/SystemMetricsTypes';
import { AICost } from '../../commands/ai/cost/shared/AICostTypes';

// ── AI tab types ─────────────────────────────────────────────────────────────

interface AITimeSeriesPoint {
  timestamp: string;
  cost: number;
  generations: number;
  tokens: number;
  avgResponseTime: number;
}

// ── Tab type ─────────────────────────────────────────────────────────────────

type MetricsTab = 'sys' | 'ai';

export class ContinuumMetricsWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(styles)
  ] as CSSResultGroup;

  @reactive() private _activeTab: MetricsTab = 'sys';
  @reactive() private _timeRange: string = '1h';

  // System metrics state
  @reactive() private _sysTimeSeries: SystemMetricsPoint[] = [];
  @reactive() private _sysCurrent = {
    cpuUsage: 0, memoryPressure: 0, memoryUsedMb: 0,
    memoryTotalMb: 0, gpuPressure: 0, gpuUsedMb: 0, gpuTotalMb: 0,
  };

  // AI metrics state
  @reactive() private _aiTimeSeries: AITimeSeriesPoint[] = [];
  @reactive() private _aiSummary = { requests: 0, tokens: 0, latency: 0, cost: 0 };

  private _refreshTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    super({ widgetName: 'ContinuumMetricsWidget' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    this.createMountEffect(() => {
      const unsubs = [
        // Refresh AI tab on new generations
        Events.subscribe(AI_DECISION_EVENTS.POSTED, () => {
          if (this._activeTab === 'ai') this._fetchAIData();
        }),
        Events.subscribe(`data:${AIGenerationEntity.collection}:created`, () => {
          if (this._activeTab === 'ai') this._fetchAIData();
        }),
      ];

      // Auto-refresh system metrics every 30s
      this._refreshTimer = setInterval(() => {
        if (this._activeTab === 'sys') this._fetchSysData();
      }, 30_000);

      return () => {
        unsubs.forEach(u => u());
        if (this._refreshTimer) clearInterval(this._refreshTimer);
      };
    });

    // Initial data fetch for both tabs
    await Promise.all([
      this._fetchSysData(),
      this._fetchAIData(),
    ]);
  }

  // ── Rendering ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    return html`
      <div class="metrics-panel">
        <div class="metrics-header">
          <div class="tab-bar">
            <button class="tab ${this._activeTab === 'sys' ? 'active' : ''}"
              @click=${() => this._switchTab('sys')}>SYS</button>
            <button class="tab ${this._activeTab === 'ai' ? 'active' : ''}"
              @click=${() => this._switchTab('ai')}>AI</button>
          </div>
          <select class="time-select" @change=${this._onTimeChange}>
            ${['1h', '6h', '24h', '7d'].map(t => html`
              <option value="${t}" ?selected=${this._timeRange === t}>${t}</option>
            `)}
          </select>
        </div>

        <div class="chart-container">
          ${this._activeTab === 'sys' ? this._renderSysChart() : this._renderAIChart()}
        </div>

        <div class="legend">
          ${this._activeTab === 'sys' ? this._renderSysLegend() : this._renderAILegend()}
        </div>
      </div>
    `;
  }

  // ── System chart ───────────────────────────────────────────────────────────

  private _renderSysChart(): TemplateResult {
    if (!this._sysTimeSeries.length) {
      return html`<div class="empty-state">Collecting data...</div>`;
    }
    return html`
      <svg viewBox="0 0 200 80" preserveAspectRatio="none">
        <polyline stroke="#ff6b6b" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getSysPoints(p => p.cpuUsage)}" />
        <polyline stroke="#4ade80" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getSysPoints(p => p.memoryPressure)}" />
        <polyline stroke="#a78bfa" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getSysPoints(p => p.gpuPressure)}" />
      </svg>
    `;
  }

  private _renderSysLegend(): TemplateResult {
    const c = this._sysCurrent;
    return html`
      ${this._renderLegendItem('CPU', '#ff6b6b', `${(c.cpuUsage * 100).toFixed(0)}%`)}
      ${this._renderLegendItem('MEM', '#4ade80',
        c.memoryTotalMb > 0
          ? `${(c.memoryUsedMb / 1024).toFixed(1)}/${(c.memoryTotalMb / 1024).toFixed(0)}G`
          : `${(c.memoryPressure * 100).toFixed(0)}%`
      )}
      ${this._renderLegendItem('GPU', '#a78bfa',
        c.gpuTotalMb > 0
          ? `${(c.gpuUsedMb / 1024).toFixed(1)}/${(c.gpuTotalMb / 1024).toFixed(0)}G`
          : `${(c.gpuPressure * 100).toFixed(0)}%`
      )}
    `;
  }

  private _getSysPoints(getValue: (p: SystemMetricsPoint) => number): string {
    return this._getPathPoints(this._sysTimeSeries, getValue);
  }

  // ── AI chart ───────────────────────────────────────────────────────────────

  private _renderAIChart(): TemplateResult {
    if (!this._aiTimeSeries.length && this._aiSummary.requests === 0) {
      return html`<div class="empty-state">No data yet</div>`;
    }
    return html`
      <svg viewBox="0 0 200 80" preserveAspectRatio="none">
        <polyline stroke="#00d4ff" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getAIPoints(p => p.generations)}" />
        <polyline stroke="#ff6b6b" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getAIPoints(p => p.tokens / 1000)}" />
        <polyline stroke="#ffd700" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getAIPoints(p => p.avgResponseTime / 1000)}" />
        <polyline stroke="#4ade80" stroke-width="2" fill="none" opacity="0.8"
          points="${this._getAIPoints(p => p.cost * 100)}" />
      </svg>
    `;
  }

  private _renderAILegend(): TemplateResult {
    const s = this._aiSummary;
    return html`
      ${this._renderLegendItem('Req', '#00d4ff', s.requests.toString())}
      ${this._renderLegendItem('Tok', '#ff6b6b', this._formatNumber(s.tokens))}
      ${this._renderLegendItem('Lat', '#ffd700', `${(s.latency / 1000).toFixed(1)}s`)}
      ${this._renderLegendItem('Cost', '#4ade80', `$${s.cost.toFixed(2)}`)}
    `;
  }

  private _getAIPoints(getValue: (p: AITimeSeriesPoint) => number): string {
    return this._getPathPoints(this._aiTimeSeries, getValue);
  }

  // ── Shared rendering helpers ───────────────────────────────────────────────

  private _getPathPoints<T>(series: T[], getValue: (p: T) => number): string {
    if (!series.length) return '2,40 198,40';

    const values = series.map(getValue);
    const max = Math.max(...values, 0.001);
    const min = Math.min(...values, 0);
    const range = max - min || 1;

    return values.map((v, i) => {
      const x = (i / Math.max(values.length - 1, 1)) * 196 + 2;
      const y = 75 - ((v - min) / range) * 65;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(' ');
  }

  private _renderLegendItem(label: string, color: string, value: string): TemplateResult {
    return html`
      <div class="legend-item">
        <span class="dot" style="background:${color}"></span>
        <span class="label">${label}</span>
        <span class="value" style="color:${color}">${value}</span>
      </div>
    `;
  }

  private _formatNumber(n: number): string {
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
    if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
    return n.toString();
  }

  // ── Tab & time range ───────────────────────────────────────────────────────

  private _switchTab(tab: MetricsTab): void {
    this._activeTab = tab;
    // Refresh data for the newly active tab
    if (tab === 'sys') this._fetchSysData();
    else this._fetchAIData();
  }

  private _onTimeChange = async (e: Event) => {
    this._timeRange = (e.target as HTMLSelectElement).value;
    if (this._activeTab === 'sys') await this._fetchSysData();
    else await this._fetchAIData();
  };

  // ── Data fetching ──────────────────────────────────────────────────────────

  private async _fetchSysData(): Promise<void> {
    try {
      const result = await SystemMetrics.execute({
        range: this._timeRange,
        maxPoints: 120,
      });

      if (result?.success) {
        this._sysTimeSeries = result.timeSeries ?? [];
        this._sysCurrent = result.current ?? this._sysCurrent;
        this.requestUpdate();
      }
    } catch {
      // System metrics not yet available
    }
  }

  private async _fetchAIData(): Promise<void> {
    try {
      const result = await AICost.execute({
        startTime: this._timeRange,
        includeTimeSeries: true,
        interval: '1h',
        includeBreakdown: false,
      } as any) as any;

      if (result?.success && result.summary) {
        this._aiTimeSeries = result.timeSeries ?? [];
        this._aiSummary = {
          requests: result.summary.totalGenerations ?? 0,
          tokens: result.summary.totalTokens ?? 0,
          latency: result.summary.avgResponseTime ?? 0,
          cost: result.summary.totalCost ?? 0,
        };
        this.requestUpdate();
      }
    } catch {
      // AI cost data not available yet
    }
  }
}
