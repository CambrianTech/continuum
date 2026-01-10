/**
 * ContinuumMetrics Widget - AI Performance Dashboard
 * Shows mini charts for: requests, tokens/sec, latency, cost
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
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
import type { AICostParams, AICostResult } from '../../commands/ai/cost/shared/AICostTypes';

interface MetricsData {
  totalGenerations: number;
  totalTokens: number;
  avgResponseTime: number;
  totalCost: number;
  timeRange: string;
  timeSeries?: Array<{
    timestamp: string;
    cost: number;
    generations: number;
    tokens: number;
    avgResponseTime: number;
  }>;
}

export class ContinuumMetricsWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      :host {
        display: block;
        width: 100%;
        --color-requests: #00d4ff;
        --color-tokens: #ff6b6b;
        --color-latency: #ffd700;
        --color-cost: #4ade80;
      }

      .metrics-container {
        display: flex;
        flex-direction: column;
        gap: 8px;
        padding: 12px;
        background: var(--surface-secondary, #0f1117);
        border: 1px solid var(--border-primary, #2a2d35);
        border-radius: 6px;
      }

      .metrics-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 4px;
      }

      .metrics-title {
        font-size: 10px;
        font-weight: 600;
        font-family: var(--font-mono, monospace);
        color: var(--content-secondary, #8a92a5);
        letter-spacing: 0.5px;
        text-transform: uppercase;
      }

      .time-select {
        padding: 2px 6px;
        font-size: 9px;
        font-family: var(--font-mono, monospace);
        background: var(--surface-primary, #1a1d24);
        color: var(--content-primary, #e8eaed);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 3px;
        cursor: pointer;
      }

      .metrics-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 8px;
      }

      .metric-card {
        display: flex;
        flex-direction: column;
        gap: 4px;
        padding: 8px;
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
        cursor: pointer;
        transition: border-color 0.2s ease;
      }

      .metric-card:hover {
        border-color: var(--accent-primary, #4a9eff);
      }

      .metric-card.requests { --card-color: var(--color-requests); }
      .metric-card.tokens { --card-color: var(--color-tokens); }
      .metric-card.latency { --card-color: var(--color-latency); }
      .metric-card.cost { --card-color: var(--color-cost); }

      .card-header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
      }

      .card-title {
        font-size: 9px;
        font-weight: 600;
        font-family: var(--font-mono, monospace);
        color: var(--content-tertiary, #6a7280);
        letter-spacing: 0.5px;
        text-transform: uppercase;
      }

      .card-value {
        font-size: 14px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        color: var(--card-color, #00d4ff);
      }

      .sparkline-container {
        height: 24px;
        width: 100%;
      }

      .sparkline-svg {
        width: 100%;
        height: 100%;
      }

      .sparkline {
        stroke: var(--card-color, #00d4ff);
        stroke-width: 1.5;
        stroke-linecap: round;
        stroke-linejoin: round;
        fill: none;
        filter: drop-shadow(0 0 2px var(--card-color));
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentTimeRange: string = '24h';
  @reactive() private metricsData: MetricsData | null = null;
  @reactive() private sparklines: Record<string, string> = {
    requests: '0,12 25,10 50,14 75,8 100,12',
    tokens: '0,12 25,14 50,10 75,16 100,8',
    latency: '0,8 25,12 50,10 75,14 100,12',
    cost: '0,10 25,12 50,8 75,14 100,10'
  };

  // Non-reactive
  private chartInterval: string = '1h';

  constructor() {
    super({
      widgetName: 'ContinuumMetricsWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Subscribe to AI events
    this.createMountEffect(() => {
      const unsubs = [
        Events.subscribe(AI_DECISION_EVENTS.POSTED, () => this.fetchMetricsData()),
        Events.subscribe(`data:${AIGenerationEntity.collection}:created`, () => this.fetchMetricsData())
      ];
      return () => unsubs.forEach(u => u());
    });

    // Initial data fetch
    this.fetchMetricsData().catch(err => console.error('ContinuumMetrics: Failed to fetch initial data:', err));
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="metrics-container">
        <div class="metrics-header">
          <span class="metrics-title">AI Performance</span>
          <select class="time-select" @change=${this.handleTimeChange}>
            <option value="1h" ?selected=${this.currentTimeRange === '1h'}>1h</option>
            <option value="6h" ?selected=${this.currentTimeRange === '6h'}>6h</option>
            <option value="24h" ?selected=${this.currentTimeRange === '24h'}>24h</option>
            <option value="7d" ?selected=${this.currentTimeRange === '7d'}>7d</option>
          </select>
        </div>

        <div class="metrics-grid">
          ${this.renderMetricCard('requests', 'Requests', this.getRequestsValue())}
          ${this.renderMetricCard('tokens', 'Tokens', this.getTokensValue())}
          ${this.renderMetricCard('latency', 'Latency', this.getLatencyValue())}
          ${this.renderMetricCard('cost', 'Cost', this.getCostValue())}
        </div>
      </div>
    `;
  }

  private renderMetricCard(metric: string, title: string, value: string): TemplateResult {
    return html`
      <div class="metric-card ${metric}" @click=${() => this.handleCardClick(metric)}>
        <div class="card-header">
          <span class="card-title">${title}</span>
          <span class="card-value">${value}</span>
        </div>
        <div class="sparkline-container">
          <svg class="sparkline-svg" viewBox="0 0 100 24" preserveAspectRatio="none">
            <polyline class="sparkline" points="${this.sparklines[metric]}" />
          </svg>
        </div>
      </div>
    `;
  }

  // === Computed Values ===

  private getRequestsValue(): string {
    if (!this.metricsData) return '—';
    return this.metricsData.totalGenerations.toString();
  }

  private getTokensValue(): string {
    if (!this.metricsData) return '—';
    return this.formatNumber(this.metricsData.totalTokens);
  }

  private getLatencyValue(): string {
    if (!this.metricsData) return '—';
    return `${(this.metricsData.avgResponseTime / 1000).toFixed(1)}s`;
  }

  private getCostValue(): string {
    if (!this.metricsData) return '—';
    return `$${this.metricsData.totalCost.toFixed(2)}`;
  }

  private formatNumber(num: number): string {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  }

  // === Event Handlers ===

  private handleTimeChange = async (e: Event): Promise<void> => {
    const select = e.target as HTMLSelectElement;
    this.currentTimeRange = select.value;
    await this.fetchMetricsData();
  };

  private handleCardClick = (metric: string): void => {
    // TODO: Open detailed view for this metric
    console.log(`Metrics: Clicked ${metric} card`);
  };

  // === Data Fetching ===

  private async fetchMetricsData(): Promise<void> {
    try {
      const result = await Commands.execute<AICostParams, AICostResult>('ai/cost', {
        startTime: this.currentTimeRange,
        includeTimeSeries: true,
        interval: this.chartInterval,
        includeBreakdown: false
      });

      if (result?.success && result.summary) {
        this.metricsData = {
          totalGenerations: result.summary.totalGenerations,
          totalTokens: result.summary.totalTokens,
          avgResponseTime: result.summary.avgResponseTime,
          totalCost: result.summary.totalCost,
          timeRange: result.summary.timeRange?.duration || this.currentTimeRange,
          timeSeries: result.timeSeries
        };

        this.updateCharts();
        this.requestUpdate();
      }
    } catch (error) {
      console.error('ContinuumMetrics: Failed to fetch data:', error);
    }
  }

  private updateCharts(): void {
    if (!this.metricsData?.timeSeries) return;

    // Update sparkline for each metric
    const metrics = ['requests', 'tokens', 'latency', 'cost'];

    for (const metric of metrics) {
      const values = this.metricsData.timeSeries.map(point => {
        switch (metric) {
          case 'requests': return point.generations;
          case 'tokens': return point.tokens;
          case 'cost': return point.cost * 1000;
          case 'latency': return point.avgResponseTime / 1000;
          default: return point.generations;
        }
      });

      const maxValue = Math.max(...values, 1);
      const minValue = Math.min(...values, 0);
      const range = maxValue - minValue || 1;

      this.sparklines[metric] = values.map((value, index) => {
        const x = (index / (values.length - 1 || 1)) * 100;
        const y = 24 - ((value - minValue) / range) * 20;
        return `${x},${y}`;
      }).join(' ');
    }

    // Trigger re-render
    this.sparklines = { ...this.sparklines };
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
