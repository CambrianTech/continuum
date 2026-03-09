/**
 * SparklineChart — Reusable SVG chart rendering module
 *
 * Renders sparkline and full-size charts as Lit SVGTemplateResult fragments.
 * Uses Lit's `svg` tagged template to avoid the self-closing tag bug
 * that causes <polyline> to nest inside <path> (invalid SVG, renders blank).
 *
 * Two chart types:
 * - Sparkline: compact sidebar chart (200×80), no axes
 * - Large: full-size chart (600×160), with grid and axis label data
 */

import { svg, type SVGTemplateResult } from 'lit';

// ── Types ───────────────────────────────────────────────────────────────────

/** A single data series rendered as a line + filled area */
export interface ChartSeries<T> {
  color: string;
  getValue: (point: T) => number;
}

/** Extended series config for large charts with labels and formatters */
export interface LargeChartSeries<T> extends ChartSeries<T> {
  label: string;
  format: (value: number) => string;
}

/** Axis label positioned by percentage */
export interface AxisLabel {
  text: string;
  pct: number; // percentage position (0-100)
}

/** Result from large chart computation — SVG + axis labels for HTML overlay */
export interface LargeChartResult {
  svgContent: SVGTemplateResult;
  yLabels: AxisLabel[];
  xLabels: AxisLabel[];
}

// ── Sparkline ───────────────────────────────────────────────────────────────

/**
 * Render a compact sparkline SVG.
 *
 * Each series is independently normalized to fill the chart height.
 * Returns SVGTemplateResult fragments to embed inside an `<svg>` element.
 *
 * @param data - Array of data points
 * @param series - Series descriptors (color + value accessor)
 * @param percentScale - true if values are already 0-1 (CPU/memory %)
 */
export function renderSparkline<T>(
  data: T[],
  series: ChartSeries<T>[],
  percentScale: boolean
): SVGTemplateResult {
  const W = 200, H = 80;
  const PAD_X = 2, PAD_Y = 5;
  const chartW = W - PAD_X * 2;
  const chartH = H - PAD_Y * 2;

  const gridLines = [0.25, 0.5, 0.75].map(frac => {
    const y = PAD_Y + chartH * (1 - frac);
    return `M${PAD_X},${y.toFixed(1)} L${W - PAD_X},${y.toFixed(1)}`;
  }).join(' ');

  return svg`
    <path d="${gridLines}" stroke="rgba(255,255,255,0.06)" stroke-width="0.5" fill="none"></path>
    ${series.map(s => renderSeriesIndependent(data, s, W, H, PAD_X, PAD_Y, chartW, chartH, percentScale, 1.5, 0.08, 0.85))}
  `;
}

// ── Large Chart ─────────────────────────────────────────────────────────────

/**
 * Render a large chart SVG with shared Y-axis across all series.
 *
 * Returns SVG content + axis label data for HTML overlay positioning.
 *
 * @param data - Array of data points (must have .timestamp for x-axis labels)
 * @param series - Series descriptors with labels and formatters
 * @param percentScale - true if values are already 0-1
 * @param formatTimeLabel - function to format timestamp for x-axis
 * @param formatAxisValue - function to format y-axis values
 */
