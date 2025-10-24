/**
 * CognitionHistogram Widget - AI Pipeline Stages Visualization
 *
 * Winamp-style frequency bars showing 5 pipeline stages:
 * rag-build, should-respond, generate, coordination, post-response
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { COGNITION_EVENTS, type StageCompleteEvent, type PipelineStage, BASELINE_SPEEDS } from '../../system/conversation/shared/CognitionEventTypes';

interface StageData {
  stage: PipelineStage;
  avgDuration: number;
  percentCapacity: number;
  percentSpeed: number;
  count: number;
  lastUpdate: number;
}

export class CognitionHistogramWidget extends BaseWidget {
  private stageData: Map<PipelineStage, StageData> = new Map();
  private animationFrame: number | null = null;

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
    console.log('🧠 CognitionHistogram: Initializing...');

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

    console.log('✅ CognitionHistogram: Initialized');
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
  }

  /**
   * Start animation loop for smooth histogram updates
   */
  private startAnimationLoop(): void {
    const animate = () => {
      this.renderPipelineStages();
      this.animationFrame = requestAnimationFrame(animate);
    };
    animate();
  }

  /**
   * Render pipeline stages as Winamp-style frequency bars
   */
  private renderPipelineStages(): void {
    const svg = this.shadowRoot?.querySelector('.histogram-svg') as SVGElement;
    if (!svg) return;

    svg.innerHTML = '';

    const stages: PipelineStage[] = ['rag-build', 'should-respond', 'generate', 'coordination', 'post-response'];
    const barWidth = 100 / stages.length;
    const maxHeight = 100;

    stages.forEach((stage, index) => {
      const data = this.stageData.get(stage);
      if (!data || data.count === 0) return;

      const height = (data.percentCapacity / 100) * maxHeight;
      const x = index * barWidth;
      const y = maxHeight - height;

      // Color based on percentSpeed
      const color = data.percentSpeed >= 80 ? '#0f0' :  // Fast (green)
                   data.percentSpeed >= 50 ? '#ff0' :  // Normal (yellow)
                   data.percentSpeed >= 25 ? '#fa0' :  // Slow (orange)
                   '#f00';                             // Bottleneck (red)

      // Create bar
      const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
      rect.setAttribute('x', `${x}%`);
      rect.setAttribute('y', `${y}%`);
      rect.setAttribute('width', `${barWidth * 0.8}%`);
      rect.setAttribute('height', `${height}%`);
      rect.setAttribute('fill', color);
      rect.setAttribute('opacity', '0.8');
      rect.setAttribute('rx', '2');

      // Add glow effect
      rect.style.filter = `drop-shadow(0 0 4px ${color})`;

      svg.appendChild(rect);

      // Add stage label at bottom (only if there's space)
      if (height < 90) {
        const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        text.setAttribute('x', `${x + barWidth * 0.4}%`);
        text.setAttribute('y', '96');
        text.setAttribute('text-anchor', 'middle');
        text.setAttribute('font-size', '6');
        text.setAttribute('fill', '#666');
        text.textContent = stage.split('-')[0]; // First word only
        svg.appendChild(text);
      }
    });
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
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/cognition-histogram/public/${filename}`;
  }
}

// Register widget
customElements.define('cognition-histogram-widget', CognitionHistogramWidget);
