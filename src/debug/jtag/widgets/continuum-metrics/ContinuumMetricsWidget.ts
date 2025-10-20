/**
 * ContinuumMetrics Widget - AI Performance Dashboard
 * Shows mini charts for: requests, tokens/sec, latency, cost
 */

import { BaseWidget } from '../shared/BaseWidget';

export class ContinuumMetricsWidget extends BaseWidget {
  constructor() {
    super({
      widgetId: 'continuum-metrics-widget',
      widgetName: 'ContinuumMetricsWidget',
      styles: 'continuum-metrics.css',
      template: 'continuum-metrics.html',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('📊 ContinuumMetrics: Initializing...');

    this.setupFilterControls();
    this.setupChartClick();

    console.log('✅ ContinuumMetrics: Initialized');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('📊 ContinuumMetrics: Cleaning up...');
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
   * Update displayed metric
   */
  private updateMetric(metric: string): void {
    console.log('📊 ContinuumMetrics: Update metric:', metric);
    const title = this.shadowRoot?.querySelector('.metric-title');
    const value = this.shadowRoot?.querySelector('.metric-value');

    if (title) title.textContent = metric.toUpperCase();
    if (value) {
      // Fake values for now
      const values: Record<string, string> = {
        requests: '39',
        tokens: '1.4K',
        latency: '0.22',
        cost: '$2.45'
      };
      value.textContent = values[metric] || '0';
    }
  }

  /**
   * Update time range
   */
  private updateTimeRange(range: string): void {
    console.log('📊 ContinuumMetrics: Update time range:', range);
    // TODO: Fetch new data for time range
  }

  protected async renderWidget(): Promise<void> {
    const styles = this.templateCSS ?? '/* No styles loaded */';
    const template = this.templateHTML ?? '<div>Loading...</div>';
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${templateString}
    `;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/continuum-metrics/public/${filename}`;
  }
}

// Register widget
customElements.define('continuum-metrics-widget', ContinuumMetricsWidget);
