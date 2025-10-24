/**
 * Abstract Chart Renderer - Base class for SVG chart visualizations
 *
 * Extracts common SVG manipulation code to eliminate duplication across
 * different visualization modes (pipeline stages, simple bars, wave graph).
 *
 * Philosophy: "Science the shit out of this" - Matt Damon, The Martian
 */

export interface ChartData {
  percentCapacity: number;  // 0-100
  percentSpeed: number;      // 0-100
  count: number;
}

export interface LegendConfig {
  visible: boolean;
  items?: Array<{ label: string; color: string }>;
}

export abstract class ChartRenderer {
  protected svg: SVGElement;
  protected readonly MAX_BAR_HEIGHT = 70;  // Limit bars to 70% of container
  protected readonly SVG_NS = 'http://www.w3.org/2000/svg';

  constructor(svg: SVGElement) {
    this.svg = svg;
  }

  /**
   * Get legend configuration for this chart type
   * Default: no legend (subclasses override)
   */
  protected getLegendConfig(): LegendConfig {
    return { visible: false };
  }

  /**
   * Clear SVG and render chart
   */
  render(data: ChartData[]): void {
    this.clearSVG();
    if (data.length === 0) return;
    this.renderChart(data);
    this.updateLegend();
  }

  /**
   * Update legend visibility based on chart type
   */
  private updateLegend(): void {
    const legendConfig = this.getLegendConfig();
    const legendElement = document.querySelector('.legend') as HTMLElement;

    if (legendElement) {
      legendElement.style.display = legendConfig.visible ? 'flex' : 'none';
    }
  }

  /**
   * Subclasses implement specific chart rendering logic
   */
  protected abstract renderChart(data: ChartData[]): void;

  /**
   * Clear all SVG contents
   */
  protected clearSVG(): void {
    this.svg.innerHTML = '';
  }

  /**
   * Get color based on speed percentage (heat map: green=fast, red=stuck)
   * Static so other code can use the same color mapping
   */
  static getSpeedColor(percentSpeed: number): string {
    if (percentSpeed >= 80) return '#0f0';  // Fast (green)
    if (percentSpeed >= 50) return '#ff0';  // Normal (yellow)
    if (percentSpeed >= 25) return '#fa0';  // Slow (orange)
    return '#f00';                          // Bottleneck (red)
  }

  /**
   * Instance method delegates to static
   */
  protected getSpeedColor(percentSpeed: number): string {
    return ChartRenderer.getSpeedColor(percentSpeed);
  }

  /**
   * Static legend items for speed categories
   * Reusable for any renderer that wants speed-based legends
   */
  static readonly SPEED_LEGEND_ITEMS = [
    { label: 'Fast', color: '#0f0' },
    { label: 'Normal', color: '#ff0' },
    { label: 'Slow', color: '#fa0' },
    { label: 'Stuck', color: '#f00' }
  ];

  /**
   * Create SVG rect element with common attributes
   */
  protected createRect(
    x: number | string,
    y: number | string,
    width: number | string,
    height: number | string,
    fill: string,
    options: {
      opacity?: number;
      rx?: number;
      glow?: boolean;
    } = {}
  ): SVGRectElement {
    const rect = document.createElementNS(this.SVG_NS, 'rect');

    rect.setAttribute('x', this.toPercent(x));
    rect.setAttribute('y', this.toPercent(y));
    rect.setAttribute('width', this.toPercent(width));
    rect.setAttribute('height', this.toPercent(height));
    rect.setAttribute('fill', fill);
    rect.setAttribute('opacity', (options.opacity ?? 0.8).toString());

    if (options.rx) {
      rect.setAttribute('rx', options.rx.toString());
    }

    if (options.glow) {
      rect.style.filter = `drop-shadow(0 0 4px ${fill})`;
    }

    return rect;
  }

  /**
   * Create SVG polyline for wave graphs
   */
  protected createPolyline(
    points: string,
    stroke: string,
    options: {
      strokeWidth?: number;
      glow?: boolean;
    } = {}
  ): SVGPolylineElement {
    const polyline = document.createElementNS(this.SVG_NS, 'polyline');

    polyline.setAttribute('points', points);
    polyline.setAttribute('fill', 'none');
    polyline.setAttribute('stroke', stroke);
    polyline.setAttribute('stroke-width', (options.strokeWidth ?? 3).toString());
    polyline.setAttribute('stroke-linecap', 'round');
    polyline.setAttribute('stroke-linejoin', 'round');

    if (options.glow) {
      polyline.style.filter = 'drop-shadow(0 0 4px rgba(255, 215, 0, 0.6))';
    }

    return polyline;
  }

  /**
   * Calculate scaled height (0-70% of container)
   */
  protected scaleHeight(percentCapacity: number): number {
    return (percentCapacity / 100) * this.MAX_BAR_HEIGHT;
  }

  /**
   * Calculate Y position for given height (SVG coordinates are top-down)
   */
  protected getYPosition(height: number): number {
    return 100 - height;
  }

  /**
   * Convert number to percentage string if needed
   */
  private toPercent(value: number | string): string {
    return typeof value === 'number' ? `${value}%` : value;
  }

  /**
   * Calculate average of data array
   */
  protected average(values: number[]): number {
    return values.length > 0 ? values.reduce((sum, v) => sum + v, 0) / values.length : 0;
  }
}
