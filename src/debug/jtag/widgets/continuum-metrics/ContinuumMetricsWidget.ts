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
        --color-primary: #00d4ff;
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

      .metric-header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
      }

      .metric-title {
        font-size: 10px;
        font-weight: 600;
        font-family: var(--font-mono, monospace);
        color: var(--content-secondary, #8a92a5);
        letter-spacing: 0.5px;
        text-transform: uppercase;
      }

      .metric-value {
        font-size: 18px;
        font-weight: 700;
        font-family: var(--font-mono, monospace);
        color: var(--color-primary, #00d4ff);
      }

      .chart-area {
        height: 80px;
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
        padding: 8px;
        cursor: pointer;
        transition: border-color 0.2s ease;
      }

      .chart-area:hover {
        border-color: var(--accent-primary, #4a9eff);
      }

      .chart-svg {
        width: 100%;
        height: 100%;
      }

      .chart-line {
        stroke: var(--accent-primary, #ffd700);
        stroke-linecap: round;
        stroke-linejoin: round;
        filter: drop-shadow(0 0 2px rgba(255, 215, 0, 0.4));
      }

      .metric-controls {
        display: flex;
        gap: 6px;
      }

      .metric-select,
      .time-select {
        flex: 1;
        padding: 4px 6px;
        font-size: 9px;
        font-family: var(--font-mono, monospace);
        background: var(--surface-primary, #1a1d24);
        color: var(--content-primary, #e8eaed);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 3px;
        cursor: pointer;
        transition: border-color 0.2s ease;
      }

      .metric-select:hover,
      .time-select:hover {
        border-color: var(--accent-primary, #4a9eff);
      }

      .metric-select:focus,
      .time-select:focus {
        outline: none;
        border-color: var(--accent-primary, #4a9eff);
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentMetric: string = 'requests';
  @reactive() private currentTimeRange: string = '24h';
  @reactive() private metricsData: MetricsData | null = null;
  @reactive() private chartPoints: string = '0,20 10,15 20,18 30,10 40,8 50,12 60,11 70,5 80,9 90,14 100,12';

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
        <div class="metric-header">
          <div class="metric-title">${this.getMetricTitle()}</div>
          <div class="metric-value">${this.getMetricValue()}</div>
        </div>

        <div class="chart-area" @click=${this.handleChartClick}>
          <svg class="chart-svg" viewBox="0 0 100 30" preserveAspectRatio="none">
            <polyline
              class="chart-line"
              points="${this.chartPoints}"
              fill="none"
              stroke-width="1"
            />
          </svg>
        </div>

        <div class="metric-controls">
          <select class="metric-select" @change=${this.handleMetricChange}>
            <option value="requests" ?selected=${this.currentMetric === 'requests'}>Requests</option>
            <option value="tokens" ?selected=${this.currentMetric === 'tokens'}>Tokens/s</option>
            <option value="latency" ?selected=${this.currentMetric === 'latency'}>Latency</option>
            <option value="cost" ?selected=${this.currentMetric === 'cost'}>Cost</option>
          </select>
          <select class="time-select" @change=${this.handleTimeChange}>
            <option value="1h" ?selected=${this.currentTimeRange === '1h'}>1h</option>
            <option value="6h" ?selected=${this.currentTimeRange === '6h'}>6h</option>
            <option value="24h" ?selected=${this.currentTimeRange === '24h'}>24h</option>
            <option value="7d" ?selected=${this.currentTimeRange === '7d'}>7d</option>
          </select>
        </div>
      </div>
    `;
  }

  // === Computed Values ===

  private getMetricTitle(): string {
    switch (this.currentMetric) {
      case 'requests': return 'REQUESTS';
      case 'tokens': return 'TOKENS';
      case 'latency': return 'LATENCY';
      case 'cost': return 'COST';
      default: return 'REQUESTS';
    }
  }

  private getMetricValue(): string {
    if (!this.metricsData) return '—';

    switch (this.currentMetric) {
      case 'requests':
        return this.metricsData.totalGenerations.toString();
      case 'tokens':
        return this.formatNumber(this.metricsData.totalTokens);
      case 'latency':
        return `${(this.metricsData.avgResponseTime / 1000).toFixed(2)}s`;
      case 'cost':
        return `$${this.metricsData.totalCost.toFixed(2)}`;
      default:
        return '—';
    }
  }

  private formatNumber(num: number): string {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  }

  // === Event Handlers ===

  private handleMetricChange = (e: Event): void => {
    const select = e.target as HTMLSelectElement;
    this.currentMetric = select.value;
    this.updateChart();
    this.requestUpdate();
  };

  private handleTimeChange = async (e: Event): Promise<void> => {
    const select = e.target as HTMLSelectElement;
    this.currentTimeRange = select.value;
    await this.fetchMetricsData();
  };

  private handleChartClick = (): void => {
    // TODO: Emit event to open full metrics view
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

        this.updateChart();
        this.requestUpdate();
      }
    } catch (error) {
      console.error('ContinuumMetrics: Failed to fetch data:', error);
    }
  }

  private updateChart(): void {
    if (!this.metricsData?.timeSeries) return;

    const values = this.metricsData.timeSeries.map(point => {
      switch (this.currentMetric) {
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

    this.chartPoints = values.map((value, index) => {
      const x = (index / (values.length - 1 || 1)) * 100;
      const y = 30 - ((value - minValue) / range) * 30;
      return `${x},${y}`;
    }).join(' ');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
