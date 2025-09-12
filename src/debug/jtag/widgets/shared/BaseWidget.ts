/**
 * BaseWidget - Abstract all complexity away from widget implementations
 * 
 * Just like JTAG commands abstract away server/browser coordination,
 * this BaseWidget abstracts away:
 * - Daemon communication (database, router, academy, etc.)
 * - Theme system integration
 * - Event coordination  
 * - State persistence
 * - Error handling
 * - Performance optimization
 * 
 * Subclasses get powerful one-line operations:
 * - this.storeData(key, value) - handles database + caching
 * - this.broadcastEvent(type, data) - handles router + WebSocket
 * - this.notifyAI(message) - handles Academy daemon
 */

// Event types - Rust-like strict typing
import type { ChatEventName, ChatEventDataFor } from '../chat/shared/ChatEventTypes';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import type { FileSaveParams, FileSaveResult } from '../../commands/file/save/shared/FileSaveTypes';
import type { ScreenshotParams, ScreenshotResult } from '../../commands/screenshot/shared/ScreenshotTypes';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import {
  WIDGET_DEFAULTS,
  DATABASE_OPERATIONS,
  ROUTER_OPERATIONS,
  ACADEMY_OPERATIONS,
  WIDGET_EVENTS,
  WIDGET_CHANNELS,
  AI_PERSONAS,
  WIDGET_DIRECTORIES,
  DAEMON_NAMES,
} from './WidgetConstants';

// Global declarations for browser/server compatibility
declare const performance: { now(): number };
declare const document: { 
  createElement(tagName: string): HTMLElement; 
  querySelector(selector: string): HTMLElement | null;
  addEventListener(type: string, listener: EventListener): void;
};
declare const window: {
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
  location?: { href: string };
};

type WidgetData = string | number | boolean | object | null;
type DaemonInstance = Record<string, unknown>;
type SerializedState = Record<string, WidgetData>;

// Elegant typed event emitter - handles specific event types with their data
type EventEmitter = Map<string, Array<(data: WidgetData) => void>>;

interface CachedValue {
  value: WidgetData;
  timestamp: number;
  ttl?: number;
}

interface EventData {
  sourceWidget: string;
  eventType: string;
  data: WidgetData;
}

interface WindowWithJTAG extends Window {
  jtag?: JTAGClient;
}

export interface WidgetConfig {
  // Core settings
  widgetId: string;
  widgetName: string;
  version?: string;
  
  // Theme and appearance
  customTheme?: Record<string, string>;
  compactMode?: boolean;
  
  // Resource files
  template?: string;  // HTML template filename
  styles?: string;    // CSS styles filename
  
  // Data and persistence
  enablePersistence?: boolean;
  cacheData?: boolean;
  syncAcrossDevices?: boolean;
  
  // Integration settings
  enableAI?: boolean;
  enableDatabase?: boolean;
  enableRouterEvents?: boolean;
  enableScreenshots?: boolean;
  
  // Debug and development
  debugMode?: boolean;
  visualDebugging?: boolean;
  performanceMonitoring?: boolean;
}

export interface WidgetState {
  isInitialized: boolean;
  isConnected: boolean;
  hasError: boolean;
  errorMessage?: string;
  lastUpdate: string;
  data: Map<string, WidgetData>;
  cache: Map<string, CachedValue>;
}

export interface WidgetContext {
  environment: 'browser' | 'server' | 'shared';
  sessionId: string;
  userId: string;
  permissions: string[];
  capabilities: string[];
}

