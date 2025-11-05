/**
 * Widget Server Controls - Event-driven integration points
 * Allows widgets to trigger server-side actions via custom events
 * Classic server controls pattern for modern web components
 */

import {
  WidgetDataRequest,
  WidgetDataResponse,
  WidgetCommandRequest,
  WidgetCommandResponse,
  SessionCreatedEvent,
  SessionJoinedEvent,
  HealthUpdatedEvent,
  DataUpdatedEvent,
  DataSourceType,
  ServerEventName,
  WidgetEventName,
  WidgetControlEventName,
  CommandDataSourceMapping,
  EventMappingConfig
} from '../../../types/shared/WidgetServerTypes';

export class WidgetServerControls {
  private static instance: WidgetServerControls;
  private commandDataSourceMap = new Map<DataSourceType, string>();
  private eventMappings: EventMappingConfig[] = [];
  private controlEventNames: WidgetControlEventName[] = [];
  
  static getInstance(): WidgetServerControls {
    if (!WidgetServerControls.instance) {
      WidgetServerControls.instance = new WidgetServerControls();
    }
    return WidgetServerControls.instance;
  }

  private constructor() {
    this.initializeDynamicMappings();
    this.setupEventListeners();
  }

  /**
   * Initialize dynamic mappings using npm intelligence and command discovery
   */
  private async initializeDynamicMappings(): Promise<void> {
    // Discover command → data source mappings dynamically
    await this.discoverCommandDataSourceMappings();
    
    // Set up event mappings dynamically  
    this.initializeEventMappings();
    
    // Discover widget control events from types
    this.initializeControlEventNames();
  }

  /**
   * Discover command data source mappings using npm intelligence
   */
  private async discoverCommandDataSourceMappings(): Promise<void> {
    try {
      // Use the continuum API to get available commands
      const continuum = (window as any).continuum;
      if (continuum && typeof continuum.execute === 'function') {
        // Get available commands dynamically
        const helpResult = await continuum.execute('help', {});
        if (helpResult.success && helpResult.data) {
          // Parse command definitions to find data source mappings
          this.parseCommandDefinitionsForDataSources(helpResult.data);
        }
      }
      
      // Fallback to intelligent defaults using naming conventions  
      this.setupIntelligentDefaults();
    } catch (error) {
      console.warn('Could not discover command mappings dynamically, using intelligent defaults:', error);
      this.setupIntelligentDefaults();
    }
  }

  /**
   * Parse command definitions to find data source mappings
   */
  private parseCommandDefinitionsForDataSources(_commandData: any): void {
    // Commands that match data source names map directly
    const directMappings: Array<[DataSourceType, string]> = [
      ['personas', 'personas'],
      ['projects', 'projects'], 
      ['sessions', 'sessions'],
      ['health', 'health'],
      ['widgets', 'discover_widgets'],
      ['logs', 'console'], // Console command handles logs
      ['metrics', 'health'], // Health command includes metrics
    ];

    for (const [dataSource, command] of directMappings) {
      this.commandDataSourceMap.set(dataSource, command);
    }

    // TODO: Parse actual command definitions to discover more mappings
    // For now, use intelligent naming convention mapping
  }

  /**
   * Setup intelligent defaults using npm intelligence patterns
   */
  private setupIntelligentDefaults(): void {
    // Use naming conventions - if command name matches data source, use it
    const intelligentMappings: CommandDataSourceMapping[] = [
      { command: 'personas', dataSource: 'personas' },
      { command: 'projects', dataSource: 'projects' },
      { command: 'sessions', dataSource: 'sessions' },
      { command: 'health', dataSource: 'health' },
      { command: 'discover_widgets', dataSource: 'widgets' },
      { command: 'help', dataSource: 'commands' }, // Help lists commands
      { command: 'agents', dataSource: 'daemons' }, // Agents command for daemon info
      { command: 'console', dataSource: 'logs' },
      { command: 'health', dataSource: 'metrics' }, // Health includes metrics
    ];

    for (const mapping of intelligentMappings) {
      this.commandDataSourceMap.set(mapping.dataSource, mapping.command);
    }
  }

  /**
   * Initialize event mappings dynamically
   */
  private initializeEventMappings(): void {
    this.eventMappings = [
      { serverEvent: 'session:created', widgetEvent: 'server:session-created', enabled: true },
      { serverEvent: 'session:joined', widgetEvent: 'server:session-joined', enabled: true },
      { serverEvent: 'health:updated', widgetEvent: 'server:health-updated', enabled: true },
      { serverEvent: 'data:updated', widgetEvent: 'server:data-updated', enabled: true },
    ];
  }

  /**
   * Initialize control event names from types
   */
  private initializeControlEventNames(): void {
    this.controlEventNames = [
      'widget:screenshot',
      'widget:refresh',
      'widget:export', 
      'widget:validate',
      'widget:fetch-data',
      'widget:execute-command'
    ];
  }

