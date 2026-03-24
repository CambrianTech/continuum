/**
 * ContinuumChart — Universal chart web component
 *
 * Single reusable web component for ALL charting across the system.
 * Wraps SparklineChart.ts SVG rendering with reactivity, streaming, and theming.
 *
 * Sizes:
 *   sparkline — 200×80, no axes (sidebar sections, compact indicators)
 *   medium   — 400×120, minimal axes (widget cards, inline charts)
 *   large    — 600×160, full axes + grid (dashboard panels)
 *   full     — 100% width, 200+ height (full-tab dashboards)
 *
 * Features:
 *   - Theme-aware via CSS custom properties
 *   - Streaming mode: new data points append with animation
 *   - Multi-series: multiple lines with independent or shared Y-axis
 *   - Hover tooltips: show exact values on mouseover
 *
 * @example
 * ```typescript
 * html`
 *   <continuum-chart
 *     .data=${lossHistory}
 *     .series=${[
 *       { key: 'loss', color: 'var(--genome-cyan)', label: 'Loss' },
 *       { key: 'accuracy', color: '#4ade80', label: 'Token Accuracy' },
 *     ]}
 *     .xKey=${'step'}
 *     .size=${'large'}
 *     .streaming=${true}
 *     .yRange=${[0, 'auto']}
 *     .formatY=${(v: number) => v.toFixed(2)}
 *     .formatX=${(v: number) => `Step ${v}`}
 *   ></continuum-chart>
 * `
 * ```
 */

import {
  ReactiveWidget,
  html,
  svg,
  css,
  reactive,
  type TemplateResult,
  type SVGTemplateResult,
  type CSSResultGroup,
} from './ReactiveWidget';
import { nothing } from 'lit';

// ── Types ───────────────────────────────────────────────────────────────────

/** Chart size preset */
export type ChartSize = 'sparkline' | 'medium' | 'large' | 'full';

/** Y-axis range: [min, max]. Use 'auto' for automatic scaling. */
export type YRange = [number | 'auto', number | 'auto'];

/** Series descriptor for continuum-chart */
export interface ContinuumChartSeries {
  /** Key in data objects to extract the value */
  key: string;
  /** Line/fill color (CSS value, supports var()) */
  color: string;
  /** Display label for legend/tooltip */
  label: string;
  /** Optional custom formatter for tooltip values */
  format?: (value: number) => string;
}

/** A single tooltip state */
interface TooltipState {
  x: number;
  y: number;
  dataIndex: number;
  visible: boolean;
}

// ── Size configs ────────────────────────────────────────────────────────────

interface SizeConfig {
  width: number;
  height: number;
  padLeft: number;
  padRight: number;
  padTop: number;
  padBottom: number;
  showAxes: boolean;
  showGrid: boolean;
  gridCount: number;
  strokeWidth: number;
  fillOpacity: number;
  xLabelCount: number;
}

const SIZE_CONFIGS: Record<ChartSize, SizeConfig> = {
  sparkline: {
    width: 200, height: 80,
    padLeft: 2, padRight: 2, padTop: 5, padBottom: 5,
    showAxes: false, showGrid: true, gridCount: 3,
    strokeWidth: 1.5, fillOpacity: 0.08,
    xLabelCount: 0,
  },
  medium: {
    width: 400, height: 120,
    padLeft: 36, padRight: 8, padTop: 8, padBottom: 18,
    showAxes: true, showGrid: true, gridCount: 3,
    strokeWidth: 1.5, fillOpacity: 0.08,
    xLabelCount: 4,
  },
  large: {
    width: 600, height: 160,
    padLeft: 45, padRight: 10, padTop: 10, padBottom: 20,
    showAxes: true, showGrid: true, gridCount: 4,
    strokeWidth: 2, fillOpacity: 0.1,
    xLabelCount: 6,
  },
  full: {
    width: 800, height: 220,
    padLeft: 50, padRight: 14, padTop: 12, padBottom: 24,
    showAxes: true, showGrid: true, gridCount: 5,
    strokeWidth: 2, fillOpacity: 0.12,
    xLabelCount: 8,
  },
};

