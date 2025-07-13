/**
 * Simple Pattern Widget - Clean Extension Example
 * ==============================================
 * Demonstrates clean extension of BaseWidget with single responsibility
 * Shows all dynamic patterns without god object anti-pattern
 */

import { BaseWidget } from '../shared/BaseWidget';
import { 
  DataSourceType, 
  DataUpdatedEvent, 
  WidgetCapabilities,
  SessionCreatedEvent
} from '../../../types/shared/WidgetServerTypes';

export interface SimpleData {
  readonly id: string;
  readonly name: string;
  readonly value: number;
  readonly status: 'active' | 'inactive';
}

export class SimplePatternWidget extends BaseWidget {
  private data: SimpleData[] = [];
  private selectedSource: DataSourceType = 'health';

  // Strong-typed processor mapping (no switch statements)
  private readonly processors: Record<DataSourceType, (data: unknown) => void> = {
    health: (data) => this.processTypedData(data, 'health'),
    widgets: (data) => this.processTypedData(data, 'widgets'),
    daemons: (data) => this.processTypedData(data, 'daemons'),
    commands: (data) => this.processTypedData(data, 'commands'),
    sessions: (data) => this.processTypedData(data, 'sessions'),
    projects: (data) => this.processTypedData(data, 'projects'),
    personas: (data) => this.processTypedData(data, 'personas'),
    logs: (data) => this.processTypedData(data, 'logs'),
    metrics: (data) => this.processTypedData(data, 'metrics')
  } as const;

  constructor() {
    super();
    this.widgetName = 'SimplePattern';
    this.widgetIcon = '✨';
    this.widgetTitle = 'Simple Pattern Demo';
  }

  protected override getWidgetCapabilities(): WidgetCapabilities {
    return {
      canFetchData: ['health', 'widgets'],
      canExecuteCommands: ['health'],
      respondsToEvents: ['data:updated'],
      supportsExport: ['json'],
      requiresAuth: false,
      updateFrequency: 'manual'
    };
  }

  protected override async initializeWidget(): Promise<void> {
    await super.initializeWidget();
    
    // Load module data once (no loops)
    await this.loadModuleData();
    
    // Fetch initial data with elegant parameters
    this.fetchWithElegantParams(this.selectedSource);
  }

  // Single responsibility: elegant parameter handling
  private fetchWithElegantParams(
    dataSource: DataSourceType, 
    overrides: Record<string, unknown> = {}
  ): void {
    const baseRequest = {
      dataSource,
      timestamp: Date.now(),
      priority: 'normal' as const
    };

    // Elegant spread pattern
    const request = { ...baseRequest, ...overrides };
    
    this.fetchServerData(dataSource, request);
  }

  // Strong types eliminate switch statements
  protected override processServerData(dataSource: DataSourceType, data: unknown): void {
    const processor = this.processors[dataSource];
    processor?.(data);
    this.update();
  }

  // Single method handles all data types with context
  private processTypedData(data: unknown, source: DataSourceType): void {
    console.log(`✨ Processing ${source}:`, data);
    
    // Convert any data to our simple format
    if (Array.isArray(data)) {
      this.data = data.slice(0, 5).map((_item, index) => ({
        id: `${source}-${index}`,
        name: `${source} item ${index}`,
        value: Math.random() * 100,
        status: Math.random() > 0.5 ? 'active' : 'inactive'
      }));
    }
  }

  // Type-safe event handling
  protected override shouldAutoRefreshOnDataUpdate(event: DataUpdatedEvent): boolean {
    return event.dataSource === this.selectedSource;
  }

  protected override onServerSessionCreated(event: SessionCreatedEvent): void {
    console.log(`✨ New session: ${event.sessionType}`);
    this.fetchWithElegantParams(this.selectedSource, { refresh: true });
  }

  protected override onDataFetchError(dataSource: DataSourceType, error: string): void {
    console.error(`✨ Fetch error for ${dataSource}:`, error);
    if (this.data.length === 0) {
      this.loadFallbackData();
    }
  }

  renderContent(): string {
    return `
      <div class="simple-widget">
        <div class="header">
          <span>${this.widgetIcon} ${this.widgetTitle}</span>
          <select class="source-selector">
            ${this.renderSourceOptions()}
          </select>
        </div>
        <div class="content">
          ${this.renderData()}
        </div>
      </div>
    `;
  }

  private renderSourceOptions(): string {
    const sources: DataSourceType[] = ['health', 'widgets', 'daemons'];
    return sources.map(source => 
      `<option value="${source}" ${source === this.selectedSource ? 'selected' : ''}>
        ${source}
      </option>`
    ).join('');
  }

  private renderData(): string {
    return this.data.map(item => `
      <div class="data-item ${item.status}">
        <span class="name">${item.name}</span>
        <span class="value">${item.value.toFixed(1)}</span>
      </div>
    `).join('');
  }

  setupEventListeners(): void {
    if (!this.shadowRoot) return;

    this.shadowRoot.querySelector('.source-selector')?.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      this.selectedSource = target.value as DataSourceType;
      this.fetchWithElegantParams(this.selectedSource);
    });
  }

  private async loadModuleData(): Promise<void> {
    // Simple fallback data - module provides its own
    this.loadFallbackData();
  }

  private loadFallbackData(): void {
    this.data = [
      { id: '1', name: 'Sample Item', value: 42, status: 'active' },
      { id: '2', name: 'Demo Data', value: 75, status: 'inactive' }
    ];
  }
}

// Register the clean, focused widget
if (!customElements.get('simple-pattern')) {
  customElements.define('simple-pattern', SimplePatternWidget);
}

export default SimplePatternWidget;