export function renderLargeChart<T extends { timestamp: string | number }>(
  data: T[],
  series: LargeChartSeries<T>[],
  percentScale: boolean,
  formatTimeLabel: (ts: string | number) => string,
  formatAxisValue: (v: number) => string
): LargeChartResult {
  const W = 600, H = 160;
  const PAD_L = 45, PAD_R = 10, PAD_T = 10, PAD_B = 20;
  const chartW = W - PAD_L - PAD_R;
  const chartH = H - PAD_T - PAD_B;

  // Compute global min/max across all series for shared axis
  let globalMin = Infinity, globalMax = -Infinity;
  if (!percentScale) {
    for (const s of series) {
      for (const d of data) {
        const v = s.getValue(d);
        if (v < globalMin) globalMin = v;
        if (v > globalMax) globalMax = v;
      }
    }
    if (globalMin === globalMax) globalMax = globalMin + 1;
    if (globalMin >= 0) globalMin = 0;
  } else {
    globalMin = 0;
    globalMax = 1;
  }

  const range = globalMax - globalMin || 1;

  // Grid lines + Y-axis labels
  const gridCount = 4;
  const gridPaths: string[] = [];
  const yLabels: AxisLabel[] = [];

  for (let i = 0; i <= gridCount; i++) {
    const frac = i / gridCount;
    const y = PAD_T + chartH * (1 - frac);
    gridPaths.push(`M${PAD_L},${y.toFixed(1)} L${W - PAD_R},${y.toFixed(1)}`);
    const val = globalMin + range * frac;
    const label = percentScale ? `${(val * 100).toFixed(0)}%` : formatAxisValue(val);
    yLabels.push({ text: label, pct: (y / H) * 100 });
  }

  // X-axis labels
  const xLabels: AxisLabel[] = [];
  const tickCount = Math.min(data.length, 6);
  for (let i = 0; i < tickCount; i++) {
    const idx = Math.floor(i * (data.length - 1) / Math.max(tickCount - 1, 1));
    const x = PAD_L + (idx / Math.max(data.length - 1, 1)) * chartW;
    xLabels.push({ text: formatTimeLabel(data[idx].timestamp), pct: (x / W) * 100 });
  }

  // SVG content
  const svgContent = svg`
    <path d="${gridPaths.join(' ')}" stroke="rgba(255,255,255,0.06)" stroke-width="0.5" fill="none"></path>
    ${series.map(s => {
      const values = data.map(s.getValue);
      const points = values.map((v, i) => {
        const x = PAD_L + (i / Math.max(values.length - 1, 1)) * chartW;
        const y = PAD_T + chartH * (1 - (v - globalMin) / range);
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      });

      const linePoints = points.join(' ');
      const bottom = PAD_T + chartH;
      const areaPath = `M${PAD_L.toFixed(1)},${bottom.toFixed(1)} L${points.join(' L')} L${(PAD_L + chartW).toFixed(1)},${bottom.toFixed(1)} Z`;

      return svg`
        <path d="${areaPath}" fill="${s.color}" opacity="0.1"></path>
        <polyline stroke="${s.color}" stroke-width="2" fill="none" opacity="0.9"
          points="${linePoints}"></polyline>
      `;
    })}
  `;

  return { svgContent, yLabels, xLabels };
}

// ── Internal ────────────────────────────────────────────────────────────────

/** Render a single series with independent normalization (for sparklines) */
function renderSeriesIndependent<T>(
  data: T[],
  s: ChartSeries<T>,
  W: number, H: number,
  padX: number, padY: number,
  chartW: number, chartH: number,
  percentScale: boolean,
  strokeWidth: number,
  fillOpacity: number,
  strokeOpacity: number
): SVGTemplateResult {
  const values = data.map(s.getValue);
  const max = percentScale ? 1 : Math.max(...values, 0.001);
  const min = percentScale ? 0 : Math.min(...values, 0);
  const range = max - min || 1;

  const points = values.map((v, i) => {
    const x = (i / Math.max(values.length - 1, 1)) * chartW + padX;
    const y = padY + chartH * (1 - (v - min) / range);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });

  const linePoints = points.join(' ');
  const areaPath = `M${points[0]} ${points.slice(1).map(p => `L${p}`).join(' ')} L${(W - padX).toFixed(1)},${(H - padY).toFixed(1)} L${padX.toFixed(1)},${(H - padY).toFixed(1)} Z`;

  return svg`
    <path d="${areaPath}" fill="${s.color}" opacity="${fillOpacity}"></path>
    <polyline stroke="${s.color}" stroke-width="${strokeWidth}" fill="none" opacity="${strokeOpacity}"
      points="${linePoints}"></polyline>
  `;
}
