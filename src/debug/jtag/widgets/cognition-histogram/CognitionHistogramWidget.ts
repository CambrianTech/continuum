/**
 * CognitionHistogram Widget - AI Pipeline Stages Visualization
 *
 * Winamp-style frequency bars showing 5 pipeline stages:
 * rag-build, should-respond, generate, coordination, post-response
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
import { Events } from '../../system/core/shared/Events';
import { COGNITION_EVENTS, type StageCompleteEvent, type PipelineStage, BASELINE_SPEEDS } from '../../system/conversation/shared/CognitionEventTypes';
import { PipelineStagesRenderer } from './PipelineStagesRenderer';
import { SimpleBarsRenderer } from './SimpleBarsRenderer';
import { WaveGraphRenderer } from './WaveGraphRenderer';
import type { ChartData } from './ChartRenderer';

interface StageData {
  stage: PipelineStage;
  avgDuration: number;
  percentCapacity: number;
  percentSpeed: number;
  count: number;
  lastUpdate: number;
}

type VisualizationMode = 'pipeline' | 'simple-bars' | 'wave-graph';

export class CognitionHistogramWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      :host {
        display: block;
        width: 100%;
        max-height: 120px;
      }

      .histogram-container {
        display: flex;
        flex-direction: column;
        height: 120px;
        max-height: 120px;
        background: var(--surface-secondary, #0f1117);
        border: 1px solid var(--border-primary, #2a2d35);
        border-radius: 6px;
        padding: 8px;
        cursor: pointer;
        transition: border-color 0.2s ease;
      }

      .histogram-container:hover {
        border-color: var(--accent-primary, #4a9eff);
      }

      .histogram-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 8px;
        padding: 0 4px;
      }

      .mode-label {
        font-size: 10px;
        font-weight: 600;
        font-family: var(--font-mono, monospace);
        color: var(--content-secondary, #8a92a5);
        letter-spacing: 0.5px;
        text-transform: uppercase;
      }

      .histogram-svg {
        flex: 1;
        width: 100%;
        background: var(--surface-primary, #1a1d24);
        border: 1px solid var(--border-secondary, #383b44);
        border-radius: 4px;
      }

      .histogram-svg rect {
        transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
      }

      .histogram-svg path {
        transition: d 0.3s cubic-bezier(0.4, 0, 0.2, 1);
      }

      .legend {
        display: flex;
        gap: 10px;
        margin-top: 4px;
        padding: 0 4px;
        font-size: 11px;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        font-weight: 500;
        color: var(--content-tertiary, #6a7280);
        text-transform: lowercase;
      }

      .legend-item {
        display: flex;
        align-items: center;
        gap: 4px;
      }

      .legend-dot {
        width: 8px;
        height: 8px;
        border-radius: 2px;
        display: inline-block;
      }

      .legend-dot.fast {
        background: lime;
        box-shadow: 0 0 3px lime;
      }

      .legend-dot.normal {
        background: #ff0;
        box-shadow: 0 0 3px #ff0;
      }

      .legend-dot.slow {
        background: #fa0;
        box-shadow: 0 0 3px #fa0;
      }

      .legend-dot.bottleneck {
        background: red;
        box-shadow: 0 0 3px red;
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private mode: VisualizationMode = 'pipeline';
  @reactive() private modeLabel: string = 'Pipeline Stages';

  // Non-reactive
  private stageData: Map<PipelineStage, StageData> = new Map();

  constructor() {
    super({
      widgetName: 'CognitionHistogramWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Initialize stage data with defaults
    const stages: PipelineStage[] = ['rag-build', 'should-respond', 'generate', 'coordination', 'post-response'];
    stages.forEach(stage => {
      this.stageData.set(stage, {
        stage,
        avgDuration: 0,
        percentCapacity: 0,
        percentSpeed: 0,
        count: 0,
        lastUpdate: Date.now()
      });
    });

    // Subscribe to cognition events
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe(COGNITION_EVENTS.STAGE_COMPLETE, (data: StageCompleteEvent) => {
        this.updateStageData(data);
      });
      return () => unsubscribe();
    });

    // Initial render
    this.renderCurrentMode();
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="histogram-container" @click=${this.handleContainerClick}>
        <div class="histogram-header">
          <span class="mode-label">${this.modeLabel}</span>
        </div>

        <svg class="histogram-svg" viewBox="0 0 100 100" preserveAspectRatio="none">
          <!-- Histogram bars rendered dynamically by renderers -->
        </svg>

        <div class="legend">
          <span class="legend-item"><span class="legend-dot fast"></span>Fast</span>
          <span class="legend-item"><span class="legend-dot normal"></span>Normal</span>
          <span class="legend-item"><span class="legend-dot slow"></span>Slow</span>
          <span class="legend-item"><span class="legend-dot bottleneck"></span>Stuck</span>
        </div>
      </div>
    `;
  }

  // === Event Handlers ===

  private handleContainerClick = (): void => {
    // Cycle through modes
    if (this.mode === 'pipeline') {
      this.mode = 'simple-bars';
      this.modeLabel = 'Simple Bars';
    } else if (this.mode === 'simple-bars') {
      this.mode = 'wave-graph';
      this.modeLabel = 'Wave Graph';
    } else {
      this.mode = 'pipeline';
      this.modeLabel = 'Pipeline Stages';
    }

    this.requestUpdate();
    this.renderCurrentMode();
  };

  // === Data Management ===

  /**
   * Update stage data with new event
   */
  private updateStageData(event: StageCompleteEvent): void {
    const existing = this.stageData.get(event.stage);
    if (!existing) return;

    // Running average
    const newCount = existing.count + 1;
    const newAvgDuration = (existing.avgDuration * existing.count + event.metrics.durationMs) / newCount;

    this.stageData.set(event.stage, {
      stage: event.stage,
      avgDuration: newAvgDuration,
      percentCapacity: event.metrics.percentCapacity,
      percentSpeed: event.metrics.percentSpeed,
      count: newCount,
      lastUpdate: Date.now()
    });

    // Render only when data changes (not 60fps infinite loop!)
    this.renderCurrentMode();
  }

  /**
   * Get SVG and legend elements from shadow DOM
   */
  private getChartElements(): { svg: SVGElement; legend: HTMLElement } | null {
    const svg = this.shadowRoot?.querySelector('.histogram-svg') as SVGElement;
    const legend = this.shadowRoot?.querySelector('.legend') as HTMLElement;
    if (!svg) return null;
    return { svg, legend };
  }

  /**
   * Build chart data from current stage data
   */
  private buildChartData(stages?: PipelineStage[]): ChartData[] {
    if (stages) {
      // Specific stages (for pipeline view)
      return stages.map(stage => {
        const data = this.stageData.get(stage);
        return {
          percentCapacity: data?.percentCapacity ?? 0,
          percentSpeed: data?.percentSpeed ?? 0,
          count: data?.count ?? 0
        };
      });
    } else {
      // All stages with data (for simple/wave views)
      return Array.from(this.stageData.values())
        .filter(d => d.count > 0)
        .map(d => ({
          percentCapacity: d.percentCapacity,
          percentSpeed: d.percentSpeed,
          count: d.count
        }));
    }
  }

  private renderCurrentMode(): void {
    switch (this.mode) {
      case 'pipeline':
        this.renderPipelineStages();
        break;
      case 'simple-bars':
        this.renderSimpleBars();
        break;
      case 'wave-graph':
        this.renderWaveGraph();
        break;
    }
  }

  /**
   * Render pipeline stages as Winamp-style frequency bars
   */
  private renderPipelineStages(): void {
    const elements = this.getChartElements();
    if (!elements) return;

    const stages: PipelineStage[] = ['rag-build', 'should-respond', 'generate', 'coordination', 'post-response'];
    const chartData = this.buildChartData(stages);

    const renderer = new PipelineStagesRenderer(elements.svg, elements.legend);
    renderer.render(chartData);
  }

  /**
   * Render simple unified bar showing average performance
   */
  private renderSimpleBars(): void {
    const elements = this.getChartElements();
    if (!elements) return;

    const chartData = this.buildChartData();

    const renderer = new SimpleBarsRenderer(elements.svg, elements.legend);
    renderer.render(chartData);
  }

  /**
   * Render wave graph showing capacity oscillation
   */
  private renderWaveGraph(): void {
    const elements = this.getChartElements();
    if (!elements) return;

    const chartData = this.buildChartData();

    const renderer = new WaveGraphRenderer(elements.svg, elements.legend);
    renderer.render(chartData);
  }

}

// Registration handled by centralized BROWSER_WIDGETS registry
