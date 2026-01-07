/**
 * CognitionHistogram Widget - AI Pipeline Stages Visualization
 *
 * Winamp-style frequency bars showing 5 pipeline stages:
 * rag-build, should-respond, generate, coordination, post-response
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { COGNITION_EVENTS, type StageCompleteEvent, type PipelineStage, BASELINE_SPEEDS } from '../../system/conversation/shared/CognitionEventTypes';
import { PipelineStagesRenderer } from './PipelineStagesRenderer';
import { SimpleBarsRenderer } from './SimpleBarsRenderer';
import { WaveGraphRenderer } from './WaveGraphRenderer';
import type { ChartData } from './ChartRenderer';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

interface StageData {
  stage: PipelineStage;
  avgDuration: number;
  percentCapacity: number;
  percentSpeed: number;
  count: number;
  lastUpdate: number;
}

type VisualizationMode = 'pipeline' | 'simple-bars' | 'wave-graph';

export class CognitionHistogramWidget extends BaseWidget {
  private stageData: Map<PipelineStage, StageData> = new Map();
  private animationFrame: number | null = null;
  private mode: VisualizationMode = 'pipeline';

  constructor() {
    super({
      widgetId: 'cognition-histogram-widget',
      widgetName: 'CognitionHistogram',
      template: 'cognition-histogram.html',
      styles: 'cognition-histogram.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    verbose() && console.log('🧠 CognitionHistogram: Initializing...');

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

    this.subscribeToCognitionEvents();
    this.startAnimationLoop();

    verbose() && console.log('✅ CognitionHistogram: Initialized');
  }

  private setupClickHandler(): void {
    const container = this.shadowRoot?.querySelector('.histogram-container');
    if (container) {
      container.addEventListener('click', () => {
        // Cycle through modes
        if (this.mode === 'pipeline') {
          this.mode = 'simple-bars';
        } else if (this.mode === 'simple-bars') {
          this.mode = 'wave-graph';
        } else {
          this.mode = 'pipeline';
        }

        verbose() && console.log(`🧠 CognitionHistogram: Switched to ${this.mode} mode`);
        this.updateModeLabel();
        this.renderCurrentMode(); // Re-render with new mode
      });
    }
  }

  private updateModeLabel(): void {
    const label = this.shadowRoot?.querySelector('.mode-label');
    if (label) {
      const modeNames = {
        'pipeline': 'Pipeline Stages',
        'simple-bars': 'Simple Bars',
        'wave-graph': 'Wave Graph'
      };
      label.textContent = modeNames[this.mode];
    }
  }

  protected async onWidgetCleanup(): Promise<void> {
    if (this.animationFrame) {
      cancelAnimationFrame(this.animationFrame);
    }
  }

  /**
   * Subscribe to cognition events from pipeline stages
   */
  private subscribeToCognitionEvents(): void {
    Events.subscribe(COGNITION_EVENTS.STAGE_COMPLETE, (data: StageCompleteEvent) => {
      this.updateStageData(data);
    });
  }

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
   * Start animation loop for smooth histogram updates
   * DISABLED: Was causing massive browser overhead (60fps infinite rendering)
   * Now only renders when data actually changes (see updateStageData)
   */
  private startAnimationLoop(): void {
    // Initial render
    this.renderCurrentMode();

    // No infinite loop - only render on data changes!
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

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS ?? '/* No styles loaded */';
    const template = this.templateHTML ?? '<div>Loading...</div>';

    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${templateString}
    `;

    // Set up click handler AFTER DOM is rendered
    this.setupClickHandler();
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/cognition-histogram/public/${filename}`;
  }
}