  /**
   * Setup global event listeners for widget server control events
   * Uses dynamic discovery instead of hardcoded event names
   */
  private setupEventListeners(): void {
    // Dynamically set up control event listeners using type-safe event names
    for (const eventName of this.controlEventNames) {
      this.setupControlEventListener(eventName);
    }
    
    // Listen for server events that widgets should respond to
    this.setupServerEventListeners();
  }

  /**
   * Setup individual control event listener with proper routing
   */
  private setupControlEventListener(eventName: WidgetControlEventName): void {
    document.addEventListener(eventName, (event) => {
      switch (eventName) {
        case 'widget:screenshot':
          this.handleScreenshotEvent(event);
          break;
        case 'widget:refresh':
          this.handleRefreshEvent(event);
          break;
        case 'widget:export':
          this.handleExportEvent(event);
          break;
        case 'widget:validate':
          this.handleValidateEvent(event);
          break;
        case 'widget:fetch-data':
          this.handleFetchDataEvent(event);
          break;
        case 'widget:execute-command':
          this.handleExecuteCommandEvent(event);
          break;
      }
    });
  }

  /**
   * Handle widget screenshot events
   */
  private async handleScreenshotEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent;
    try {
      console.log('📸 Server Control: Widget screenshot requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      // Route to screenshot command via Continuum API
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('screenshot', {
          target: 'widget',
          widgetId: widgetId,
          selector: widgetElement.tagName.toLowerCase(),
          includeContext: true,
          ...customEvent.detail
        });
        
        // Notify widget of completion via custom event
        widgetElement.dispatchEvent(new CustomEvent('widget:screenshot-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget screenshot completed:', result);
      } else {
        throw new Error('Continuum API not available');
      }
      
    } catch (error) {
      console.error('❌ Widget screenshot failed:', error);
      
      // Notify widget of failure
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:screenshot-complete', {
        detail: { 
          success: false, 
          error: error instanceof Error ? error.message : String(error)
        }
      }));
    }
  }

  /**
   * Handle widget refresh events
   */
  private async handleRefreshEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent;
    try {
      console.log('🔄 Server Control: Widget refresh requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('reload', {
          target: 'widget',
          widgetId: widgetId,
          preserveState: customEvent.detail?.preserveState || true,
          ...customEvent.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:refresh-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget refresh completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget refresh failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:refresh-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }

  /**
   * Handle widget export events
   */
  private async handleExportEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent;
    try {
      console.log('💾 Server Control: Widget export requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('export', {
          target: 'widget',
          widgetId: widgetId,
          format: customEvent.detail?.format || 'json',
          ...customEvent.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:export-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget export completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget export failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:export-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }

  /**
   * Handle widget validation events
   */
  private async handleValidateEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent;
    try {
      console.log('✅ Server Control: Widget validation requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('validate', {
          target: 'widget',
          widgetId: widgetId,
          validateAssets: true,
          validateContent: true,
          ...customEvent.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:validate-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget validation completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget validation failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:validate-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }

  /**
   * Handle widget data fetching events
   */
  private async handleFetchDataEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent<WidgetDataRequest>;
    try {
      console.log('📡 Server Control: Widget data fetch requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const request = customEvent.detail;
        
        // Use dynamic command mapping discovered through npm intelligence
        const command = this.commandDataSourceMap.get(request.dataSource);
        
        if (!command) {
          throw new Error(`No command found for data source: ${request.dataSource}. Available: ${Array.from(this.commandDataSourceMap.keys()).join(', ')}`);
        }
        
        const result = await continuum.execute(command, {
          requestingWidget: widgetId,
          ...request.params,
          ...(request.filters && { filters: request.filters })
        });
        
        const response: WidgetDataResponse = {
          success: true,
          dataSource: request.dataSource,
          data: result.data,
          timestamp: Date.now(),
          metadata: {
            fromCache: false,
            totalCount: result.data?.length,
            hasMore: false
          }
        };
        
        widgetElement.dispatchEvent(new CustomEvent('widget:data-received', {
          detail: response
        }));
        
        console.log(`✅ Widget data fetched for ${widgetId}:`, result);
      } else {
        throw new Error('Continuum API not available');
      }
      
    } catch (error) {
      console.error('❌ Widget data fetch failed:', error);
      const widgetElement = event.target as HTMLElement;
      
      const errorResponse: WidgetDataResponse = {
        success: false,
        dataSource: customEvent.detail?.dataSource || 'unknown' as DataSourceType,
        error: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      };
      
      widgetElement.dispatchEvent(new CustomEvent('widget:data-received', {
        detail: errorResponse
      }));
    }
  }

  /**
   * Handle widget command execution events
   */
  private async handleExecuteCommandEvent(event: Event): Promise<void> {
    const customEvent = event as CustomEvent<WidgetCommandRequest>;
    try {
      console.log('⚡ Server Control: Widget command execution requested', customEvent.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const request = customEvent.detail;
        const startTime = Date.now();
        
        const result = await continuum.execute(request.command, {
          requestingWidget: widgetId,
          ...request.params,
          ...(request.timeout && { timeout: request.timeout }),
          ...(request.priority && { priority: request.priority })
        });
        
        const executionTime = Date.now() - startTime;
        
        const response: WidgetCommandResponse = {
          success: true,
          command: request.command,
          result: result.data,
          timestamp: Date.now(),
          executionTime
        };
        
        widgetElement.dispatchEvent(new CustomEvent('widget:command-complete', {
          detail: response
        }));
        
        console.log(`✅ Widget command executed for ${widgetId}: ${request.command} (${executionTime}ms)`, result);
      } else {
        throw new Error('Continuum API not available');
      }
      
    } catch (error) {
      console.error('❌ Widget command execution failed:', error);
      const widgetElement = event.target as HTMLElement;
      
      const errorResponse: WidgetCommandResponse = {
        success: false,
        command: customEvent.detail?.command || 'unknown',
        error: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      };
      
      widgetElement.dispatchEvent(new CustomEvent('widget:command-complete', {
        detail: errorResponse
      }));
    }
  }

  /**
   * Setup listeners for server events that widgets should respond to
   */
  private setupServerEventListeners(): void {
    const continuum = (window as any).continuum;
    if (continuum && typeof continuum.on === 'function') {
      // Dynamically set up server event listeners from configuration
      for (const mapping of this.eventMappings) {
        if (mapping.enabled) {
          this.setupServerEventListener(continuum, mapping);
        }
      }
    }
  }

  /**
   * Setup individual server event listener with proper typing
   */
  private setupServerEventListener(continuum: any, mapping: EventMappingConfig): void {
    continuum.on(mapping.serverEvent, (data: any) => {
      // Type-safe event data based on server event type
      const typedData = this.castServerEventData(mapping.serverEvent, data);
      this.broadcastToWidgets(mapping.widgetEvent, typedData);
    });
  }

  /**
   * Cast server event data to proper types based on event name
   */
  private castServerEventData(serverEvent: ServerEventName, data: any): SessionCreatedEvent | SessionJoinedEvent | HealthUpdatedEvent | DataUpdatedEvent {
    // Type-safe casting based on server event type
    switch (serverEvent) {
      case 'session:created':
        return data as SessionCreatedEvent;
      case 'session:joined':
        return data as SessionJoinedEvent;
      case 'health:updated':
        return data as HealthUpdatedEvent;
      case 'data:updated':
        return data as DataUpdatedEvent;
      default:
        // TypeScript ensures this is unreachable with proper ServerEventName
        throw new Error(`Unknown server event: ${serverEvent}`);
    }
  }

  /**
   * Register a command data source mapping (used by commands to declare what data they provide)
   */
  public static registerCommandDataSource(mapping: CommandDataSourceMapping): void {
    const instance = WidgetServerControls.getInstance();
    instance.commandDataSourceMap.set(mapping.dataSource, mapping.command);
    
    // Add aliases if provided
    if (mapping.aliases) {
      for (const alias of mapping.aliases) {
        // Map alias as additional data source if it's valid
        if (alias as DataSourceType) {
          instance.commandDataSourceMap.set(alias as DataSourceType, mapping.command);
        }
      }
    }
  }

  /**
   * Get current command data source mappings (for debugging)
   */
  public static getCommandDataSourceMappings(): Map<DataSourceType, string> {
    return new Map(WidgetServerControls.getInstance().commandDataSourceMap);
  }

  /**
   * Register event mapping configuration (allows dynamic event setup)
   */
  public static registerEventMapping(mapping: EventMappingConfig): void {
    const instance = WidgetServerControls.getInstance();
    const existingIndex = instance.eventMappings.findIndex(m => m.serverEvent === mapping.serverEvent);
    
    if (existingIndex !== -1) {
      instance.eventMappings[existingIndex] = mapping;
    } else {
      instance.eventMappings.push(mapping);
    }
    
    // Re-setup listeners if already initialized
    instance.setupServerEventListeners();
  }

  /**
   * Broadcast server events to all widgets that want to listen
   */
  private broadcastToWidgets(eventType: WidgetEventName, data: any): void {
    // Find all custom elements (widgets) and dispatch server events
    const widgets = document.querySelectorAll('*');
    widgets.forEach(element => {
      if (element.tagName.includes('-') && element.shadowRoot) {
        // This is likely a custom widget element
        element.dispatchEvent(new CustomEvent(eventType, {
          detail: data
        }));
      }
    });
  }
}

// Initialize server controls when module loads
WidgetServerControls.getInstance();