export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  protected eventEmitter: EventEmitter = new Map();
  protected dispatcherEventTypes?: Set<string>; // Track event types with active dispatchers
  protected config: WidgetConfig;
  protected state: WidgetState;
  protected context: WidgetContext;
  
  // Daemon connections (abstracted away from subclasses)
  private databaseDaemon?: DaemonInstance;
  private routerDaemon?: DaemonInstance;
  private academyDaemon?: DaemonInstance;
  
  // Performance and caching
  private operationCache = new Map<string, WidgetData>();
  private throttledOperations = new Map<string, number>();
  
  // Template resources (loaded from external files)
  protected templateHTML?: string;
  protected templateCSS?: string;
  
  constructor(config: Partial<WidgetConfig> = {}) {
    super();
    this.attachShadow({ mode: WIDGET_DEFAULTS.SHADOW_MODE });
    
    // Default configuration with smart defaults
    this.config = {
      widgetId: this.generateWidgetId(),
      widgetName: this.constructor.name,
      version: '1.0.0',
      enablePersistence: true,
      cacheData: true,
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true,
      debugMode: false,
      visualDebugging: false,
      performanceMonitoring: true,
      ...config
    };
    
    // Initialize state
    this.state = {
      isInitialized: false,
      isConnected: false,
      hasError: false,
      lastUpdate: new Date().toISOString(),
      data: new Map(),
      cache: new Map()
    };
    
    // Initialize context (would be populated by widget system)
    this.context = {
      environment: 'browser', // Detected automatically
      sessionId: this.generateSessionId(),
      userId: 'current_user', // From user system
      permissions: ['read', 'write'], // From permission system
      capabilities: ['screenshot', 'file/save', 'ai_integration'] // From capability system
    };
  }

  async connectedCallback(): Promise<void> {
    try {
      console.log(`🎨 ${this.config.widgetName}: BaseWidget initialization starting...`);
      
      // 1. Connect to daemon systems (abstracted) WAS DEAD CODE
      //await this.initializeDaemonConnections();
      
      // 3. Restore persisted state (abstracted)
      await this.restorePersistedState();
      
      // 4. Load external resources (template & styles)
      await this.loadResources();
      
      // 5. Let subclass initialize its specific logic
      await this.onWidgetInitialize();
      
      // 6. Render UI (subclass-specific but with base support)
      await this.renderWidget();
      
      // 7. Setup event coordination (abstracted) 3600000
      //await this.initializeEventSystem();
      
      this.state.isInitialized = true;
      this.state.isConnected = true;
      
      console.log(`✅ ${this.config.widgetName}: BaseWidget initialization complete`);
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Initialization failed:`, error);
      this.handleInitializationError(error);
    }
  }

  async disconnectedCallback(): Promise<void> {
    console.log(`🎨 ${this.config.widgetName}: BaseWidget cleanup starting...`);
    
    // Let subclass clean up first
    await this.onWidgetCleanup();
    
    // Persist final state
    await this.persistCurrentState();
    
    // Disconnect from daemons
    this.disconnectFromDaemons();
    
    // Clean up event listeners
    this.eventEmitter.clear();
    this.operationCache.clear();
    this.throttledOperations.clear();
    
    this.state.isConnected = false;
    console.log(`✅ ${this.config.widgetName}: BaseWidget cleanup complete`);
  }

  // === ABSTRACT METHODS - Subclasses must implement ===
  
  /**
   * Subclass-specific initialization
   * Called after daemon connections are established
   */
  protected abstract onWidgetInitialize(): Promise<void>;
  
  /**
   * Render the widget UI
   * BaseWidget provides theme, but subclass provides content
   */
  protected abstract renderWidget(): Promise<void>;
  
  /**
   * Subclass-specific cleanup
   * Called before daemon disconnection
   */
  protected abstract onWidgetCleanup(): Promise<void>;

  // === ABSTRACTED OPERATIONS - One-line power for subclasses ===

  /**
   * Store data with automatic database + cache coordination
   * Like JTAG's this.screenshot() - one call, full coordination
   */
  protected async storeData(key: string, value: WidgetData, _options: { 
    persistent?: boolean;
    broadcast?: boolean;
    ttl?: number;
  } = {}): Promise<boolean> {
    try {
      // const {
      //   persistent = this.config.enablePersistence,
      //   broadcast = this.config.enableRouterEvents,
      //   ttl = 3600000 // 1 hour default
      // } = options;
      
      // 1. Update local cache immediately
      this.state.cache.set(key, { value, timestamp: Date.now(), ttl:3600000 });
      
      // // 2. Store in database if enabled. DEAD CODE detefcted here, JUSt call the database command
      // if (persistent && this.config.enableDatabase) {
      //   // await this.databaseOperation(DATABASE_OPERATIONS.STORE, {
      //   //   widgetId: this.config.widgetId,
      //   //   key,
      //   //   value: JSON.stringify(value),
      //   //   ttl
      //   // });
      // }
      
      // 3. Broadcast change if enabled
      // if (broadcast) {
      //   await this.broadcastEvent(WIDGET_EVENTS.DATA_UPDATED, { key, value });
      // }
      
      // 4. Update widget state
      this.state.data.set(key, value);
      this.state.lastUpdate = new Date().toISOString();
      
      return true;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: storeData failed for key ${key}:`, error);
      return false;
    }
  }

  /**
   * Retrieve data with automatic cache + database coordination
   */
  protected async getData(key: string, defaultValue: WidgetData = null): Promise<WidgetData> {
    try {
      // 1. Check cache first (fastest)
      const cached = this.state.cache.get(key);
      if (cached && this.isCacheValid(cached)) {
        return cached.value;
      }
      
      // 2. Check widget state
      if (this.state.data.has(key)) {
        return this.state.data.get(key)!; // Non-null assertion - has() confirms existence
      }
      
      //DEAD CODE DETECTED HERE:
      // // 3. Load from database if enabled
      // if (this.config.enableDatabase) {
      //   const dbResult = await this.databaseOperation(DATABASE_OPERATIONS.RETRIEVE, {
      //     widgetId: this.config.widgetId,
      //     key
      //   });
        
      //   if (dbResult.success && dbResult.data && typeof dbResult.data === 'object' && 'value' in dbResult.data && typeof (dbResult.data as { value: string }).value === 'string') {
      //     const value = JSON.parse((dbResult.data as { value: string }).value);
          
      //     // Update cache and state
      //     this.state.cache.set(key, { value, timestamp: Date.now() });
      //     this.state.data.set(key, value);
          
      //     return value;
      //   }
      // }
      
      return defaultValue;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: getData failed for key ${key}:`, error);
      return defaultValue;
    }
  }

  // DEAD CODE DETECTED HERE:
  // /**
  //  * Broadcast event with automatic router + WebSocket coordination
  //  * Like JTAG's cross-environment messaging but for widgets
  //  */
  // //DEAD CODE DETECTED HERE: have them call the event system directly
  // protected async broadcastEvent(eventType: string, data: WidgetData, _options: {
  //   targetWidgets?: string[];
  //   excludeSelf?: boolean;
  //   persistent?: boolean;
  // } = {}): Promise<boolean> {
  //   try {
  //     if (!this.config.enableRouterEvents) return false;
      
  //     // DEAD CODE DETECTED HERE:
  //     // const {
  //     //   targetWidgets,
  //     //   excludeSelf = true,
  //     //   persistent = false
  //     // } = options;
      
  //     // const eventData = {
  //     //   sourceWidget: this.config.widgetId,
  //     //   eventType,
  //     //   data,
  //     //   timestamp: new Date().toISOString(),
  //     //   targetWidgets,
  //     //   excludeSelf
  //     // };
      
  //     // // Send via router daemon (handles WebSocket, cross-browser, etc.)
  //     // const result = await this.routerOperation(ROUTER_OPERATIONS.BROADCAST, {
  //     //   channel: WIDGET_CHANNELS.WIDGET_EVENTS,
  //     //   event: eventData,
  //     //   persistent
  //     // });
      
  //     //return Boolean(result.success);

  //     return false;
      
  //   } catch (error) {
  //     console.error(`❌ ${this.config.widgetName}: broadcastEvent failed:`, error);
  //     return false;
  //   }
  // }

  // DEAD CODE DETECTED HERE:
  // /**
  //  * AI integration with automatic Academy daemon coordination
  //  */
  // protected async queryAI(message: string, options: {
  //   persona?: string;
  //   context?: WidgetData;
  //   expectResponse?: boolean;
  // } = {}): Promise<WidgetData> {
  //   try {
  //     if (!this.config.enableAI) {
  //       console.warn(`⚠️ ${this.config.widgetName}: AI disabled for this widget`);
  //       return null;
  //     }
      
  //     const {
  //       persona = AI_PERSONAS.GENERAL_ASSISTANT,
  //       context = this.getAIContext(),
  //       expectResponse = true
  //     } = options;
      
  //     const result = await this.academyOperation(ACADEMY_OPERATIONS.QUERY, {
  //       message,
  //       persona,
  //       context,
  //       widgetId: this.config.widgetId,
  //       expectResponse
  //     });
      
  //     return result.success ? result.data : null;
      
  //   } catch (error) {
  //     console.error(`❌ ${this.config.widgetName}: queryAI failed:`, error);
  //     return null;
  //   }
  // }

  /**
   * Screenshot widget with automatic JTAG coordination
   * Just like base JTAG screenshot command
   */
  protected async takeScreenshot(options: {
    filename?: string;
    selector?: string;
    includeContext?: boolean;
  } = {}): Promise<string | null> {
    try {
      if (!this.config.enableScreenshots) return null;
      
      const {
        filename = `${this.config.widgetName}-${Date.now()}.png`,
        selector = `:host`, // Screenshot this widget by default
        includeContext = true
      } = options;
      
      // Use JTAG screenshot command with proper types
      const result = await this.executeCommand<ScreenshotParams, ScreenshotResult>('screenshot', {
        filename,
        querySelector: selector,
        includeContext
      });
      
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: takeScreenshot failed:`, error);
      return null;
    }
  }


  /**
   * Save file with automatic file system coordination
   */
  protected async saveFile(filename: string, content: string | Blob, options: {
    directory?: string;
    format?: string;
    compress?: boolean;
  } = {}): Promise<string | null> {
    try {
      const {
        directory = WIDGET_DIRECTORIES.WIDGET_DATA
      } = options;
      
      // Use JTAG file/save command with proper types
      const result = await this.executeCommand<FileSaveParams, FileSaveResult>('file/save', {
        filepath: `${directory}/${filename}`,
        content: content,
        createDirs: true
      });
      
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: saveFile failed:`, error);
      return null;
    }
  }

  // === OVERRIDE POINTS - Subclasses can customize these ===

  /**
   * Override to customize AI context sent with queries
   */
  protected getAIContext(): WidgetData {
    return {
      widgetType: this.config.widgetName,
      currentState: Object.fromEntries(this.state.data),
      capabilities: this.context.capabilities
    };
  }

  /**
   * Override to customize error handling
   */
  protected handleError(error: Error | unknown, operation: string): void {
    console.error(`❌ ${this.config.widgetName}: ${operation} error:`, error);
    this.state.hasError = true;
    this.state.errorMessage = error instanceof Error ? error.message : String(error);
    
    // Default error UI - subclasses can override
    const errorMessage = error instanceof Error ? error.message : String(error);
    this.showErrorMessage(`${operation} failed: ${errorMessage}`);
  }

  /**
   * Override to customize performance monitoring
   */
  protected logPerformance(operation: string, startTime: number): void {
    if (!this.config.performanceMonitoring) return;
    
    const duration = performance.now() - startTime;
    console.log(`⚡ ${this.config.widgetName}: ${operation} took ${duration.toFixed(2)}ms`);
  }

  /**
   * Override to customize event handling
   */
  protected onEventReceived(eventType: string, data: WidgetData): void {
    // Default: just emit to internal event system
    const handlers = this.eventEmitter.get(eventType) ?? [];
    handlers.forEach(handler => handler(data));
  }

  /**
   * Override to customize state serialization
   */
  protected serializeState(): SerializedState {
    return {
      data: Object.fromEntries(this.state.data),
      lastUpdate: this.state.lastUpdate,
      config: this.config
    };
  }

  /**
   * Override to customize state deserialization
   */
  protected deserializeState(serialized: SerializedState): void {
    if (serialized.data) {
      this.state.data = new Map(Object.entries(serialized.data));
    }
    if (serialized.config && typeof serialized.config === 'object' && serialized.config !== null) {
      this.config = { ...this.config, ...serialized.config as Partial<WidgetConfig> };
    }
  }

  /**
   * Load external resources (HTML template and CSS styles) via HTTP fetch
   * Uses HTTP server instead of file/load JTAG command to avoid session directory issues
   */
  private async loadResources(): Promise<void> {
    if (!this.config.template && !this.config.styles) {
      return; // No resources to load
    }

    try {
      // Load HTML template if specified
      if (this.config.template) {
        this.templateHTML = await this.loadResource(this.config.template, 'template', '<div>Widget template not found</div>');
      }

      // Load CSS styles if specified  
      if (this.config.styles) {
        this.templateCSS = await this.loadResource(this.config.styles, 'styles', '/* Widget styles not found */');
      }

    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Resource loading failed:`, error);
      // Provide fallback content
      this.templateHTML = this.templateHTML ?? '<div>Resource loading error</div>';
      this.templateCSS = this.templateCSS ?? '/* Fallback styles */';
    }
  }

  /**
   * Resolve resource path for JTAG file/load command
   */
  protected resolveResourcePath(filename: string): string {
    // Extract widget directory name from widget name (ChatWidget -> chat)
    const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
    // Return relative path from current working directory
    return `widgets/${widgetDir}/public/${filename}`;
  }

  /**
   * Load a single resource using JTAG file/load command
   */
  protected async loadResource(filename: string, resourceType: string, fallback: string): Promise<string> {
    const resourcePath = this.resolveResourcePath(filename);
    const emoji = resourceType === 'template' ? '📄' : '🎨';
    
    console.log(`${emoji} ${this.config.widgetName}: Loading ${resourceType} from ${resourcePath}`);
    
    try {
      const result = await this.executeCommand<FileLoadParams, FileLoadResult>('file/load', {
        filepath: resourcePath
      });
      
      // FileLoadResult IS the command result - no nested structure
      if (result && result.success && result.content) {
        console.log(`✅ ${this.config.widgetName}: ${resourceType} loaded successfully (${result.bytesRead} bytes)`);
        return result.content;
      } else {
        console.warn(`⚠️ ${this.config.widgetName}: ${resourceType} load failed: ${resourcePath}`);
        console.warn(`  Debug: result.success=${result?.success}, has content=${!!result?.content}`);
        return fallback;
      }
    } catch (loadError) {
      console.warn(`⚠️ ${this.config.widgetName}: ${resourceType} load error: ${resourcePath}`, loadError);
      return fallback;
    }
  }


  private async restorePersistedState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    const serializedState = await this.getData('widget_state');
    if (serializedState && typeof serializedState === 'object' && serializedState !== null) {
      this.deserializeState(serializedState as SerializedState);
    }
  }

  private async persistCurrentState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    const serialized = this.serializeState();
    await this.storeData('widget_state', serialized, { persistent: true });
  }

  // DEAD CODE DETECTED HERE:
  // private async initializeEventSystem(): Promise<void> {
  //   if (this.routerDaemon) {
  //     // Subscribe to widget events. currently commented out because the routerOperation method should go thoirhg the router daemon not our own method
  //     // await this.routerOperation('subscribe', {
  //     //   channel: 'widget_events',
  //     //   callback: (eventData: WidgetData) => {
  //     //     if (eventData && typeof eventData === 'object' && eventData !== null) {
  //     //       const typedEventData = eventData as EventData;
  //     //       if (typedEventData.sourceWidget !== this.config.widgetId) {
  //     //         this.onEventReceived(typedEventData.eventType, typedEventData.data);
  //     //       }
  //     //     }
  //     //   }
  //     // });
  //   }
  // }

  private disconnectFromDaemons(): void {
    // Clean disconnection from all daemon systems
    this.databaseDaemon = undefined;
    this.routerDaemon = undefined;
    this.academyDaemon = undefined;
  }

  private async handleInitializationError(error: Error | unknown): Promise<void> {
    this.state.hasError = true;
    this.state.errorMessage = error instanceof Error ? error.message : String(error);
    
    // Render error state
    this.shadowRoot.innerHTML = `
      <div class="widget-error">
        <div class="error-title">❌ Widget Error</div>
        <div class="error-message">${this.state.errorMessage}</div>
        <button onclick="location.reload()">Reload</button>
      </div>
    `;
  }

  private showErrorMessage(message: string): void {
    // Simple error display - subclasses should override for better UX
    const errorEl = document.createElement('div');
    errorEl.className = 'widget-error-toast';
    errorEl.textContent = message;
    this.shadowRoot.appendChild(errorEl);
    
    setTimeout(() => errorEl.remove(), 5000);
  }

  // DEAD CODE DETECTED HERE:
  // private applyCSSProperties(styles: Record<string, string>): void {
  //   const styleElement = this.shadowRoot.querySelector('style') ?? document.createElement('style');
    
  //   let css = ':host {\n';
  //   for (const [property, value] of Object.entries(styles)) {
  //     css += `  --${property}: ${value};\n`;
  //   }
  //   css += '}\n';
    
  //   styleElement.textContent = (styleElement.textContent ?? '') + css;
    
  //   if (!styleElement.parentNode) {
  //     this.shadowRoot.appendChild(styleElement);
  //   }
  // }

  private isCacheValid(cached: CachedValue): boolean {
    const ttl = cached.ttl ?? 3600000; // 1 hour default
    return (Date.now() - cached.timestamp) < ttl;
  }

  protected async executeCommand<P extends CommandParams = CommandParams, R extends CommandResult = CommandResult>(command: string, params?: Record<string, WidgetData>): Promise<R> {
    try {
      // Wait for JTAG system to be ready  
      await this.waitForSystemReady();
      
      // Get the JTAG client via elegant daemon interface
      const jtag = JTAGClient.sharedInstance;
      if (!jtag?.daemons?.commands) {
        throw new Error('JTAG client daemons not available - system not ready');
      }
      
      // Use elegant daemon interface with proper CommandParams
      const jtagContext = jtag.context;
      const sessionId = jtag.sessionId;
      const commandParams = {
        context: jtagContext,
        sessionId: sessionId,
        ...params
      } as P;
      
      return await jtag.daemons.commands.execute<P, R>(command, commandParams);
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: JTAG operation ${command} failed:`, error);
      throw error;
    }
  }

  /**
   * Execute event with Rust-like strict typing - same elegance as executeCommand
   * Type-safe event emission and listening with explicit contracts
   */
  protected async executeEvent<T extends ChatEventName>(
    eventName: T, 
    eventData: ChatEventDataFor<T>,
    _options: {
      broadcast?: boolean;
      targetWidgets?: string[];
      excludeSelf?: boolean;
    } = {}
  ): Promise<boolean> {
    try {

      // DEAD CODE DETECTED HERE:
      // const {
      //   broadcast = true,
      //   targetWidgets,
      //   excludeSelf = true
      // } = options;

      // Emit locally first - immediate feedback
      const handlers = this.eventEmitter.get(eventName) ?? [];
      handlers.forEach(handler => handler(eventData));

      // DEAD CODE DETECTED HERE:
      // // Broadcast to other widgets if enabled
      // if (broadcast) {
      //   return await this.broadcastEvent(eventName, eventData, {
      //     targetWidgets,
      //     excludeSelf,
      //     persistent: false
      //   });
      // }
      
      return true;

    } catch (error) {
      console.error(`❌ BaseWidget: executeEvent ${eventName} failed:`, error);
      throw error;
    }
  }

  /**
   * Register type-safe event listener - companion to executeEvent
   */
  protected addWidgetEventListener<T extends ChatEventName>(
    eventName: T,
    handler: (data: ChatEventDataFor<T>) => void
  ): void {
    if (!this.eventEmitter.has(eventName)) {
      this.eventEmitter.set(eventName, []);
    }
    this.eventEmitter.get(eventName)!.push(handler as (data: WidgetData) => void);
    
    // CRITICAL: Set up automatic event dispatcher to connect server events to widget handlers
    this.setupEventDispatcher(eventName);
  }

  /**
   * CRITICAL: Sets up event dispatcher to connect server-originated events to widget handlers
   * This is the missing link between server events and registered widget event handlers
   */
  private setupEventDispatcher<T extends ChatEventName>(eventName: T): void {
    // Only set up dispatcher once per event type
    if (this.dispatcherEventTypes?.has(eventName)) {
      return;
    }

    this.dispatcherEventTypes ??= new Set();
    this.dispatcherEventTypes.add(eventName);

    console.log(`🔗 BaseWidget: Setting up event dispatcher for ${eventName}`);

    // Listen for server-originated events via the JTAG event system
    // These events come from EventsDaemon when server emits events
    document.addEventListener(eventName, (event: Event) => {
      const customEvent = event as Event & { detail: ChatEventDataFor<T> };
      console.log(`🔥 EVENT-DISPATCHER: Received server event ${eventName}:`, customEvent.detail);
      
      // Dispatch to all registered widget handlers
      const handlers = this.eventEmitter.get(eventName);
      if (handlers && handlers.length > 0) {
        console.log(`🔗 EVENT-DISPATCHER: Dispatching ${eventName} to ${handlers.length} widget handlers`);
        handlers.forEach(handler => {
          try {
            handler(customEvent.detail);
          } catch (error) {
            console.error(`❌ EVENT-DISPATCHER: Handler error for ${eventName}:`, error);
          }
        });
      } else {
        console.warn(`⚠️ EVENT-DISPATCHER: No widget handlers found for ${eventName}`);
      }
    });
    
    console.log(`✅ BaseWidget: Event dispatcher ready for ${eventName}`);
  }

  /**
   * Wait for window.jtag to be available (simple polling without timeout complexity)
   */
  private async waitForSystemReady(): Promise<void> {
    return new Promise((resolve) => {
      // Check if system is already ready
      const jtagClient = (window as WindowWithJTAG).jtag;
      if (jtagClient?.commands) {
        console.log(`✅ BaseWidget: JTAG system already ready for ${this.config.widgetName}`);
        resolve();
        return;
      }
      
      console.log(`⏳ BaseWidget: Waiting for JTAG system to be ready for ${this.config.widgetName}`);
      
      // Simple polling - check every 100ms for window.jtag
      const checkReady = (): void => {
        const jtag = (window as WindowWithJTAG).jtag;
        if (jtag?.commands) {
          console.log(`✅ BaseWidget: JTAG system ready for ${this.config.widgetName}`);
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      };
      
      checkReady();
    });
  }

  private generateWidgetId(): string {
    return `widget_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private generateSessionId(): string {
    return `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}