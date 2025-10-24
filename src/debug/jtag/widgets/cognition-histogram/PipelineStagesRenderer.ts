/**
 * Pipeline Stages Renderer - Winamp-style frequency bars
 *
 * Renders individual bars for each pipeline stage with speed-based coloring.
 */

import { ChartRenderer, type ChartData } from './ChartRenderer';

export class PipelineStagesRenderer extends ChartRenderer {
  protected renderChart(data: ChartData[]): void {
    const barWidth = 100 / data.length;

    data.forEach((stageData, index) => {
      if (stageData.count === 0) return;

      const height = this.scaleHeight(stageData.percentCapacity);
      const x = index * barWidth;
      const y = this.getYPosition(height);
      const color = this.getSpeedColor(stageData.percentSpeed);

      // Create centered bar (60% width)
      const barWidthPercent = barWidth * 0.6;
      const xCentered = x + (barWidth - barWidthPercent) / 2;

      const rect = this.createRect(xCentered, y, barWidthPercent, height, color, {
        opacity: 0.8,
        rx: 2,
        glow: true
      });

      this.svg.appendChild(rect);
    });
  }
}
