/**
 * Widget Daemon - Server-side widget lifecycle management
 * 
 * Uses strongly-typed events from shared/types/WidgetEventTypes.ts
 * Coordinates with browser-side WidgetDaemon and RendererDaemon
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { DAEMON_EVENT_BUS } from '../base/DaemonEventBus';
import { SystemEventType } from '../base/EventTypes';
import { 
  WidgetEventType, 
  WidgetStatus, 
  WidgetEvent,
  WidgetManifest,
  createWidgetEvent,
  isWidgetEvent,
  WidgetDiscoveredEvent,
  WidgetSystemReadyEvent
} from '../../shared/types/WidgetEventTypes';

console.log('🎨 WIDGET_DAEMON_DEBUG: WidgetDaemon.ts module loading...');

export interface WidgetRegistration {
  manifest: WidgetManifest;
  status: WidgetStatus;
  lastUpdate: Date;
  errorCount: number;
  lastError?: string;
}

export class WidgetDaemon extends BaseDaemon {
  public readonly name = 'widget';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.WIDGET;

  private widgets = new Map<string, WidgetRegistration>();
  private eventListeners = new Map<WidgetEventType, Set<Function>>();
  private discoveryInProgress = false;
  private systemReady = false;

  constructor() {
    super();
    console.log('🎨 WIDGET_DAEMON_DEBUG: WidgetDaemon constructor called!');
    console.log('🎨 WIDGET_DAEMON_DEBUG: Widget daemon instance created');
  }

  protected async onStart(): Promise<void> {
    console.log('🎨 WIDGET_DAEMON_DEBUG: onStart() called');
    this.log('🎨 Starting Widget Daemon...');
    this.log('🎨 WIDGET_DAEMON_DEBUG: Widget Daemon constructor completed');
    
    // Listen for session events to trigger widget discovery
    DAEMON_EVENT_BUS.onEvent(SystemEventType.SESSION_CREATED, async (event) => {
      this.log(`🎨 WIDGET_DAEMON_DEBUG: SESSION_CREATED event received for ${event.sessionId}`);
      await this.handleSessionCreated(event.sessionId);
    });

    // Widget discovery can be triggered manually or on session creation
    this.log('🎨 WIDGET_DAEMON_DEBUG: Event listeners registered');

    this.log('✅ Widget Daemon ready for widget management');
    this.log('🎨 WIDGET_DAEMON_DEBUG: Widget Daemon fully initialized and ready');
    console.log('🎨 WIDGET_DAEMON_DEBUG: onStart() completed');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Widget Daemon...');
    this.eventListeners.clear();
    this.widgets.clear();
  }

  /**
   * Get message types this daemon handles
   */
  getMessageTypes(): string[] {
    return [
      'widget:discover',
      'widget:register', 
      'widget:unregister',
      'widget:list',
      'widget:status',
      'widget:health_check',
      'widget:emit_event'
    ];
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse<any>> {
    try {
      switch (message.type) {
        case 'widget:discover':
          return await this.handleDiscoverWidgets(message.data);
          
        case 'widget:register':
          return await this.handleRegisterWidget(message.data);
          
        case 'widget:unregister':
          return this.handleUnregisterWidget(message.data);
          
        case 'widget:list':
          return this.handleListWidgets(message.data);
          
        case 'widget:status':
          return this.handleGetWidgetStatus(message.data);
          
        case 'widget:health_check':
          return await this.handleWidgetHealthCheck(message.data);
          
        case 'widget:emit_event':
          return this.handleEmitWidgetEvent(message.data);
          
        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Widget management failed: ${errorMessage}`
      };
    }
  }

  // === MESSAGE HANDLERS ===

  private async handleSessionCreated(sessionId: string): Promise<void> {
    this.log(`📋 New session created: ${sessionId} - preparing widget discovery`);
    this.log(`🎨 WIDGET_DAEMON_DEBUG: handleSessionCreated called for session ${sessionId}`);
    // Could pre-cache widget manifests for faster discovery
    // TODO: Auto-trigger widget discovery here or wait for manual trigger
    this.log(`🎨 WIDGET_DAEMON_DEBUG: Session ${sessionId} preparation complete`);
  }

  private async handleDiscoverWidgets(data: any): Promise<DaemonResponse> {
    this.log(`🎨 WIDGET_DAEMON_DEBUG: handleDiscoverWidgets called with data: ${JSON.stringify(data)}`);
    
    if (this.discoveryInProgress) {
      this.log(`🎨 WIDGET_DAEMON_DEBUG: Discovery already in progress, rejecting request`);
      return {
        success: false,
        error: 'Widget discovery already in progress'
      };
    }

    this.discoveryInProgress = true;
    this.log(`🎨 WIDGET_DAEMON_DEBUG: Discovery started, setting discoveryInProgress=true`);
    
    try {
      const startTime = Date.now();
      const { paths = ['src/ui/components'] } = data;
      
      this.log(`🔍 Starting widget discovery in paths: ${paths.join(', ')}`);
      this.log(`🎨 WIDGET_DAEMON_DEBUG: Discovery paths: ${JSON.stringify(paths)}`);
      
      // Emit discovery start event
      this.log(`🎨 WIDGET_DAEMON_DEBUG: Emitting WIDGET_DISCOVERED event`);
      this.emitWidgetEvent(createWidgetEvent<WidgetDiscoveredEvent>(
        WidgetEventType.WIDGET_DISCOVERED,
        {
          widgetId: 'discovery',
          widgetType: 'system',
          element: 'discovery-process'
        },
        'server'
      ));

      // TODO: Implement actual widget discovery logic
      // For now, simulate discovery
      this.log(`🎨 WIDGET_DAEMON_DEBUG: Starting performWidgetDiscovery`);
      const discoveredWidgets = await this.performWidgetDiscovery(paths);
      
      const endTime = Date.now();
      const discoveryTime = endTime - startTime;
      
      // Emit system ready event
      this.emitWidgetEvent(createWidgetEvent<WidgetSystemReadyEvent>(
        WidgetEventType.WIDGET_SYSTEM_READY,
        {
          totalWidgets: discoveredWidgets.length,
          loadedWidgets: discoveredWidgets.filter(w => w.status === WidgetStatus.READY).length,
          failedWidgets: discoveredWidgets.filter(w => w.status === WidgetStatus.ERROR).length,
          readyTime: discoveryTime
        },
        'server'
      ));
      
      this.systemReady = true;
      
      return {
        success: true,
        data: {
          discoveredWidgets: discoveredWidgets.length,
          readyWidgets: discoveredWidgets.filter(w => w.status === WidgetStatus.READY).length,
          failedWidgets: discoveredWidgets.filter(w => w.status === WidgetStatus.ERROR).length,
          discoveryTime
        }
      };
      
    } finally {
      this.discoveryInProgress = false;
    }
  }

  private async handleRegisterWidget(data: any): Promise<DaemonResponse> {
    const { manifest } = data;
    
    if (!manifest || !manifest.id) {
      return {
        success: false,
        error: 'Widget manifest with id is required'
      };
    }

    const registration: WidgetRegistration = {
      manifest,
      status: WidgetStatus.DISCOVERED,
      lastUpdate: new Date(),
      errorCount: 0
    };

    this.widgets.set(manifest.id, registration);
    
    this.log(`📦 Registered widget: ${manifest.name} (${manifest.id})`);
    
    return {
      success: true,
      data: {
        widgetId: manifest.id,
        status: registration.status,
        message: 'Widget registered successfully'
      }
    };
  }

  private handleUnregisterWidget(data: any): DaemonResponse {
    const { widgetId } = data;
    
    if (!widgetId) {
      return {
        success: false,
        error: 'widgetId is required'
      };
    }

    if (!this.widgets.has(widgetId)) {
      return {
        success: false,
        error: `Widget ${widgetId} not found`
      };
    }

    this.widgets.delete(widgetId);
    this.log(`🗑️ Unregistered widget: ${widgetId}`);
    
    return {
      success: true,
      data: {
        widgetId,
        message: 'Widget unregistered successfully'
      }
    };
  }

  private handleListWidgets(data: any): DaemonResponse {
    const { status } = data;
    
    let widgets = Array.from(this.widgets.values());
    
    if (status) {
      widgets = widgets.filter(w => w.status === status);
    }
    
    return {
      success: true,
      data: {
        widgets: widgets.map(w => ({
          id: w.manifest.id,
          name: w.manifest.name,
          status: w.status,
          lastUpdate: w.lastUpdate,
          errorCount: w.errorCount
        })),
        total: widgets.length,
        systemReady: this.systemReady
      }
    };
  }

  private handleGetWidgetStatus(data: any): DaemonResponse {
    const { widgetId } = data;
    
    if (!widgetId) {
      return {
        success: false,
        error: 'widgetId is required'
      };
    }

    const widget = this.widgets.get(widgetId);
    
    if (!widget) {
      return {
        success: false,
        error: `Widget ${widgetId} not found`
      };
    }

    return {
      success: true,
      data: {
        id: widget.manifest.id,
        name: widget.manifest.name,
        ...widget,
        manifest: widget.manifest
      }
    };
  }

  private async handleWidgetHealthCheck(data: any): Promise<DaemonResponse> {
    const { widgetId } = data;
    
    if (widgetId) {
      // Check specific widget
      const widget = this.widgets.get(widgetId);
      if (!widget) {
        return {
          success: false,
          error: `Widget ${widgetId} not found`
        };
      }
      
      // TODO: Implement actual health check logic
      const isHealthy = widget.status === WidgetStatus.READY && widget.errorCount === 0;
      
      return {
        success: true,
        data: {
          widgetId,
          healthy: isHealthy,
          status: widget.status,
          errorCount: widget.errorCount,
          lastError: widget.lastError
        }
      };
    } else {
      // Check all widgets
      const healthReport = Array.from(this.widgets.values()).map(widget => ({
        id: widget.manifest.id,
        name: widget.manifest.name,
        healthy: widget.status === WidgetStatus.READY && widget.errorCount === 0,
        ...{ status: widget.status, errorCount: widget.errorCount }
      }));
      
      return {
        success: true,
        data: {
          totalWidgets: this.widgets.size,
          healthyWidgets: healthReport.filter(w => w.healthy).length,
          unhealthyWidgets: healthReport.filter(w => !w.healthy).length,
          widgets: healthReport
        }
      };
    }
  }

  private handleEmitWidgetEvent(data: any): DaemonResponse {
    const { event } = data;
    
    if (!isWidgetEvent(event)) {
      return {
        success: false,
        error: 'Invalid widget event format'
      };
    }
    
    this.emitWidgetEvent(event);
    
    return {
      success: true,
      data: {
        eventType: event.type,
        message: 'Widget event emitted successfully'
      }
    };
  }

  // === WIDGET EVENT EMITTER IMPLEMENTATION ===

  emitWidgetEvent<T extends WidgetEvent>(event: T): void {
    const listeners = this.eventListeners.get(event.type);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(event);
        } catch (error) {
          this.log(`⚠️ Widget event listener error: ${error}`, 'warn');
        }
      });
    }
    
    this.log(`📡 Emitted widget event: ${event.type}`);
  }

  onWidgetEvent<T extends WidgetEventType>(
    eventType: T, 
    listener: (event: Extract<WidgetEvent, { type: T }>) => void
  ): void {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, new Set());
    }
    this.eventListeners.get(eventType)!.add(listener);
    
    this.log(`📡 Added listener for ${eventType} (${this.eventListeners.get(eventType)!.size} total)`);
  }

  offWidgetEvent<T extends WidgetEventType>(
    eventType: T,
    listener: (event: Extract<WidgetEvent, { type: T }>) => void
  ): void {
    const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.eventListeners.delete(eventType);
      }
    }
  }

  // === PRIVATE HELPER METHODS ===

  private async performWidgetDiscovery(paths: string[]): Promise<WidgetRegistration[]> {
    // TODO: Implement actual widget discovery
    this.log(`🔍 Widget discovery simulation in paths: ${paths.join(', ')}`);
    // For now, return mock data to test the type system
    
    const mockWidgets: WidgetRegistration[] = [
      {
        manifest: {
          id: 'chat-widget',
          name: 'Chat Widget',
          path: 'src/ui/components/Chat',
          config: {
            name: 'Chat Widget',
            version: '1.0.0',
            type: 'ui'
          },
          discovered: new Date().toISOString()
        },
        status: WidgetStatus.READY,
        lastUpdate: new Date(),
        errorCount: 0
      },
      {
        manifest: {
          id: 'sidebar-widget',
          name: 'Sidebar Widget', 
          path: 'src/ui/components/Sidebar',
          config: {
            name: 'Sidebar Widget',
            version: '1.0.0',
            type: 'ui'
          },
          discovered: new Date().toISOString()
        },
        status: WidgetStatus.READY,
        lastUpdate: new Date(),
        errorCount: 0
      }
    ];

    // Register the discovered widgets
    for (const widget of mockWidgets) {
      this.widgets.set(widget.manifest.id, widget);
    }

    return mockWidgets;
  }

}