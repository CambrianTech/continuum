/**
 * ContinuumMetrics Widget - AI Performance Dashboard
 * Clean, simple design with 4 metric sparklines
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';
import { AIGenerationEntity } from '../../system/data/entities/AIGenerationEntity';
// Types imported but cast to any due to browser-server communication
import { styles } from './public/continuum-metrics.styles';

interface TimeSeriesPoint {
  timestamp: string;
  cost: number;
  generations: number;
  tokens: number;
  avgResponseTime: number;
}

export class ContinuumMetricsWidget extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(styles)
  ] as CSSResultGroup;

  @reactive() private timeRange: string = '7d';
  @reactive() private timeSeries: TimeSeriesPoint[] = [];
  @reactive() private summary: {
    requests: number;
    tokens: number;
    latency: number;
    cost: number;
  } = { requests: 0, tokens: 0, latency: 0, cost: 0 };

  constructor() {
    super({ widgetName: 'ContinuumMetricsWidget' });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_DECISION_EVENTS.POSTED, () => this.fetchData()),
        Events.subscribe(`data:${AIGenerationEntity.collection}:created`, () => this.fetchData())
      ];
      return () => unsubs.forEach(u => u());
    });

    // Auto-detect best time range based on data distribution
    await this.autoDetectTimeRange();
  }

  private async autoDetectTimeRange(): Promise<void> {
    // Fetch 7d data to analyze distribution
    const result = await Commands.execute('ai/cost', {
      startTime: '7d',
      includeTimeSeries: true,
      interval: '1h',
      includeBreakdown: false
    } as any) as any;

    if (!result?.success || !result.timeSeries?.length) {
      await this.fetchData();
      return;
    }

    // Find where the activity is concentrated
    const now = Date.now();
    const hourMs = 3600000;
    const timeSeries = result.timeSeries as TimeSeriesPoint[];

    // Count activity in each time window
    let last1h = 0, last6h = 0, last24h = 0, total = 0;

    for (const point of timeSeries) {
      const age = now - new Date(point.timestamp).getTime();
      const activity = point.generations;
      total += activity;

      if (age <= hourMs) last1h += activity;
      if (age <= 6 * hourMs) last6h += activity;
      if (age <= 24 * hourMs) last24h += activity;
    }

    // Choose the smallest range that contains most of the activity
    if (total > 0) {
      if (last1h / total >= 0.7) {
        this.timeRange = '1h';
      } else if (last6h / total >= 0.7) {
        this.timeRange = '6h';
      } else if (last24h / total >= 0.7) {
        this.timeRange = '24h';
      } else {
        this.timeRange = '7d';
      }
    }

    // Now fetch with the optimal range
    await this.fetchData();
  }

  protected override renderContent(): TemplateResult {
    return html`
      <div class="metrics-panel">
        <div class="metrics-header">
          <span class="title">AI Performance</span>
          <select class="time-select" @change=${this.onTimeChange}>
            ${['1h', '6h', '24h', '7d'].map(t => html`
              <option value="${t}" ?selected=${this.timeRange === t}>${t}</option>
            `)}
          </select>
        </div>

        <div class="chart-container">
          <svg viewBox="0 0 200 80" preserveAspectRatio="none">
            <polyline stroke="#00d4ff" stroke-width="2" fill="none" opacity="0.8"
              points="${this.getPathPoints(p => p.generations)}" />
            <polyline stroke="#ff6b6b" stroke-width="2" fill="none" opacity="0.8"
              points="${this.getPathPoints(p => p.tokens / 1000)}" />
            <polyline stroke="#ffd700" stroke-width="2" fill="none" opacity="0.8"
              points="${this.getPathPoints(p => p.avgResponseTime / 1000)}" />
            <polyline stroke="#4ade80" stroke-width="2" fill="none" opacity="0.8"
              points="${this.getPathPoints(p => p.cost * 100)}" />
          </svg>
        </div>

        <div class="legend">
          ${this.renderLegendItem('Req', '#00d4ff', this.summary.requests.toString())}
          ${this.renderLegendItem('Tok', '#ff6b6b', this.formatNumber(this.summary.tokens))}
          ${this.renderLegendItem('Lat', '#ffd700', `${(this.summary.latency / 1000).toFixed(1)}s`)}
          ${this.renderLegendItem('Cost', '#4ade80', `$${this.summary.cost.toFixed(2)}`)}
        </div>
      </div>
    `;
  }

  private getPathPoints(getValue: (p: TimeSeriesPoint) => number): string {
    if (!this.timeSeries.length) return '2,40 198,40';

    const values = this.timeSeries.map(getValue);
    const max = Math.max(...values, 0.001);
    const min = Math.min(...values, 0);
    const range = max - min || 1;

    return values.map((v, i) => {
      const x = (i / Math.max(values.length - 1, 1)) * 196 + 2;
      const y = 75 - ((v - min) / range) * 65;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(' ');
  }

  private renderLegendItem(label: string, color: string, value: string): TemplateResult {
    return html`
      <div class="legend-item">
        <span class="dot" style="background:${color}"></span>
        <span class="label">${label}</span>
        <span class="value" style="color:${color}">${value}</span>
      </div>
    `;
  }


  private formatNumber(n: number): string {
    if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
    if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
    return n.toString();
  }

  private onTimeChange = async (e: Event) => {
    this.timeRange = (e.target as HTMLSelectElement).value;
    await this.fetchData();
  };

  private async fetchData(): Promise<void> {
    try {
      const result = await Commands.execute('ai/cost', {
        startTime: this.timeRange,
        includeTimeSeries: true,
        interval: '1h',
        includeBreakdown: false
      } as any) as any;

      if (result?.success && result.summary) {
        this.timeSeries = result.timeSeries || [];
        this.summary = {
          requests: result.summary.totalGenerations ?? 0,
          tokens: result.summary.totalTokens ?? 0,
          latency: result.summary.avgResponseTime ?? 0,
          cost: result.summary.totalCost ?? 0
        };
        // Debug logging removed - was flooding console
        this.requestUpdate();
      }
    } catch (e) {
      console.error('ContinuumMetrics fetch error:', e);
    }
  }
}
