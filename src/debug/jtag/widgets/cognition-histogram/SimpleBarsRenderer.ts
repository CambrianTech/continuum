/**
 * Simple Bars Renderer - Single unified bar showing average performance
 *
 * Renders one wide bar with average speed/capacity across all stages.
 */

import { ChartRenderer, type ChartData, type LegendConfig } from './ChartRenderer';

export class SimpleBarsRenderer extends ChartRenderer {
  protected getLegendConfig(): LegendConfig {
    return {
      visible: true,
      minLabel: 'Stuck',
      maxLabel: 'Fast',
      colorFunction: ChartRenderer.getSpeedColor,
      minValue: 0,
      maxValue: 100
    };
  }

  protected renderChart(data: ChartData[]): void {
    // Calculate overall averages
    const avgSpeed = this.average(data.map(d => d.percentSpeed));
    const avgCapacity = this.average(data.map(d => d.percentCapacity));

    const height = this.scaleHeight(avgCapacity);
    const y = this.getYPosition(height);
    const color = this.getSpeedColor(avgSpeed);

    // Create centered wide bar (30% width at 35% x position)
    const rect = this.createRect(35, y, 30, height, color, {
      opacity: 0.8,
      rx: 4,
      glow: true
    });

    // Enhanced glow for simple bars
    rect.style.filter = `drop-shadow(0 0 6px ${color})`;

    this.svg.appendChild(rect);
  }
}
