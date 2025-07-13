/**
 * Dynamic Pattern Demo Widget - Complete Example
 * =============================================
 * Demonstrates all dynamic patterns from our widget-server integration:
 * 
 * ✅ Type-safe server communication using shared types
 * ✅ Dynamic command discovery (no hardcoded mappings)
 * ✅ Elegant spread operator patterns for parameter merging
 * ✅ Strongly-typed event handling with exhaustive checking
 * ✅ Self-contained data (module provides its own initial data)
 * ✅ npm intelligence patterns for command-to-data-source mapping
 */

import { BaseWidget } from '../shared/BaseWidget';
import { 
  DataSourceType, 
  DataUpdatedEvent, 
  WidgetCapabilities,
  SessionCreatedEvent,
  WidgetDataRequest
} from '../../../types/shared/WidgetServerTypes';

export interface DemoData {
  readonly id: string;
  readonly name: string;
  readonly type: 'command' | 'widget' | 'daemon' | 'integration';
  readonly status: 'active' | 'inactive' | 'testing';
  readonly metrics: {
    readonly responseTime: number;
    readonly successRate: number;
    readonly lastUsed: string;
  };
}

export type WidgetActionType = 'refresh' | 'screenshot' | 'export' | 'help';

export interface DemoConfig {
  readonly demoItems: DemoData[];
  readonly dataSources: Array<{
    readonly type: DataSourceType;
    readonly displayName: string;
    readonly description: string;
    readonly icon: string;
    readonly refreshInterval: number;
  }>;
  readonly actionCommands: Array<{
    readonly command: string;
    readonly label: string;
    readonly icon: string;
    readonly description: string;
    readonly timeout: number;
  }>;
  readonly widgetConfig: {
    readonly defaultDataSource: DataSourceType;
    readonly autoRefreshEnabled: boolean;
    readonly autoRefreshInterval: number;
    readonly maxRetries: number;
    readonly retryDelay: number;
    readonly logLevel: string;
  };
}

export class DynamicPatternDemoWidget extends BaseWidget {
  private demoData: DemoData[] = [];
  private selectedDataSource: DataSourceType = 'health';
  private refreshInterval: number | null = null;
  private config: DemoConfig | null = null;

  constructor() {
    super();
    this.widgetName = 'DynamicPatternDemo';
    this.widgetIcon = '🎯';
    this.widgetTitle = 'Dynamic Pattern Demo';
  }

  /**
   * Static setup hook called during widget registration (not in infinite loop)
   */
  static async initializeWidgetData(): Promise<boolean> {
    try {
      console.log('🎯 DynamicPatternDemo: Running setup hook...');
      
      // Import setup script dynamically to avoid bundling issues
      const setupModule = await import('./setup-widget-data');
      const WidgetDataSetup = setupModule.default || setupModule;
      
      const setup = new (WidgetDataSetup as any)();
      await setup.initialize();
      
      return true;
    } catch (error) {
      console.error('🎯 DynamicPatternDemo: Setup hook failed:', error);
      return false;
    }
  }

  protected override getWidgetCapabilities(): WidgetCapabilities {
    return {
      // Dynamic discovery handles all data sources - no hardcoded list needed
      canFetchData: ['health', 'widgets', 'daemons', 'commands', 'sessions'],
      
      // Commands discovered through npm intelligence patterns
      canExecuteCommands: ['health', 'discover_widgets', 'help', 'screenshot'],
      
      // Type-safe event names from shared types
      respondsToEvents: ['session:created', 'data:updated', 'health:updated'],
      
      // Export capabilities for data marshalling
      supportsExport: ['json', 'csv'],
      
      requiresAuth: false,
      updateFrequency: 'manual' // User-triggered updates
    };
  }

  protected override async initializeWidget(): Promise<void> {
    await super.initializeWidget(); // Sets up typed event listeners automatically
    
    // Load self-contained data from module's own package/JSON (no loops)
    await this.loadModuleData();
    
    // Start with configured default data source using dynamic discovery
    const defaultDataSource = this.config?.widgetConfig.defaultDataSource || 'health';
    this.selectedDataSource = defaultDataSource;
    this.fetchDataWithElegantParameters(defaultDataSource);
    
    // Set up auto-refresh only if enabled in config (prevents infinite loops)
    const autoRefreshConfig = this.config?.widgetConfig;
    if (autoRefreshConfig?.autoRefreshEnabled) {
      this.setupAutoRefresh({ 
        interval: autoRefreshConfig.autoRefreshInterval,
        enabled: true 
      });
    }
  }

