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

        console.log(`🧠 CognitionHistogram: Switched to ${this.mode} mode`);
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
  }

  /**
   * Start animation loop for smooth histogram updates
   */
  private startAnimationLoop(): void {
    const animate = () => {
      this.renderCurrentMode();
      this.animationFrame = requestAnimationFrame(animate);
    };
    animate();
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
    const svg = this.shadowRoot?.querySelector('.histogram-svg') as SVGElement;
    if (!svg) return;

    svg.innerHTML = '';

    const stages: PipelineStage[] = ['rag-build', 'should-respond', 'generate', 'coordination', 'post-response'];
    const barWidth = 100 / stages.length;
    const maxHeight = 100;
    const maxBarHeight = 70;  // Limit bars to 70% of container height

    stages.forEach((stage, index) => {
      const data = this.stageData.get(stage);
      if (!data || data.count === 0) return;

      // Scale height to maxBarHeight (70%) instead of full container
      const height = (data.percentCapacity / 100) * maxBarHeight;
      const x = index * barWidth;
      const y = maxHeight - height;

      // Color based on percentSpeed
      const color = data.percentSpeed >= 80 ? '#0f0' :  // Fast (green)
                   data.percentSpeed >= 50 ? '#ff0' :  // Normal (yellow)
                   data.percentSpeed >= 25 ? '#fa0' :  // Slow (orange)
                   '#f00';                             // Bottleneck (red)

      // Create bar (thinner bars at 60% width, centered)
      const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
      const barWidthPercent = barWidth * 0.6;
      const xCentered = x + (barWidth - barWidthPercent) / 2;
      rect.setAttribute('x', `${xCentered}%`);
      rect.setAttribute('y', `${y}%`);
      rect.setAttribute('width', `${barWidthPercent}%`);
      rect.setAttribute('height', `${height}%`);
      rect.setAttribute('fill', color);
      rect.setAttribute('opacity', '0.8');
      rect.setAttribute('rx', '2');

      // Add glow effect
      rect.style.filter = `drop-shadow(0 0 4px ${color})`;

      svg.appendChild(rect);
    });
  }

  /**
   * Render simple unified bar showing average performance
   */
  private renderSimpleBars(): void {
    const svg = this.shadowRoot?.querySelector('.histogram-svg') as SVGElement;
    if (!svg) return;

    svg.innerHTML = '';

    // Calculate overall average
    const stages = Array.from(this.stageData.values()).filter(d => d.count > 0);
    if (stages.length === 0) return;

    const avgSpeed = stages.reduce((sum, d) => sum + d.percentSpeed, 0) / stages.length;
    const avgCapacity = stages.reduce((sum, d) => sum + d.percentCapacity, 0) / stages.length;

    const height = (avgCapacity / 100) * 70; // Max 70% height
    const y = 100 - height;

    // Color based on average speed
    const color = avgSpeed >= 80 ? '#0f0' :
                 avgSpeed >= 50 ? '#ff0' :
                 avgSpeed >= 25 ? '#fa0' : '#f00';

    const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    rect.setAttribute('x', '35%');
    rect.setAttribute('y', `${y}%`);
    rect.setAttribute('width', '30%');
    rect.setAttribute('height', `${height}%`);
    rect.setAttribute('fill', color);
    rect.setAttribute('opacity', '0.8');
    rect.setAttribute('rx', '4');
    rect.style.filter = `drop-shadow(0 0 6px ${color})`;

    svg.appendChild(rect);
  }

  /**
   * Render wave graph showing capacity oscillation
   */
  private renderWaveGraph(): void {
    const svg = this.shadowRoot?.querySelector('.histogram-svg') as SVGElement;
    if (!svg) return;

    svg.innerHTML = '';

    const stages = Array.from(this.stageData.values()).filter(d => d.count > 0);
    if (stages.length < 2) return;

    const points = stages.map((data, index) => {
      const x = (index / (stages.length - 1)) * 100;
      const y = 100 - ((data.percentCapacity / 100) * 70); // Max 70% height
      return `${x},${y}`;
    }).join(' ');

    const polyline = document.createElementNS('http://www.w3.org/2000/svg', 'polyline');
    polyline.setAttribute('points', points);
    polyline.setAttribute('fill', 'none');
    polyline.setAttribute('stroke', '#ffd700');
    polyline.setAttribute('stroke-width', '3');
    polyline.setAttribute('stroke-linecap', 'round');
    polyline.setAttribute('stroke-linejoin', 'round');
    polyline.style.filter = 'drop-shadow(0 0 4px rgba(255, 215, 0, 0.6))';

    svg.appendChild(polyline);
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
