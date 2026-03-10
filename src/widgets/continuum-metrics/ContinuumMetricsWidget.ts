/**
 * ContinuumMetrics Widget - System & AI Performance Dashboard (Sidebar)
 *
 * Compact sidebar widget with two tabs:
 *   SYS — CPU, GPU, Memory sparklines from system_metrics database
 *   AI  — Generations, Tokens, Latency, Cost from ai_generations
 *
 * Click the chart area to open a full-tab detailed metrics view.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  css,
  type TemplateResult,
  type CSSResultGroup,
} from '../shared/ReactiveWidget';
import { renderSparkline, type ChartSeries } from '../shared/SparklineChart';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';
import { AIGenerationEntity } from '../../system/data/entities/AIGenerationEntity';
import { ContentService } from '../../system/state/ContentService';
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
    css`
      :host { display: block; width: 100%; }

      .metrics-panel {
        display: flex;
        flex-direction: column;
        padding: 10px;
        background: var(--surface-secondary, #0f1117);
        border: 1px solid var(--border-primary, #2a2d35);
        border-radius: 6px;
      }

      .metrics-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
        flex-shrink: 0;
      }

      .tab-bar { display: flex; gap: 2px; }

      .tab {
        padding: 2px 8px;
        font-size: 9px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        text-transform: uppercase;
        letter-spacing: .5px;
        color: var(--content-tertiary, #6a7280);
        background: none;
        border: 1px solid transparent;
        border-radius: 3px;
        cursor: pointer;
        transition: all .15s ease;
      }
      .tab:hover { color: var(--content-secondary, #8a92a5); background: var(--surface-primary, #1a1d24); }
      .tab.active { color: var(--accent-primary, #4a9eff); border-color: var(--accent-primary, #4a9eff); background: rgba(74,158,255,.08); }

      .time-select {
        padding: 3px 8px;
        font-size: 10px;
        font-family: var(--font-mono, monospace);
        background: var(--surface-primary, #1a1d24);
        color: var(--content-primary, #e8eaed);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 3px;
        cursor: pointer;
      }
      .time-select:hover { border-color: var(--accent-primary, #4a9eff); }

      .chart-container {
        height: 80px;
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
        padding: 6px;
        margin-bottom: 8px;
        position: relative;
        cursor: pointer;
        transition: border-color .2s ease;
      }
      .chart-container:hover {
        border-color: var(--accent-primary, #4a9eff);
      }
      .chart-container:hover .expand-hint { opacity: 1; }

      .chart-container svg { width: 100%; height: 100%; }

      .expand-hint {
        position: absolute;
        top: 4px;
        right: 6px;
        font-size: 9px;
        font-family: var(--font-mono, monospace);
        color: var(--accent-primary, #4a9eff);
        opacity: 0;
        transition: opacity .2s ease;
        pointer-events: none;
      }

      .empty-state {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 11px;
        font-family: var(--font-mono, monospace);
        color: var(--content-tertiary, #6a7280);
        letter-spacing: .3px;
      }

      .legend {
        display: flex;
        justify-content: space-between;
        gap: 6px;
        flex-shrink: 0;
        min-height: 20px;
      }

      .legend-item {
        display: flex;
        align-items: center;
        gap: 4px;
        font-family: var(--font-mono, monospace);
      }

      .dot { width: 8px; height: 8px; border-radius: 2px; }
      .label { font-size: 9px; color: var(--content-tertiary, #6a7280); text-transform: uppercase; }
      .value { font-size: 11px; font-weight: 600; }
    `
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
  @reactive() private _fetchError: string | null = null;

  private _refreshTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    super({ widgetName: 'ContinuumMetricsWidget' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_DECISION_EVENTS.POSTED, () => {
          if (this._activeTab === 'ai') this._fetchAIData();
        }),
        Events.subscribe(`data:${AIGenerationEntity.collection}:created`, () => {
          if (this._activeTab === 'ai') this._fetchAIData();
        }),
      ];

      this._refreshTimer = setInterval(() => {
        if (this._activeTab === 'sys') this._fetchSysData();
        else this._fetchAIData();
      }, 30_000);

      return () => {
        unsubs.forEach(u => u());
        if (this._refreshTimer) clearInterval(this._refreshTimer);
      };
    });

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

        <div class="chart-container" @click=${this._openDetailView}>
          <span class="expand-hint">expand</span>
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
    if (this._fetchError) {
      return html`<div class="empty-state">${this._fetchError}</div>`;
    }
    if (!this._sysTimeSeries.length) {
      return html`<div class="empty-state">Collecting data...</div>`;
    }

    const series: ChartSeries<SystemMetricsPoint>[] = [
      { color: '#ff6b6b', getValue: p => p.cpuUsage },
      { color: '#4ade80', getValue: p => p.memoryPressure },
      { color: '#a78bfa', getValue: p => p.gpuPressure },
    ];

    return html`<svg viewBox="0 0 200 80" preserveAspectRatio="none">
      ${renderSparkline(this._sysTimeSeries, series, false)}
    </svg>`;
  }

  private _renderSysLegend(): TemplateResult {
    const c = this._sysCurrent;
    return html`
      ${this._legendItem('CPU', '#ff6b6b', `${(c.cpuUsage * 100).toFixed(0)}%`)}
      ${this._legendItem('MEM', '#4ade80',
        c.memoryTotalMb > 0
          ? `${(c.memoryUsedMb / 1024).toFixed(1)}/${(c.memoryTotalMb / 1024).toFixed(0)}G`
          : `${(c.memoryPressure * 100).toFixed(0)}%`
      )}
      ${this._legendItem('GPU', '#a78bfa',
        c.gpuTotalMb > 0
          ? `${(c.gpuUsedMb / 1024).toFixed(1)}/${(c.gpuTotalMb / 1024).toFixed(0)}G`
          : `${(c.gpuPressure * 100).toFixed(0)}%`
      )}
    `;
  }

  // ── AI chart ───────────────────────────────────────────────────────────────

  private _renderAIChart(): TemplateResult {
    if (this._fetchError) {
      return html`<div class="empty-state">${this._fetchError}</div>`;
    }
    if (!this._aiTimeSeries.length && this._aiSummary.requests === 0) {
      return html`<div class="empty-state">No AI data yet</div>`;
    }

    if (!this._aiTimeSeries.length) {
      // Have summary but no time series — show summary-only sparkline
      return html`<div class="empty-state">
        ${this._aiSummary.requests} req / ${this._formatNumber(this._aiSummary.tokens)} tok
      </div>`;
    }

    const series: ChartSeries<AITimeSeriesPoint>[] = [
      { color: '#00d4ff', getValue: p => p.generations },
      { color: '#ff6b6b', getValue: p => p.tokens },
      { color: '#ffd700', getValue: p => p.avgResponseTime },
      { color: '#4ade80', getValue: p => p.cost },
    ];

    return html`<svg viewBox="0 0 200 80" preserveAspectRatio="none">
      ${renderSparkline(this._aiTimeSeries, series, false)}
    </svg>`;
  }

  private _renderAILegend(): TemplateResult {
    const s = this._aiSummary;
    return html`
      ${this._legendItem('Req', '#00d4ff', s.requests.toString())}
      ${this._legendItem('Tok', '#ff6b6b', this._formatNumber(s.tokens))}
      ${this._legendItem('Lat', '#ffd700', `${(s.latency / 1000).toFixed(1)}s`)}
      ${this._legendItem('$', '#4ade80', `$${s.cost.toFixed(2)}`)}
    `;
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  private _legendItem(label: string, color: string, value: string): TemplateResult {
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

  // ── Actions ────────────────────────────────────────────────────────────────

  private _switchTab(tab: MetricsTab): void {
    this._activeTab = tab;
    if (tab === 'sys') this._fetchSysData();
    else this._fetchAIData();
  }

  private _onTimeChange = async (e: Event) => {
    this._timeRange = (e.target as HTMLSelectElement).value;
    if (this._activeTab === 'sys') await this._fetchSysData();
    else await this._fetchAIData();
  };

  private _openDetailView = (): void => {
    ContentService.open('metrics', undefined, {
      title: 'Metrics',
      metadata: { initialTab: this._activeTab, timeRange: this._timeRange },
    });
  };

  // ── Data fetching ──────────────────────────────────────────────────────────

  private async _fetchSysData(): Promise<void> {
    try {
      const result = await SystemMetrics.execute({
        range: this._timeRange,
        maxPoints: 120,
      });

      if (result?.success) {
        this._fetchError = null;
        this._sysTimeSeries = result.timeSeries ?? [];
        this._sysCurrent = result.current ?? this._sysCurrent;
      } else if (result?.error) {
        this._fetchError = result.error.includes('IPC') ? 'Core process offline' : result.error;
      }
      this.requestUpdate();
    } catch (e) {
      this._fetchError = 'Metrics unavailable';
      this.requestUpdate();
    }
  }

  private async _fetchAIData(): Promise<void> {
    try {
      // Adaptive interval: finer granularity for shorter time ranges
      const intervalMap: Record<string, string> = {
        '1h': '5m',
        '6h': '30m',
        '24h': '1h',
        '7d': '6h',
      };

      const result = await AICost.execute({
        startTime: this._timeRange,
        includeTimeSeries: true,
        interval: intervalMap[this._timeRange] ?? '1h',
        includeBreakdown: false,
      });

      if (result?.success && result.summary) {
        this._fetchError = null;
        this._aiTimeSeries = result.timeSeries ?? [];
        this._aiSummary = {
          requests: result.summary.totalGenerations ?? 0,
          tokens: result.summary.totalTokens ?? 0,
          latency: result.summary.avgResponseTime ?? 0,
          cost: result.summary.totalCost ?? 0,
        };
      } else if (result?.error) {
        this._fetchError = result.error.includes('IPC') ? 'Core process offline' : result.error;
      }
      this.requestUpdate();
    } catch (e) {
      this._fetchError = 'AI metrics unavailable';
      this.requestUpdate();
    }
  }
}
