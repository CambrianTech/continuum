/**
 * ContinuumMetrics Widget - AI Performance Dashboard
 * Shows mini charts for: requests, tokens/sec, latency, cost
 */

import { BaseWidget } from '../shared/BaseWidget';
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
}

export class ContinuumMetricsWidget extends BaseWidget {
  private currentMetric: string = 'requests';  // Match the default in template
  private currentTimeRange: string = '24h';
  private metricsData: MetricsData | null = null;

  constructor() {
    console.log('🔧 WIDGET-DEBUG-' + Date.now() + ': ContinuumMetricsWidget constructor called');
    super({
      widgetId: 'continuum-metrics-widget',
      widgetName: 'ContinuumMetricsWidget',
      styles: 'continuum-metrics.css',
      template: 'continuum-metrics.html',
      enableAI: false,
      enableDatabase: true,  // Enable to receive ai_generations database events
      enableRouterEvents: false,
      enableScreenshots: false
    });
    console.log('🔧 WIDGET-DEBUG-' + Date.now() + ': ContinuumMetricsWidget constructor completed');
  }

  protected async onWidgetInitialize(): Promise<void> {
    this.subscribeToAIEvents();

    // Spawn async data fetch (don't block initialization)
    this.fetchMetricsData().catch(error => {
      console.error('❌ ContinuumMetrics: Failed to fetch initial data:', error);
    });
  }

  protected async renderWidget(): Promise<void> {
    const styles = this.templateCSS ?? '/* No styles loaded */';
    const template = this.templateHTML ?? '<div>Loading...</div>';
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${templateString}
    `;

    // Setup controls after shadowRoot populated
    this.setupFilterControls();
    this.setupChartClick();
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('📊 ContinuumMetrics: Cleaning up...');
  }

  /**
   * Subscribe to AI events for real-time metrics updates
   */
  private subscribeToAIEvents(): void {
    // Update metrics whenever an AI generates a response (cost/tokens change)
    Events.subscribe(AI_DECISION_EVENTS.POSTED, () => {
      console.log('📊 ContinuumMetrics: AI response posted, refreshing metrics...');
      this.fetchMetricsData();
    });

    // Also subscribe to database events for AI generation tracking
    const dbEvent = `data:${AIGenerationEntity.collection}:created`;
    Events.subscribe(dbEvent, () => {
      console.log('📊 ContinuumMetrics: New AI generation saved, refreshing metrics...');
      this.fetchMetricsData();
    });
  }

  /**
   * Setup filter dropdown handlers
   */
  private setupFilterControls(): void {
    const metricSelect = this.shadowRoot?.querySelector('.metric-select') as HTMLSelectElement;
    const timeSelect = this.shadowRoot?.querySelector('.time-select') as HTMLSelectElement;

    if (metricSelect) {
      metricSelect.addEventListener('change', () => {
        this.updateMetric(metricSelect.value);
      });
    }

    if (timeSelect) {
      timeSelect.addEventListener('change', () => {
        this.updateTimeRange(timeSelect.value);
      });
    }
  }

  /**
   * Setup click handler for chart expansion
   */
  private setupChartClick(): void {
    const chartArea = this.shadowRoot?.querySelector('.chart-area');
    if (chartArea) {
      chartArea.addEventListener('click', () => {
        console.log('📊 ContinuumMetrics: Chart clicked - expand to full view');
        // TODO: Emit event to open full metrics view
      });
    }
  }

  /**
   * Fetch metrics data from ai/cost command
   */
  private async fetchMetricsData(): Promise<void> {
    try {
      const result = await Commands.execute<AICostParams, AICostResult>('ai/cost', {
        startTime: this.currentTimeRange,
        includeTimeSeries: false,
        includeBreakdown: false
      });

      if (result?.success && result.summary) {
        this.metricsData = {
          totalGenerations: result.summary.totalGenerations,
          totalTokens: result.summary.totalTokens,
          avgResponseTime: result.summary.avgResponseTime,
          totalCost: result.summary.totalCost,
          timeRange: result.summary.timeRange?.duration || this.currentTimeRange
        };

        // Update DOM directly
        const title = this.shadowRoot?.querySelector('.metric-title') as HTMLElement;
        const value = this.shadowRoot?.querySelector('.metric-value') as HTMLElement;

        if (title && value) {
          title.textContent = 'REQUESTS';
          value.textContent = this.metricsData.totalGenerations.toString();
        }
      }
    } catch (error) {
      console.error('❌ ContinuumMetrics: Failed to fetch data:', error);
    }
  }

  /**
   * Update displayed metric
   */
  private updateMetric(metric: string): void {
    console.log('📊 ContinuumMetrics: Update metric:', metric);
    this.currentMetric = metric;
    this.updateMetricDisplay();
  }

  /**
   * Update metric display based on current metric selection
   */
  private updateMetricDisplay(): void {
    if (!this.metricsData) return;

    const title = this.shadowRoot?.querySelector('.metric-title') as HTMLElement;
    const value = this.shadowRoot?.querySelector('.metric-value') as HTMLElement;

    if (!title || !value) {
      console.warn('⚠️ ContinuumMetrics: DOM elements not found');
      return;
    }

    switch (this.currentMetric) {
      case 'requests':
        title.textContent = 'REQUESTS';
        value.textContent = this.metricsData.totalGenerations.toString();
        break;
      case 'tokens':
        title.textContent = 'TOKENS';
        value.textContent = this.formatNumber(this.metricsData.totalTokens);
        break;
      case 'latency':
        title.textContent = 'LATENCY';
        value.textContent = `${(this.metricsData.avgResponseTime / 1000).toFixed(2)}s`;
        break;
      case 'cost':
        title.textContent = 'COST';
        value.textContent = `$${this.metricsData.totalCost.toFixed(2)}`;
        break;
    }
  }

  /**
   * Format large numbers with K/M suffixes
   */
  private formatNumber(num: number): string {
    if (num >= 1000000) {
      return `${(num / 1000000).toFixed(1)}M`;
    } else if (num >= 1000) {
      return `${(num / 1000).toFixed(1)}K`;
    }
    return num.toString();
  }

  /**
   * Update time range
   */
  private async updateTimeRange(range: string): Promise<void> {
    console.log('📊 ContinuumMetrics: Update time range:', range);
    this.currentTimeRange = range;
    await this.fetchMetricsData();
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/continuum-metrics/public/${filename}`;
  }
}