// ── Component ───────────────────────────────────────────────────────────────

export class ContinuumChart extends ReactiveWidget {
  static override styles = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        position: relative;
        width: 100%;
      }

      .chart-container {
        position: relative;
        width: 100%;
      }

      /* Full size stretches to fill parent */
      :host([size="full"]) .chart-container {
        width: 100%;
      }

      svg {
        display: block;
        width: 100%;
        height: auto;
      }

      /* Axis labels — positioned absolutely over SVG */
      .y-labels, .x-labels {
        position: absolute;
        pointer-events: none;
      }

      .y-labels {
        left: 0;
        top: 0;
        height: 100%;
      }

      .x-labels {
        left: 0;
        bottom: 0;
        width: 100%;
      }

      .axis-label {
        position: absolute;
        font-family: var(--font-mono, monospace);
        font-size: 9px;
        color: var(--content-secondary, #8a92a5);
        white-space: nowrap;
      }

      .y-label {
        right: calc(100% - var(--pad-left, 40px) + 4px);
        transform: translateY(-50%);
        text-align: right;
      }

      .x-label {
        top: calc(100% - var(--pad-bottom, 18px) + 4px);
        transform: translateX(-50%);
      }

      /* Tooltip */
      .tooltip {
        position: absolute;
        pointer-events: none;
        background: rgba(15, 20, 25, 0.95);
        border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
        border-radius: 4px;
        padding: 5px 8px;
        font-family: var(--font-mono, monospace);
        font-size: 10px;
        color: var(--content-primary, #e0e6ed);
        white-space: nowrap;
        z-index: 10;
        transform: translate(-50%, -100%);
        margin-top: -8px;
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4);
        transition: opacity 0.1s ease;
      }

      .tooltip-row {
        display: flex;
        align-items: center;
        gap: 6px;
        line-height: 1.4;
      }

      .tooltip-swatch {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        flex-shrink: 0;
      }

      .tooltip-value {
        font-weight: 600;
      }

      .tooltip-x {
        color: var(--content-secondary, #8a92a5);
        font-size: 9px;
        margin-bottom: 2px;
      }

      /* Legend */
      .legend {
        display: flex;
        gap: 12px;
        justify-content: center;
        padding: 4px 0 0;
        flex-wrap: wrap;
      }

      .legend-item {
        display: flex;
        align-items: center;
        gap: 4px;
        font-family: var(--font-mono, monospace);
        font-size: 9px;
        color: var(--content-secondary, #8a92a5);
      }

      .legend-swatch {
        width: 8px;
        height: 3px;
        border-radius: 1px;
      }

      /* Streaming animation */
      @keyframes chart-slide-in {
        from { opacity: 0.5; }
        to { opacity: 1; }
      }

      :host([streaming]) polyline,
      :host([streaming]) path {
        animation: chart-slide-in 0.3s ease;
      }
    `,
  ] as CSSResultGroup;

  // ── Reactive properties ─────────────────────────────────────────────────

  /** Data points array — each element is an object with numeric values */
  @reactive() data: Record<string, number>[] = [];

  /** Series descriptors */
  @reactive() series: ContinuumChartSeries[] = [];

  /** Key in data objects for X-axis values */
  @reactive() xKey: string = 'step';

  /** Chart size preset */
  @reactive() size: ChartSize = 'large';

  /** Enable streaming mode (append animation) */
  @reactive() streaming: boolean = false;

  /** Y-axis range. Default: [0, 'auto'] */
  @reactive() yRange: YRange = [0, 'auto'];

  /** Y-axis value formatter */
  @reactive() formatY: (value: number) => string = (v) => v.toFixed(2);

  /** X-axis value formatter */
  @reactive() formatX: (value: number) => string = (v) => String(v);

  /** Show legend below chart (auto-shown for multi-series in medium+ sizes) */
  @reactive() showLegend: boolean | 'auto' = 'auto';

  // ── Internal state ──────────────────────────────────────────────────────

  @reactive() private _tooltip: TooltipState = { x: 0, y: 0, dataIndex: -1, visible: false };
  private _prevDataLength: number = 0;

  constructor() {
    super({ widgetName: 'ContinuumChart' });
  }

  // ── Streaming: auto-scroll to latest data ───────────────────────────────

  protected override updated(changed: Map<string, unknown>): void {
    super.updated(changed);

    if (this.streaming && changed.has('data')) {
      const newLen = this.data.length;
      if (newLen > this._prevDataLength && this._prevDataLength > 0) {
        // New data arrived — could scroll or animate here
      }
      this._prevDataLength = newLen;
    }
  }

  // ── Render ──────────────────────────────────────────────────────────────

  protected override renderContent(): TemplateResult {
    const cfg = SIZE_CONFIGS[this.size];
    const data = this.data;
    const series = this.series;

    if (data.length === 0 || series.length === 0) {
      return html`
        <div class="chart-container" style="height: ${cfg.height}px; display: flex; align-items: center; justify-content: center;">
          <span style="font-size: 10px; color: var(--content-secondary, #8a92a5); font-style: italic;">No data</span>
        </div>
      `;
    }

    const chartW = cfg.width - cfg.padLeft - cfg.padRight;
    const chartH = cfg.height - cfg.padTop - cfg.padBottom;

    // Compute Y range
    const { yMin, yMax } = this._computeYRange(data, series);
    const yRange = yMax - yMin || 1;

    // Build SVG content
    const gridSvg = cfg.showGrid ? this._renderGrid(cfg, chartW, chartH, yMin, yMax) : nothing;
    const seriesSvg = series.map(s => this._renderSeries(data, s, cfg, chartW, chartH, yMin, yRange));

    // Hover crosshair
    const crosshairSvg = this._tooltip.visible && this._tooltip.dataIndex >= 0
      ? this._renderCrosshair(cfg, chartH)
      : nothing;

    // Axis labels
    const yLabels = cfg.showAxes ? this._computeYLabels(cfg, chartH, yMin, yMax) : [];
    const xLabels = cfg.showAxes && cfg.xLabelCount > 0 ? this._computeXLabels(data, cfg, chartW) : [];

    // Legend
    const legendVisible = this.showLegend === true || (this.showLegend === 'auto' && series.length > 1 && this.size !== 'sparkline');

    const viewBox = `0 0 ${cfg.width} ${cfg.height}`;

    return html`
      <div class="chart-container"
        style="--pad-left: ${cfg.padLeft}px; --pad-bottom: ${cfg.padBottom}px;"
        @mousemove=${(e: MouseEvent) => this._onMouseMove(e, cfg, chartW, data.length)}
        @mouseleave=${() => this._onMouseLeave()}
      >
        <svg viewBox="${viewBox}" preserveAspectRatio="xMidYMid meet">
          ${gridSvg}
          ${seriesSvg}
          ${crosshairSvg}
        </svg>

        ${yLabels.map(l => html`
          <span class="axis-label y-label" style="top: ${l.pct}%; left: 2px; right: auto; text-align: left;">${l.text}</span>
        `)}

        ${xLabels.map(l => html`
          <span class="axis-label x-label" style="left: ${l.pct}%;">${l.text}</span>
        `)}

        ${this._tooltip.visible ? this._renderTooltipHtml(data, series, cfg) : nothing}
      </div>

      ${legendVisible ? html`
        <div class="legend">
          ${series.map(s => html`
            <div class="legend-item">
              <div class="legend-swatch" style="background: ${s.color};"></div>
              <span>${s.label}</span>
            </div>
          `)}
        </div>
      ` : nothing}
    `;
  }

  // ── Y range computation ─────────────────────────────────────────────────

  private _computeYRange(
    data: Record<string, number>[],
    series: ContinuumChartSeries[]
  ): { yMin: number; yMax: number } {
    let yMin: number;
    let yMax: number;

    const requestedMin = this.yRange[0];
    const requestedMax = this.yRange[1];

    // Compute data extents
    let dataMin = Infinity;
    let dataMax = -Infinity;
    for (const s of series) {
      for (const d of data) {
        const v = d[s.key];
        if (v != null && isFinite(v)) {
          if (v < dataMin) dataMin = v;
          if (v > dataMax) dataMax = v;
        }
      }
    }
    if (!isFinite(dataMin)) dataMin = 0;
    if (!isFinite(dataMax)) dataMax = 1;
    if (dataMin === dataMax) dataMax = dataMin + 1;

    yMin = requestedMin === 'auto' ? dataMin : requestedMin;
    yMax = requestedMax === 'auto' ? dataMax : requestedMax;

    // Add 5% headroom when auto-scaling max
    if (requestedMax === 'auto' && yMax > yMin) {
      yMax += (yMax - yMin) * 0.05;
    }

    return { yMin, yMax };
  }

  // ── Grid rendering ──────────────────────────────────────────────────────

  private _renderGrid(cfg: SizeConfig, chartW: number, chartH: number, yMin: number, yMax: number): SVGTemplateResult {
    const paths: string[] = [];
    const range = yMax - yMin || 1;

    for (let i = 0; i <= cfg.gridCount; i++) {
      const frac = i / cfg.gridCount;
      const y = cfg.padTop + chartH * (1 - frac);

      // Horizontal grid line
      paths.push(`M${cfg.padLeft},${y.toFixed(1)} L${cfg.padLeft + chartW},${y.toFixed(1)}`);
    }

    // Grid lines only — labels handled by _computeYLabels for non-sparkline sizes

    return svg`
      <path d="${paths.join(' ')}" stroke="rgba(255,255,255,0.06)" stroke-width="0.5" fill="none"></path>
    `;
  }

  // ── Series rendering ────────────────────────────────────────────────────

  private _renderSeries(
    data: Record<string, number>[],
    s: ContinuumChartSeries,
    cfg: SizeConfig,
    chartW: number,
    chartH: number,
    yMin: number,
    yRange: number
  ): SVGTemplateResult {
    const points: string[] = [];

    for (let i = 0; i < data.length; i++) {
      const v = data[i][s.key];
      if (v == null || !isFinite(v)) continue;

      const x = cfg.padLeft + (i / Math.max(data.length - 1, 1)) * chartW;
      const y = cfg.padTop + chartH * (1 - (v - yMin) / yRange);
      points.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    }

    if (points.length === 0) return svg``;

    const linePoints = points.join(' ');

    // Area fill path
    const firstX = cfg.padLeft;
    const lastX = cfg.padLeft + chartW;
    const bottom = cfg.padTop + chartH;
    const areaPath = `M${firstX.toFixed(1)},${bottom.toFixed(1)} L${points.join(' L')} L${lastX.toFixed(1)},${bottom.toFixed(1)} Z`;

    return svg`
      <path d="${areaPath}" fill="${s.color}" opacity="${cfg.fillOpacity}"></path>
      <polyline stroke="${s.color}" stroke-width="${cfg.strokeWidth}" fill="none" opacity="0.9"
        points="${linePoints}"></polyline>
    `;
  }

  // ── Crosshair ───────────────────────────────────────────────────────────

  private _renderCrosshair(cfg: SizeConfig, chartH: number): SVGTemplateResult {
    const x = this._tooltip.x;
    return svg`
      <line x1="${x}" y1="${cfg.padTop}" x2="${x}" y2="${cfg.padTop + chartH}"
        stroke="rgba(255,255,255,0.2)" stroke-width="1" stroke-dasharray="3,3"></line>
    `;
  }

  // ── Axis label computation ──────────────────────────────────────────────

  private _computeYLabels(
    cfg: SizeConfig,
    chartH: number,
    yMin: number,
    yMax: number,
  ): Array<{ text: string; pct: number }> {
    const labels: Array<{ text: string; pct: number }> = [];
    const range = yMax - yMin || 1;

    for (let i = 0; i <= cfg.gridCount; i++) {
      const frac = i / cfg.gridCount;
      const y = cfg.padTop + chartH * (1 - frac);
      const val = yMin + range * frac;
      labels.push({
        text: this.formatY(val),
        pct: (y / cfg.height) * 100,
      });
    }
    return labels;
  }

  private _computeXLabels(
    data: Record<string, number>[],
    cfg: SizeConfig,
    chartW: number,
  ): Array<{ text: string; pct: number }> {
    const labels: Array<{ text: string; pct: number }> = [];
    const count = Math.min(data.length, cfg.xLabelCount);
    if (count < 2) return labels;

    for (let i = 0; i < count; i++) {
      const idx = Math.floor(i * (data.length - 1) / Math.max(count - 1, 1));
      const x = cfg.padLeft + (idx / Math.max(data.length - 1, 1)) * chartW;
      const val = data[idx][this.xKey];
      labels.push({
        text: this.formatX(val ?? idx),
        pct: (x / cfg.width) * 100,
      });
    }
    return labels;
  }

  // ── Tooltip ─────────────────────────────────────────────────────────────

  private _renderTooltipHtml(
    data: Record<string, number>[],
    series: ContinuumChartSeries[],
    cfg: SizeConfig,
  ): TemplateResult {
    const idx = this._tooltip.dataIndex;
    if (idx < 0 || idx >= data.length) return html``;

    const point = data[idx];
    const xVal = point[this.xKey] ?? idx;

    // Position tooltip as percentage of container
    const pctX = (this._tooltip.x / cfg.width) * 100;
    const pctY = (this._tooltip.y / cfg.height) * 100;

    return html`
      <div class="tooltip" style="left: ${pctX}%; top: ${pctY}%;">
        <div class="tooltip-x">${this.formatX(xVal)}</div>
        ${series.map(s => {
          const v = point[s.key];
          if (v == null) return nothing;
          const fmt = s.format ?? this.formatY;
          return html`
            <div class="tooltip-row">
              <div class="tooltip-swatch" style="background: ${s.color};"></div>
              <span>${s.label}:</span>
              <span class="tooltip-value">${fmt(v)}</span>
            </div>
          `;
        })}
      </div>
    `;
  }

  // ── Mouse interaction ───────────────────────────────────────────────────

  private _onMouseMove(e: MouseEvent, cfg: SizeConfig, chartW: number, dataLen: number): void {
    if (this.size === 'sparkline' || dataLen === 0) return;

    const container = e.currentTarget as HTMLElement;
    const rect = container.getBoundingClientRect();
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    // Convert to SVG coordinate space
    const svgX = (mouseX / rect.width) * cfg.width;
    const svgY = (mouseY / rect.height) * cfg.height;

    // Map to data index
    const relX = svgX - cfg.padLeft;
    if (relX < 0 || relX > chartW) {
      this._tooltip = { ...this._tooltip, visible: false };
      return;
    }

    const frac = relX / chartW;
    const idx = Math.round(frac * (dataLen - 1));
    const clampedIdx = Math.max(0, Math.min(dataLen - 1, idx));

    // Snap X to data point
    const snapX = cfg.padLeft + (clampedIdx / Math.max(dataLen - 1, 1)) * chartW;

    this._tooltip = { x: snapX, y: svgY, dataIndex: clampedIdx, visible: true };
  }

  private _onMouseLeave(): void {
    this._tooltip = { ...this._tooltip, visible: false };
  }

  // ── Public API ──────────────────────────────────────────────────────────

  /** Append a data point (streaming mode). Triggers re-render. */
  appendPoint(point: Record<string, number>): void {
    this.data = [...this.data, point];
  }

  /** Append multiple data points at once. */
  appendPoints(points: Record<string, number>[]): void {
    this.data = [...this.data, ...points];
  }

  /** Clear all data points. */
  clearData(): void {
    this.data = [];
    this._prevDataLength = 0;
  }

  /** Get the latest data point, or undefined. */
  get latestPoint(): Record<string, number> | undefined {
    return this.data.length > 0 ? this.data[this.data.length - 1] : undefined;
  }
}

// ── Register ────────────────────────────────────────────────────────────────

if (typeof customElements !== 'undefined' && !customElements.get('continuum-chart')) {
  customElements.define('continuum-chart', ContinuumChart);
}