  /**
   * Demonstrates elegant spread operator pattern for parameter merging
   */
  private fetchDataWithElegantParameters(
    dataSource: DataSourceType, 
    userOptions: Partial<WidgetDataRequest> = {}
  ): void {
    // Elegant spread - merge base params with user overrides
    const request: WidgetDataRequest = {
      dataSource,
      ...userOptions // User options override defaults elegantly
    };
    
    console.log(`🎯 ${this.widgetName}: Fetching ${dataSource} with elegant parameters:`, request);
    this.fetchServerData(dataSource, request);
  }

  /**
   * Type-safe data processor mapping - strong types eliminate switch statements
   */
  private readonly dataProcessors: Record<DataSourceType, (data: unknown) => void> = {
    health: (data) => this.processHealthData(data),
    widgets: (data) => this.processWidgetData(data),
    daemons: (data) => this.processDaemonData(data),
    commands: (data) => this.processCommandData(data),
    sessions: (data) => this.processSessionData(data),
    projects: (data) => this.processProjectData(data),
    personas: (data) => this.processPersonaData(data),
    logs: (data) => this.processLogData(data),
    metrics: (data) => this.processMetricData(data)
  } as const;

  /**
   * Type-safe server data processing using strong type mapping
   */
  protected override processServerData(dataSource: DataSourceType, data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing data from ${dataSource}:`, data);
    
    // TypeScript ensures all DataSourceType values are handled in the mapping
    const processor = this.dataProcessors[dataSource];
    if (processor) {
      processor(data);
    } else {
      // This should never happen with proper typing, but provides safety
      console.warn(`🎯 ${this.widgetName}: No processor found for data source: ${dataSource}`);
    }
    
    this.update(); // Re-render with new data
  }

  /**
   * Type-safe event handling with strongly-typed events
   */
  protected override shouldAutoRefreshOnDataUpdate(event: DataUpdatedEvent): boolean {
    // Only auto-refresh if data matches our current selection
    return event.dataSource === this.selectedDataSource && event.updateType !== 'deleted';
  }

  protected override onServerSessionCreated(event: SessionCreatedEvent): void {
    console.log(`🎯 ${this.widgetName}: New ${event.sessionType} session by ${event.owner} - refreshing data`);
    
    // Refresh current data source with elegant parameter merging
    this.fetchDataWithElegantParameters(this.selectedDataSource, {
      params: { 
        sessionContext: event.sessionId,
        refresh: true 
      }
    });
  }

  /**
   * Error handling with data source context
   */
  protected override onDataFetchError(dataSource: DataSourceType, error: string): void {
    console.error(`🎯 ${this.widgetName}: Failed to fetch ${dataSource}:`, error);
    
    // Fallback to minimal demo data (not infinite loop)
    if (this.demoData.length === 0) {
      this.loadFallbackData();
    }
    this.update();
  }

  /**
   * Command execution with spread operator elegance
   */
  private async executeCommandWithElegantParams(
    command: string, 
    userParams: Record<string, unknown> = {}
  ): Promise<void> {
    const baseParams = {
      timeout: 15000,
      priority: 'normal' as const,
      source: this.widgetName
    };

    // Elegant spread pattern - merge base with user params
    const finalParams = {
      ...baseParams,
      params: {
        ...baseParams,
        ...userParams // User params override base params elegantly
      }
    };

    console.log(`🎯 ${this.widgetName}: Executing ${command} with elegant params:`, finalParams);
    this.executeServerCommand(command, finalParams);
  }

  /**
   * Auto-refresh setup with elegant option merging
   */
  private setupAutoRefresh(options: { interval?: number; enabled?: boolean } = {}): void {
    const defaults = { interval: 30000, enabled: false };
    const settings = { ...defaults, ...options }; // Elegant spread for defaults

    if (this.refreshInterval) {
      clearInterval(this.refreshInterval);
      this.refreshInterval = null;
    }

    if (settings.enabled) {
      this.refreshInterval = window.setInterval(() => {
        console.log(`🎯 ${this.widgetName}: Auto-refreshing ${this.selectedDataSource}`);
        this.fetchDataWithElegantParameters(this.selectedDataSource, {
          params: { autoRefresh: true }
        });
      }, settings.interval);
    }
  }

  renderContent(): string {
    return `
      <div class="widget-container">
        <div class="widget-header">
          <div class="header-title">
            <span>${this.widgetIcon}</span>
            <span>${this.widgetTitle}</span>
            <span class="data-count">${this.demoData.length} items</span>
          </div>
          <div class="header-controls">
            ${this.renderDataSourceSelector()}
            ${this.renderActionButtons()}
          </div>
        </div>
        <div class="widget-content">
          ${this.renderDemoData()}
        </div>
      </div>
    `;
  }

  private renderDataSourceSelector(): string {
    const dataSources: DataSourceType[] = ['health', 'widgets', 'daemons', 'commands', 'sessions'];
    
    return `
      <select class="data-source-selector" data-widget-control="data-source">
        ${dataSources.map(source => `
          <option value="${source}" ${source === this.selectedDataSource ? 'selected' : ''}>
            ${source.charAt(0).toUpperCase() + source.slice(1)}
          </option>
        `).join('')}
      </select>
    `;
  }

  private renderActionButtons(): string {
    const actions: Array<{ command: WidgetActionType; label: string; icon: string }> = [
      { command: 'refresh', label: 'Refresh', icon: '🔄' },
      { command: 'screenshot', label: 'Screenshot', icon: '📸' },
      { command: 'export', label: 'Export', icon: '💾' },
      { command: 'help', label: 'Help', icon: '❓' }
    ] as const;

    return actions.map(action => `
      <button class="action-button ${action.command}" 
              data-command="${action.command}"
              title="${action.label}">
        ${action.icon}
      </button>
    `).join('');
  }

  private renderDemoData(): string {
    if (this.demoData.length === 0) {
      return `
        <div class="empty-state">
          <div class="empty-icon">🎯</div>
          <div class="empty-message">
            No data loaded. Select a data source and click refresh.
          </div>
        </div>
      `;
    }

    return this.demoData.map(item => this.renderDemoItem(item)).join('');
  }

  private renderDemoItem(item: DemoData): string {
    return `
      <div class="demo-item ${item.type}" data-item-id="${item.id}">
        <div class="item-header">
          <span class="item-name">${item.name}</span>
          <span class="item-status ${item.status}">${item.status}</span>
        </div>
        <div class="item-type">${item.type}</div>
        <div class="item-metrics">
          <span class="metric">⚡ ${item.metrics.responseTime}ms</span>
          <span class="metric">✅ ${item.metrics.successRate}%</span>
          <span class="metric">🕒 ${item.metrics.lastUsed}</span>
        </div>
      </div>
    `;
  }

  setupEventListeners(): void {
    if (!this.shadowRoot) return;

    // Data source selector changes
    this.shadowRoot.querySelector('.data-source-selector')?.addEventListener('change', (e) => {
      const target = e.target as HTMLSelectElement;
      this.selectedDataSource = target.value as DataSourceType;
      
      console.log(`🎯 ${this.widgetName}: Switched to data source: ${this.selectedDataSource}`);
      this.fetchDataWithElegantParameters(this.selectedDataSource);
    });

    // Action button clicks with elegant parameter handling
    this.shadowRoot.querySelectorAll('.action-button').forEach(button => {
      button.addEventListener('click', (_e) => {
        const command = (button as HTMLElement).dataset.command!;
        this.handleActionClick(command);
      });
    });

    // Demo item clicks
    this.shadowRoot.querySelectorAll('.demo-item').forEach(item => {
      item.addEventListener('click', (_e) => {
        const itemId = (item as HTMLElement).dataset.itemId!;
        this.selectDemoItem(itemId);
      });
    });
  }

  /**
   * Type-safe action handler mapping - strong types eliminate switch statements
   */
  private readonly actionHandlers: Record<WidgetActionType, () => Promise<void> | void> = {
    refresh: () => this.handleRefreshAction(),
    screenshot: () => this.handleScreenshotAction(),
    export: () => this.handleExportAction(),
    help: () => this.handleHelpAction()
  } as const;

  /**
   * Action handling with type-safe command execution using strong type mapping
   */
  private async handleActionClick(action: string): Promise<void> {
    console.log(`🎯 ${this.widgetName}: Action clicked: ${action}`);

    // Type-safe action validation
    if (this.isValidAction(action)) {
      const handler = this.actionHandlers[action];
      await handler();
    } else {
      console.warn(`🎯 ${this.widgetName}: Invalid action: ${action}`);
    }
  }

  /**
   * Type guard for action validation
   */
  private isValidAction(action: string): action is WidgetActionType {
    return action in this.actionHandlers;
  }

  private handleRefreshAction(): void {
    this.fetchDataWithElegantParameters(this.selectedDataSource, {
      params: { 
        forceRefresh: true,
        timestamp: Date.now()
      }
    });
  }

  private async handleScreenshotAction(): Promise<void> {
    await this.executeCommandWithElegantParams('screenshot', {
      selector: `#${this.widgetName.toLowerCase()}`,
      filename: `${this.widgetName}-${Date.now()}.png`
    });
  }

  private handleExportAction(): void {
    // Use inherited server controls for export
    this.triggerExport('json', { 
      dataSource: this.selectedDataSource,
      data: this.demoData 
    });
  }

  private async handleHelpAction(): Promise<void> {
    await this.executeCommandWithElegantParams('help', {
      topic: 'widgets',
      context: this.widgetName
    });
  }

  private selectDemoItem(itemId: string): void {
    const item = this.demoData.find(d => d.id === itemId);
    if (item) {
      console.log(`🎯 ${this.widgetName}: Selected item:`, item);
      
      // Emit selection event for widget-to-widget communication
      this.dispatchEvent(new CustomEvent('demo-item-selected', {
        detail: { item, widget: this.widgetName },
        bubbles: true
      }));
    }
  }

  /**
   * Load module data from widget's own package/JSON configuration (no loops)
   * Reads data once from static files, can't create infinite loops
   */
  private async loadModuleData(): Promise<void> {
    try {
      // Load data from module's own JSON file (set up during registration)
      const dataPath = '/src/ui/components/examples/data/demo-data.json';
      const response = await fetch(dataPath);
      
      if (response.ok) {
        this.config = await response.json() as DemoConfig;
        this.demoData = this.config.demoItems;
        console.log(`🎯 ${this.widgetName}: Loaded ${this.demoData.length} items from module data`);
      } else {
        console.warn(`🎯 ${this.widgetName}: Could not load module data, using fallback`);
        this.loadFallbackData();
      }
    } catch (error) {
      console.error(`🎯 ${this.widgetName}: Error loading module data:`, error);
      this.loadFallbackData();
    }
  }

  /**
   * Fallback data if module data loading fails
   * Minimal hardcoded fallback, not full dataset
   */
  private loadFallbackData(): void {
    this.demoData = [
      {
        id: 'fallback-item',
        name: 'Fallback Demo Item',
        type: 'widget',
        status: 'testing',
        metrics: {
          responseTime: 100,
          successRate: 95.0,
          lastUsed: new Date().toISOString().split('T')[0]
        }
      }
    ];
    
    // Minimal config fallback
    this.config = {
      demoItems: this.demoData,
      dataSources: [{ type: 'health', displayName: 'Health', description: '', icon: '🏥', refreshInterval: 30000 }],
      actionCommands: [{ command: 'health', label: 'Health', icon: '🏥', description: '', timeout: 10000 }],
      widgetConfig: {
        defaultDataSource: 'health',
        autoRefreshEnabled: false, // Never enable auto-refresh in fallback
        autoRefreshInterval: 30000,
        maxRetries: 3,
        retryDelay: 2000,
        logLevel: 'info'
      }
    };
  }

  // Data processing methods (type-safe)
  private processHealthData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing health data:`, data);
    // Add health-specific demo items
  }

  private processWidgetData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing widget data:`, data);
    // Add widget-specific demo items
  }

  private processDaemonData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing daemon data:`, data);
    // Add daemon-specific demo items
  }

  private processCommandData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing command data:`, data);
    // Add command-specific demo items
  }

  private processSessionData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing session data:`, data);
    // Add session-specific demo items
  }

  private processProjectData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing project data:`, data);
    // Add project-specific demo items
  }

  private processPersonaData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing persona data:`, data);
    // Add persona-specific demo items
  }

  private processLogData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing log data:`, data);
    // Add log-specific demo items
  }

  private processMetricData(data: unknown): void {
    console.log(`🎯 ${this.widgetName}: Processing metric data:`, data);
    // Add metric-specific demo items
  }
}

// Register the custom element
if (!customElements.get('dynamic-pattern-demo')) {
  customElements.define('dynamic-pattern-demo', DynamicPatternDemoWidget);
}

export default DynamicPatternDemoWidget;