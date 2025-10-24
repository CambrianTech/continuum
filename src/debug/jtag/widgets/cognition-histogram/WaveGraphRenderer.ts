/**
 * Wave Graph Renderer - Trend line showing capacity oscillation
 *
 * Renders a smooth polyline connecting capacity data points over time.
 */

import { ChartRenderer, type ChartData } from './ChartRenderer';

export class WaveGraphRenderer extends ChartRenderer {
  protected renderChart(data: ChartData[]): void {
    if (data.length < 2) return;  // Need at least 2 points for a line

    // Generate points for polyline
    const points = data.map((stageData, index) => {
      const x = (index / (data.length - 1)) * 100;
      const height = this.scaleHeight(stageData.percentCapacity);
      const y = this.getYPosition(height);
      return `${x},${y}`;
    }).join(' ');

    const polyline = this.createPolyline(points, '#ffd700', {
      strokeWidth: 3,
      glow: true
    });

    this.svg.appendChild(polyline);
  }
}
