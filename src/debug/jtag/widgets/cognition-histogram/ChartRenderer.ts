/**
 * Abstract Chart Renderer - Base class for SVG chart visualizations
 *
 * Extracts common SVG manipulation code to eliminate duplication across
 * different visualization modes (pipeline stages, simple bars, wave graph).
 *
 * Philosophy: "Science the shit out of this" - Matt Damon, The Martian
 */

import { ColorPalette, getPalette } from './ColorPalettes';

// ONE PLACE to change the palette for ALL visualizations
const ACTIVE_PALETTE = ColorPalette.MAGMA;
const INVERT_COLORS = false;  // false = fast is hot (white), slow is cold (purple)

export interface ChartData {
  percentCapacity: number;  // 0-100 (percentage)
  percentSpeed: number;      // 0-100 (percentage)
  count: number;
}

export interface LegendConfig {
  visible: boolean;
  minLabel: string;
  maxLabel: string;
  colorFunction: (value: number) => string;
  minValue: number;
  maxValue: number;
}

export abstract class ChartRenderer {
  protected svg: SVGElement;
  protected legend: HTMLElement | null;
  protected readonly MAX_BAR_HEIGHT = 70;  // Limit bars to 70% of container
  protected readonly SVG_NS = 'http://www.w3.org/2000/svg';

  constructor(svg: SVGElement, legend?: HTMLElement) {
    this.svg = svg;
    this.legend = legend ?? null;
  }

  /**
   * Get legend configuration for this chart type
   * Default: no legend (subclasses override)
   */
  protected getLegendConfig(): LegendConfig {
    return {
      visible: false,
      minLabel: '',
      maxLabel: '',
      colorFunction: () => '#000',
      minValue: 0,
      maxValue: 100
    };
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
   * Update legend based on chart type
   * Legend element passed in constructor
   */
  private updateLegend(): void {
    if (!this.legend) return;

    const legendConfig = this.getLegendConfig();

    if (!legendConfig.visible) {
      this.legend.style.display = 'none';
      return;
    }

    // Generate CSS gradient using the SAME color function as the bars
    const gradientStops = ChartRenderer.generateCSSGradient(
      legendConfig.colorFunction,
      legendConfig.minValue,
      legendConfig.maxValue,
      20
    );

    // Single gradient bar with labels
    this.legend.style.display = 'flex';
    this.legend.style.alignItems = 'center';
    this.legend.style.gap = '8px';
    this.legend.innerHTML = `
      <span style="font-size: 10px; color: var(--content-tertiary, #6a7280);">${legendConfig.minLabel}</span>
      <div style="flex: 1; height: 10px; background: linear-gradient(to right, ${gradientStops}); border-radius: 2px;"></div>
      <span style="font-size: 10px; color: var(--content-tertiary, #6a7280);">${legendConfig.maxLabel}</span>
    `;
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
   * Generate heatmap color from normalized value (0-1)
   * Uses the active palette - change ACTIVE_PALETTE to switch colors everywhere
   * @param valueNormalized - Value between 0.0 and 1.0
   */
  static heatmap(valueNormalized: number): string {
    const paletteFunc = getPalette(ACTIVE_PALETTE);
    return paletteFunc(valueNormalized);
  }

  /**
   * Map speed percentage to heatmap color
   * INVERT_COLORS=false: fast=hot (white), slow=cold (purple)
   * INVERT_COLORS=true: fast=cold (purple), slow=hot (white)
   * @param speedPercent - Percentage value 0-100
   */
  static getSpeedColor(speedPercent: number): string {
    const speedNormalized = speedPercent / 100;
    const heatValue = INVERT_COLORS ? (1.0 - speedNormalized) : speedNormalized;
    return ChartRenderer.heatmap(heatValue);
  }

  /**
   * Instance method delegates to static
   */
  protected getSpeedColor(speedPercent: number): string {
    return ChartRenderer.getSpeedColor(speedPercent);
  }

  /**
   * Generate CSS gradient string by sampling color function
   * Returns CSS gradient stops that match the color mapping exactly
   */
  static generateCSSGradient(
    colorFunction: (value: number) => string,
    minValue: number,
    maxValue: number,
    samples: number = 20
  ): string {
    const stops: string[] = [];

    for (let i = 0; i < samples; i++) {
      const t = i / (samples - 1);
      const value = minValue + t * (maxValue - minValue);
      const color = colorFunction(value);
      const percent = t * 100;

      stops.push(`${color} ${percent}%`);
    }

    return stops.join(', ');
  }

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
      // rect.style.filter = `drop-shadow(0 0 4px ${fill})`; // DISABLED for performance
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
      // polyline.style.filter = 'drop-shadow(0 0 4px rgba(255, 215, 0, 0.6))'; // DISABLED for performance
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
