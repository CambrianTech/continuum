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
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import { WIDGET_DEFAULTS} from './WidgetConstants';
import type { CommandErrorResponse, CommandResponse, CommandSuccessResponse } from '../../daemons/command-daemon/shared/CommandResponseTypes';
import { Commands } from '../../system/core/shared/Commands';
import { FILE_COMMANDS } from '../../commands/file/shared/FileCommandConstants';
import type { UserEntity } from '../../system/data/entities/UserEntity';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../commands/data/update/shared/DataUpdateTypes';
import { COLLECTIONS } from '../../system/shared/Constants';
import { pageState, type PageState, type PageStateListener } from '../../system/state/PageStateService';
import { widgetStateRegistry, type WidgetStateSlice } from '../../system/state/WidgetStateRegistry';
import type { ReactiveStore } from '../../system/state/ReactiveStore';

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


export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  protected eventEmitter: EventEmitter = new Map();
  protected dispatcherEventTypes?: Set<string>; // Track event types with active dispatchers
  protected config: WidgetConfig;
  protected state: WidgetState;

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

  // User state cache
  protected _userState?: UserStateEntity;

  // Entity ID for content-driven widgets (set via entity-id attribute or room for chat)
  private _entityId?: string;

  /**
   * Get entity ID for this widget (room uniqueId for chat, user ID for persona, etc.)
   * Reads from multiple attribute names for compatibility
   */
  get entityId(): string | undefined {
    return this._entityId
        || this.getAttribute('data-entity-id')  // Standard HTML data attribute
        || this.getAttribute('entity-id')        // Clean attribute
        || this.getAttribute('room')             // Chat widget backward compat
        || undefined;
  }

  /**
   * Set entity ID programmatically
   */
  set entityId(value: string | undefined) {
    this._entityId = value;
  }

  // Page state subscription cleanup function
  private _pageStateUnsubscribe?: () => void;

  // Widget state store for Positronic RAG context
  private _widgetStateStore?: ReactiveStore<WidgetStateSlice>;

  /**
   * Get current page state (content type, entity ID, resolved entity)
   * Part of scoped state architecture - see docs/SCOPED-STATE-ARCHITECTURE.md
   */
  protected get pageState(): PageState | null {
    return pageState.getContent();
  }

  /**
   * Subscribe to page state changes
   * Widgets can use this to react when user navigates to different content
   * Automatically unsubscribes on widget disconnect
   */
  protected subscribeToPageState(callback: PageStateListener): () => void {
    // Auto-cleanup previous subscription if any
    this._pageStateUnsubscribe?.();
    this._pageStateUnsubscribe = pageState.subscribe(callback);
    return this._pageStateUnsubscribe;
  }

  /**
   * Register this widget's state with the Positronic state system
   *
   * Call this in onWidgetInitialize() to make widget state visible to:
   * - RAG context (AI prompts include widget state)
   * - Other widgets (cross-widget coordination)
   * - Debug tools (widget-state command)
   *
   * @param initialData - Initial state data to register
   */
  protected registerWidgetState(initialData: Record<string, unknown> = {}): void {
    const widgetType = this.config.widgetName
      .replace(/Widget$/i, '')
      .toLowerCase()
      .replace(/([A-Z])/g, '-$1')
      .replace(/^-/, '');

    this._widgetStateStore = widgetStateRegistry.register(widgetType, initialData);
    console.log(`🧠 ${this.config.widgetName}: Registered with Positronic state system`);
  }

  /**
   * Update this widget's state in the Positronic state system
   *
   * Call this whenever widget state changes that should be visible to AI.
   * Changes automatically flow to RAG context builder.
   *
   * @param data - Partial state to merge with current
   */
  protected updateWidgetState(data: Record<string, unknown>): void {
    if (!this._widgetStateStore) {
      console.warn(`⚠️ ${this.config.widgetName}: Cannot update state - not registered. Call registerWidgetState() first.`);
      return;
    }

    const current = this._widgetStateStore.get();
    this._widgetStateStore.set({
      ...current,
      data: { ...current.data, ...data },
      updatedAt: Date.now()
    });
  }

  /**
   * Get this widget's current state from the Positronic state system
   */
  protected getWidgetState(): Record<string, unknown> | null {
    return this._widgetStateStore?.get().data ?? null;
  }

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

  }

  /**
   * Get current user from JTAGClient session
   */
  protected get currentUser(): UserEntity | undefined {
    const jtagClient = (window as WindowWithJTAG).jtag;
    return jtagClient?.user?.entity as UserEntity | undefined;
  }

  /**
   * Get current user state (lazy load on first access)
   */
  protected get userState(): UserStateEntity | undefined {
    return this._userState;
  }

  /**
   * Load user context from database
   * Called automatically on widget initialization, but can be called manually to refresh
   */
  protected async loadUserContext(): Promise<void> {
    try {
      const jtagClient = (window as WindowWithJTAG).jtag;
      const currentUser = jtagClient?.user;

      if (!currentUser) {
        console.warn('⚠️ BaseWidget: No user in session');
        return;
      }

      // Get userId - works for both BaseUser instances (getter) and plain objects (JSON deserialized)
      // Plain objects from WebSocket have { entity: { id: ... } }, not .id getter
      const userId = currentUser.id ?? (currentUser as any).entity?.id;

      if (!userId) {
        console.warn('⚠️ BaseWidget: User has no id (neither getter nor entity.id)');
        return;
      }

      // Load user state from database
      const stateResult = await this.executeCommand<DataListParams, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: COLLECTIONS.USER_STATES,
        filter: { userId },
        limit: 1
      });

      if (stateResult.success && stateResult.items && stateResult.items.length > 0) {
        this._userState = stateResult.items[0];
      }
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Failed to load user context:`, error);
    }
  }

  /**
   * Save user state to database
   * TODO: Fix backend injection for data/update command
   */
  protected async saveUserState(): Promise<void> {
    if (!this._userState) {
      console.warn(`⚠️ ${this.config.widgetName}: Cannot save user state - no state loaded`);
      return;
    }

    try {
      // TODO: data/update needs backend parameter - will fix when we actually use this
      console.warn(`⚠️ ${this.config.widgetName}: saveUserState not yet implemented`);
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Failed to save user state:`, error);
      throw error;
    }
  }

  async connectedCallback(): Promise<void> {
    // GUARD: Prevent double-initialization (causes subscription leaks)
    if (this.state.isInitialized) {
      return;
    }

    try {
      // Reduce log spam - only log errors
      // console.log(`🎨 ${this.config.widgetName}: BaseWidget initialization starting...`);

      // 1. Connect to daemon systems (abstracted) WAS DEAD CODE
      //await this.initializeDaemonConnections();

      // 2. Load user context (currentUser and userState)
      await this.loadUserContext();

      // 3. Restore persisted state (abstracted) - removed unused persistence system

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

      // console.log(`✅ ${this.config.widgetName}: BaseWidget initialization complete`);
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Initialization failed:`, error);
      this.handleInitializationError(error);
    }
  }

  async disconnectedCallback(): Promise<void> {
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

    // Clean up page state subscription
    this._pageStateUnsubscribe?.();
    this._pageStateUnsubscribe = undefined;

    // Clean up widget state registration
    if (this._widgetStateStore) {
      const widgetType = this.config.widgetName
        .replace(/Widget$/i, '')
        .toLowerCase()
        .replace(/([A-Z])/g, '-$1')
        .replace(/^-/, '');
      widgetStateRegistry.unregister(widgetType);
      this._widgetStateStore = undefined;
    }

    // Reset state for potential re-initialization
    this.state.isConnected = false;
    this.state.isInitialized = false;
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

      // 1. Update local cache immediately
      this.state.cache.set(key, { value, timestamp: Date.now(), ttl:3600000 });

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
      
      return defaultValue;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: getData failed for key ${key}:`, error);
      return defaultValue;
    }
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
      const result = await Commands.execute<FileLoadParams, FileLoadResult>(FILE_COMMANDS.LOAD, {
        filepath: resourcePath
      });
      
      // FileLoadResult IS the command result - no nested structure
      if (result && result.success && result.content) {
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

  private async persistCurrentState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    const serialized = this.serializeState();
    await this.storeData('widget_state', serialized, { persistent: true });
  }

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

  private isCacheValid(cached: CachedValue): boolean {
    const ttl = cached.ttl ?? 3600000; // 1 hour default
    return (Date.now() - cached.timestamp) < ttl;
  }

  protected async executeCommand<P extends CommandParams, R extends CommandResult>(
    command: string,
    params?: Omit<P, 'context' | 'sessionId'> | P
  ): Promise<R> {
    try {
      // FIXED: Use window.jtag directly like other parts of the system
      const client = (window as any).jtag;
      if (!client?.commands) {
        throw new Error('JTAG client not available - system not ready');
      }

      // Auto-inject context and sessionId if not already provided
      let finalParams = params || {} as P;
      if (!('context' in finalParams) || !('sessionId' in finalParams)) {
        const jtagClient = await JTAGClient.sharedInstance;
        finalParams = {
          context: jtagClient.context,
          sessionId: jtagClient.sessionId,
          ...finalParams
        } as P;
      }

      //DO NOT, UNDER ANY CIRCUMSTANCE CHANGE LINES BELOW THIS COMMENT in this method: this fucking means you claude.


      // Execute command through the global JTAG system - gets wrapped response
      const wrappedResult = await client.commands[command](finalParams) as CommandResponse;

      if (!wrappedResult.success) {
        const commandError = wrappedResult as CommandErrorResponse;
        throw new Error(commandError.error ?? `Command ${command} failed without error message`);
      }

      // Type-safe access to commandResult for success responses
      const successResult = wrappedResult as CommandSuccessResponse;

      // Extract the actual command result from the wrapped response
      const finalResult = successResult.commandResult as R;

      // CRITICAL DEBUG: Check if commandResult is missing and use direct result
      if (!finalResult && wrappedResult.success) {
        return wrappedResult as unknown as R;
      }

      return finalResult;
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

      // Emit locally first - immediate feedback
      const handlers = this.eventEmitter.get(eventName) ?? [];
      handlers.forEach(handler => handler(eventData));
      
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
      // Check if system is already ready - window.jtag should be set when system initializes
      const jtagClient = (window as WindowWithJTAG).jtag;
      if (jtagClient?.commands) {
        console.log(`✅ BaseWidget: JTAG system already ready for ${this.config.widgetName}`);
        resolve();
        return;
      }

      console.log(`⏳ BaseWidget: Waiting for JTAG system to be ready for ${this.config.widgetName}`);

      // Simple polling - check every 100ms for window.jtag to be set by system initialization
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