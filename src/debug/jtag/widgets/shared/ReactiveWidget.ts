/**
 * ReactiveWidget - Efficient reactive base class using Lit
 *
 * Like React, but for web components:
 * - Reactive properties trigger efficient re-renders
 * - Template diffing (only updates what changed)
 * - Declarative rendering with tagged template literals
 * - Automatic event cleanup
 * - TypeScript-first with decorators
 *
 * Migration from BaseWidget:
 * - Replace `innerHTML = ...` with `render()` method
 * - Replace manual state with `@state()` decorator
 * - Replace manual event listeners with `@click` etc in templates
 *
 * @example
 * ```typescript
 * class MyWidget extends ReactiveWidget {
 *   @state() count = 0;
 *
 *   render() {
 *     return html`
 *       <button @click=${() => this.count++}>
 *         Clicked ${this.count} times
 *       </button>
 *     `;
 *   }
 * }
 * ```
 */

import { LitElement, html, css, unsafeCSS, type TemplateResult, type CSSResultGroup, type PropertyDeclaration } from 'lit';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../system/data/entities/UserEntity';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import { PositronWidgetState, type InteractionHint } from './services/state/PositronWidgetState';
import { widgetStateRegistry, type WidgetStateSlice } from '../../system/state/WidgetStateRegistry';
import type { ReactiveStore } from '../../system/state/ReactiveStore';
import { COLLECTIONS } from '../../system/shared/Constants';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import { entityCache, type CacheChange } from '../../system/state/EntityCacheService';

/**
 * Cleanup function returned by effects
 */
export type EffectCleanup = () => void;

/**
 * Effect handler function - can return cleanup
 */
export type EffectHandler<T> = (value: T, prevValue: T | undefined) => void | EffectCleanup;

/**
 * Selector function to extract dependencies from widget
 */
export type DependencySelector<W, T> = (widget: W) => T;

/**
 * Configuration for useCollection - React-like data fetching hook
 */
export interface UseCollectionConfig<T extends BaseEntity> {
  /** Collection name (e.g., 'chat_messages', 'users') */
  collection: string;
  /** Filter function to select entities (runs on cached data) */
  filter?: (entity: T) => boolean;
  /** Sort function for ordering results */
  sort?: (a: T, b: T) => number;
  /** Maximum entities to return */
  limit?: number;
  /** Called when data changes - receives filtered/sorted entities */
  onData: (entities: T[]) => void;
  /** Called when loading state changes */
  onLoading?: (loading: boolean) => void;
  /** Called on error */
  onError?: (error: Error) => void;
  /** Database query filter (for initial load) */
  dbFilter?: Record<string, unknown>;
  /** Database query orderBy (for initial load) */
  dbOrderBy?: Array<{ field: string; direction: 'asc' | 'desc' }>;
  /** Skip initial database load (use cache only) */
  cacheOnly?: boolean;
}

/**
 * Return type for useCollection - control handle
 */
export interface UseCollectionHandle {
  /** Unsubscribe and clean up */
  unsubscribe: () => void;
  /** Force refresh from database */
  refresh: () => Promise<void>;
  /** Update filter dynamically */
  setFilter: (filter: ((entity: BaseEntity) => boolean) | undefined) => void;
  /** Get current cached count */
  count: () => number;
}

// Re-export Lit utilities for subclasses
export { html, css, unsafeCSS, type TemplateResult, type CSSResultGroup };
export type { InteractionHint };

/**
 * Property decorator that works with TC39 standard decorators.
 * Use: @reactive() myProp = initialValue;
 *
 * TC39 class fields shadow Lit's prototype accessor (from createProperty),
 * so we schedule a microtask to remove the own property after field
 * initialization completes. This makes Lit's reactive setter visible,
 * and subsequent assignments trigger requestUpdate() automatically.
 */
export function reactive(options?: PropertyDeclaration) {
  return function(target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      // Register as reactive property on the class (creates prototype accessor)
      const ctor = (this as { constructor: typeof ReactiveWidget }).constructor;
      ctor.createProperty(fieldName, {
        ...options,
        state: true // Internal state, not reflected to attribute
      });

      // Fix TC39 class field shadowing: after field initialization completes,
      // the own property shadows Lit's prototype accessor. Remove it so
      // Lit's reactive setter becomes visible and triggers re-renders.
      const instance = this as Record<string, unknown>;
      queueMicrotask(() => {
        if (Object.prototype.hasOwnProperty.call(instance, fieldName)) {
          const value = instance[fieldName];
          delete instance[fieldName];
          instance[fieldName] = value; // Now goes through Lit's accessor
        }
      });
    });
  };
}

/**
 * Attribute property decorator - reflects to/from HTML attribute
 * Use: @attr() label = 'default';
 *
 * Same TC39 field shadowing fix as @reactive().
 */
export function attr(options?: PropertyDeclaration) {
  return function(target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      const ctor = (this as { constructor: typeof ReactiveWidget }).constructor;
      ctor.createProperty(fieldName, {
        ...options,
        reflect: true
      });

      const instance = this as Record<string, unknown>;
      queueMicrotask(() => {
        if (Object.prototype.hasOwnProperty.call(instance, fieldName)) {
          const value = instance[fieldName];
          delete instance[fieldName];
          instance[fieldName] = value;
        }
      });
    });
  };
}

interface WindowWithJTAG extends Window {
  jtag?: JTAGClient;
}

interface CachedValue<T> {
  value: T;
  timestamp: number;
  ttl: number;
}

/**
 * Configuration for reactive widgets
 */
export interface ReactiveWidgetConfig {
  /** Widget display name */
  widgetName: string;
  /** Enable command execution */
  enableCommands?: boolean;
  /** Enable Positron context emission */
  enablePositron?: boolean;
  /** Cache TTL in milliseconds */
  cacheTTL?: number;
  /** Debug logging */
  debug?: boolean;
}

/**
 * Base class for efficient reactive widgets
 *
 * Extends LitElement for:
 * - Reactive property system (state changes → efficient re-render)
 * - Template diffing (only updates changed DOM)
 * - Declarative event binding (automatic cleanup)
 * - Scoped styles via Shadow DOM
 */
export abstract class ReactiveWidget extends LitElement {
  /**
   * Declare reactive properties using static properties
   * This is the non-decorator way that works with TC39 standard decorators
   */
  static properties = {
    loading: { type: Boolean, state: true },
    error: { type: String, state: true }
  };

  /**
   * Widget configuration
   */
  protected config: ReactiveWidgetConfig;

  /**
   * Loading state - use for async operations
   */
  protected loading = false;

  /**
   * Error state - displays error UI when set
   */
  protected error: string | null = null;

  /**
   * Command result cache
   */
  private commandCache = new Map<string, CachedValue<unknown>>();

  /**
   * User state cache - loaded via loadUserContext()
   */
  protected _userState?: UserStateEntity;

  /**
   * Widget state store for Positronic state system
   * Enables automatic RAG context injection
   */
  private _widgetStateStore?: ReactiveStore<WidgetStateSlice>;

  /**
   * Active effects with their selectors, handlers, and cleanup functions
   */
  private _effects: Array<{
    selector: (widget: ReactiveWidget) => unknown;
    handler: EffectHandler<unknown>;
    prevValue: unknown;
    cleanup?: EffectCleanup;
  }> = [];

  constructor(config: Partial<ReactiveWidgetConfig> = {}) {
    super();
    this.config = {
      widgetName: this.constructor.name,
      enableCommands: true,
      enablePositron: true,
      cacheTTL: 30000, // 30 seconds default
      debug: false,
      ...config
    };
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // LIFECYCLE HOOKS - Override these in subclasses
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Called when widget is added to DOM
   * Override for initialization logic
   */
  connectedCallback(): void {
    super.connectedCallback();
    this.log('Connected to DOM');

    // Auto-load user context (non-blocking)
    this.loadUserContext().catch(err => {
      // Silent fail - user context is optional for some widgets
      this.log('User context load failed (may be expected):', err);
    });

    this.onConnect();
  }

  /**
   * Called when widget is removed from DOM
   * Override for cleanup logic
   */
  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.log('Disconnected from DOM');

    // Clean up all effects
    this.disposeEffects();

    // Clean up collection subscriptions
    this.disposeCollectionHandles();

    // Clean up widget state registration
    this.unregisterWidgetState();

    this.onDisconnect();
  }

  /**
   * Clean up all collection handles and entity subscriptions
   */
  private disposeCollectionHandles(): void {
    for (const handle of this._collectionHandles) {
      try {
        handle.unsubscribe();
      } catch (e) {
        console.error(`${this.config.widgetName}: Collection handle cleanup error:`, e);
      }
    }
    this._collectionHandles = [];

    for (const unsubscribe of this._entityUnsubscribers) {
      try {
        unsubscribe();
      } catch (e) {
        console.error(`${this.config.widgetName}: Entity unsubscribe error:`, e);
      }
    }
    this._entityUnsubscribers = [];
  }

  /**
   * Clean up all effects
   */
  private disposeEffects(): void {
    for (const effect of this._effects) {
      if (effect.cleanup) {
        try {
          effect.cleanup();
        } catch (e) {
          console.error(`${this.config.widgetName}: Effect cleanup error:`, e);
        }
      }
    }
    this._effects = [];
  }

  /**
   * Called after first render
   * Override for post-render initialization
   */
  protected firstUpdated(): void {
    this.log('First render complete');
    this.onFirstRender();
  }

  /**
   * Called after every render
   * Override for post-render logic
   */
  protected updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);
    if (this.config.debug) {
      this.log(`Updated: ${[...changedProperties.keys()].join(', ')}`);
    }

    // Run effects when any property changes
    if (changedProperties.size > 0) {
      this.runEffects();
    }
  }

  /**
   * Run all effects, checking for dependency changes
   */
  private runEffects(): void {
    for (const effect of this._effects) {
      try {
        const currentValue = effect.selector(this);

        // Check if dependencies changed
        if (!this.shallowEqual(currentValue, effect.prevValue)) {
          // Run cleanup from previous execution
          if (effect.cleanup) {
            effect.cleanup();
            effect.cleanup = undefined;
          }

          // Run the effect handler
          const cleanup = effect.handler(currentValue, effect.prevValue);
          if (typeof cleanup === 'function') {
            effect.cleanup = cleanup;
          }

          // Store current value for next comparison
          effect.prevValue = currentValue;

          if (this.config.debug) {
            this.log(`Effect triggered:`, { prev: effect.prevValue, current: currentValue });
          }
        }
      } catch (e) {
        console.error(`${this.config.widgetName}: Effect error:`, e);
      }
    }
  }

  /**
   * Shallow equality check for effect dependency comparison
   */
  private shallowEqual<T>(a: T, b: T): boolean {
    if (a === b) return true;
    if (a === null || b === null) return false;
    if (typeof a !== 'object' || typeof b !== 'object') return false;

    if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) return false;
      return a.every((v, i) => v === b[i]);
    }

    const keysA = Object.keys(a) as (keyof T)[];
    const keysB = Object.keys(b) as (keyof T)[];
    if (keysA.length !== keysB.length) return false;

    return keysA.every(key => a[key] === b[key]);
  }

  // Hooks for subclasses (cleaner than overriding lifecycle methods)
  protected onConnect(): void {}
  protected onDisconnect(): void {}
  protected onFirstRender(): void {}

  // ═══════════════════════════════════════════════════════════════════════════
  // EFFECTS - React-like side effect management
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Create an effect that runs when dependencies change (like React useEffect)
   *
   * Call in onFirstRender() or onConnect() to set up side effects.
   * Effects run after every render where their dependencies changed.
   *
   * @param selector - Function that returns dependencies (runs on every render)
   * @param handler - Function that runs when dependencies change
   * @returns Dispose function to remove the effect
   *
   * @example
   * ```typescript
   * class MyChatWidget extends ReactiveWidget {
   *   @reactive() roomId: string | null = null;
   *
   *   protected onFirstRender() {
   *     // Effect: load messages when roomId changes
   *     this.createEffect(
   *       (w) => w.roomId,
   *       (roomId) => {
   *         if (roomId) this.loadMessages(roomId);
   *       }
   *     );
   *   }
   *
   *   switchRoom(id: string) {
   *     this.roomId = id;  // Effect runs automatically
   *   }
   * }
   * ```
   */
  protected createEffect<T>(
    selector: DependencySelector<this, T>,
    handler: EffectHandler<T>
  ): EffectCleanup {
    // Get initial value
    const initialValue = selector(this);

    // Create effect entry - cast selector to avoid 'this' type issues
    const effect = {
      selector: selector as unknown as (widget: ReactiveWidget) => unknown,
      handler: handler as EffectHandler<unknown>,
      prevValue: undefined as unknown, // Will trigger on first run
      cleanup: undefined as EffectCleanup | undefined
    };

    this._effects.push(effect);

    // Run effect immediately with initial value
    try {
      const cleanup = handler(initialValue, undefined);
      if (typeof cleanup === 'function') {
        effect.cleanup = cleanup;
      }
      effect.prevValue = initialValue;
    } catch (e) {
      console.error(`${this.config.widgetName}: Initial effect error:`, e);
    }

    // Return dispose function
    return () => {
      const index = this._effects.indexOf(effect);
      if (index >= 0) {
        if (effect.cleanup) {
          effect.cleanup();
        }
        this._effects.splice(index, 1);
      }
    };
  }

  /**
   * Create an effect that only runs once (like React useEffect with empty deps)
   *
   * @param handler - Function that runs once on first render
   * @returns Dispose function
   */
  protected createMountEffect(handler: () => void | EffectCleanup): EffectCleanup {
    let hasRun = false;
    let cleanup: EffectCleanup | undefined;

    return this.createEffect(
      () => null, // Constant dependency
      () => {
        if (!hasRun) {
          hasRun = true;
          cleanup = handler() ?? undefined;
        }
        return cleanup;
      }
    );
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // DATA SUBSCRIPTION - React-like data fetching with EntityCacheService
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Subscribe to a collection with automatic caching, event handling, and DB loading
   *
   * This is the primary way widgets should access entity data. It:
   * 1. Subscribes to EntityCacheService for real-time updates
   * 2. Loads initial data from DB if cache is empty
   * 3. Filters and sorts data before delivering to callback
   * 4. Handles cleanup automatically on widget disconnect
   *
   * @example
   * ```typescript
   * class MyChatWidget extends ReactiveWidget {
   *   private messagesHandle?: UseCollectionHandle;
   *
   *   protected onFirstRender() {
   *     this.messagesHandle = this.useCollection<ChatMessageEntity>({
   *       collection: 'chat_messages',
   *       filter: m => m.roomId === this.currentRoomId,
   *       sort: (a, b) => a.timestamp.localeCompare(b.timestamp),
   *       limit: 50,
   *       dbFilter: { roomId: this.currentRoomId },
   *       dbOrderBy: [{ field: 'timestamp', direction: 'desc' }],
   *       onData: (messages) => {
   *         this.messages = messages;
   *         this.requestUpdate();
   *       },
   *       onLoading: (loading) => this.loading = loading
   *     });
   *   }
   *
   *   switchRoom(newRoomId: string) {
   *     // Update filter - triggers refresh automatically
   *     this.messagesHandle?.setFilter(m => m.roomId === newRoomId);
   *     this.messagesHandle?.refresh();
   *   }
   * }
   * ```
   */
  protected useCollection<T extends BaseEntity>(
    config: UseCollectionConfig<T>
  ): UseCollectionHandle {
    const {
      collection,
      onData,
      onLoading,
      onError,
      dbFilter,
      dbOrderBy,
      cacheOnly = false,
      limit
    } = config;

    // Mutable filter/sort (can be updated via handle)
    let currentFilter = config.filter;
    let currentSort = config.sort;

    // Process and deliver entities to callback
    const deliverData = (entities: T[]) => {
      let result = entities;

      // Apply filter
      if (currentFilter) {
        result = result.filter(currentFilter);
      }

      // Apply sort
      if (currentSort) {
        result = [...result].sort(currentSort);
      }

      // Apply limit
      if (limit && result.length > limit) {
        result = result.slice(0, limit);
      }

      onData(result);
    };

    // Subscribe to cache changes
    const unsubscribe = entityCache.subscribe<T>(
      collection,
      (entities: T[], _change: CacheChange<T>) => {
        deliverData(entities);
      }
    );

    // Load from database if cache is empty or explicit refresh needed
    const loadFromDB = async () => {
      if (cacheOnly) return;

      const cachedCount = entityCache.count(collection);

      // Only load if cache appears empty for this collection
      // (subscribe already delivered cached data if present)
      if (cachedCount === 0 || dbFilter) {
        onLoading?.(true);
        try {
          const result = await this.executeCommand<DataListParams, DataListResult<T>>(
            DATA_COMMANDS.LIST,
            {
              collection,
              filter: dbFilter,
              orderBy: dbOrderBy,
              limit: limit ? limit * 2 : 100 // Load extra for filtering headroom
            }
          );

          if (result.success && result.items) {
            // Populate cache - this triggers subscriber with fresh data
            // Spread to convert readonly array to mutable
            entityCache.populate(collection, [...result.items]);
          }
        } catch (err) {
          onError?.(err instanceof Error ? err : new Error(String(err)));
        } finally {
          onLoading?.(false);
        }
      }
    };

    // Initial load
    loadFromDB();

    // Return control handle
    const handle: UseCollectionHandle = {
      unsubscribe: () => {
        unsubscribe();
      },

      refresh: async () => {
        // Clear collection cache to force reload
        entityCache.clear(collection);
        await loadFromDB();
      },

      setFilter: (newFilter) => {
        currentFilter = newFilter as ((entity: T) => boolean) | undefined;
        // Re-deliver with new filter
        const entities = entityCache.getAll<T>(collection);
        deliverData(entities);
      },

      count: () => entityCache.count(collection)
    };

    // Track for cleanup on disconnect
    this._collectionHandles.push(handle);

    return handle;
  }

  /**
   * Subscribe to a single entity with automatic caching
   *
   * @example
   * ```typescript
   * const unsubscribe = this.useEntity<UserEntity>('users', userId, (user) => {
   *   this.user = user;
   *   this.requestUpdate();
   * });
   * ```
   */
  protected useEntity<T extends BaseEntity>(
    collection: string,
    entityId: string,
    onData: (entity: T | null) => void
  ): () => void {
    const unsubscribe = entityCache.subscribeToEntity<T>(collection, entityId, onData);

    // Track for cleanup
    this._entityUnsubscribers.push(unsubscribe);

    return unsubscribe;
  }

  // Collection handles for cleanup
  private _collectionHandles: UseCollectionHandle[] = [];
  private _entityUnsubscribers: (() => void)[] = [];

  // ═══════════════════════════════════════════════════════════════════════════
  // RENDERING - The React-like pattern
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Override to define widget styles
   * Uses CSS-in-JS with automatic scoping
   */
  static styles: CSSResultGroup = css`
    :host {
      display: block;
      width: 100%;
      height: 100%;
    }

    .loading {
      display: flex;
      align-items: center;
      justify-content: center;
      height: 100%;
      color: var(--color-text-muted, #888);
    }

    .error {
      padding: 16px;
      background: rgba(255, 80, 80, 0.1);
      border: 1px solid var(--color-error, #ff5050);
      border-radius: 4px;
      color: var(--color-error, #ff5050);
    }

    .spinner {
      width: 24px;
      height: 24px;
      border: 2px solid var(--color-primary-dark, #0088aa);
      border-top-color: var(--color-primary, #00d4ff);
      border-radius: 50%;
      animation: spin 1s linear infinite;
    }

    @keyframes spin {
      to { transform: rotate(360deg); }
    }
  `;

  /**
   * Main render method - override in subclasses
   * Return html`` tagged template for efficient diffing
   */
  protected render(): TemplateResult {
    // Handle loading state
    if (this.loading) {
      return this.renderLoading();
    }

    // Handle error state
    if (this.error) {
      return this.renderError();
    }

    // Subclasses implement this
    return this.renderContent();
  }

  /**
   * Override to render widget content
   */
  protected renderContent(): TemplateResult {
    return html`<slot></slot>`;
  }

  /**
   * Override to customize loading UI
   */
  protected renderLoading(): TemplateResult {
    return html`
      <div class="loading">
        <div class="spinner"></div>
      </div>
    `;
  }

  /**
   * Override to customize error UI
   */
  protected renderError(): TemplateResult {
    return html`
      <div class="error">
        <strong>Error:</strong> ${this.error}
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // COMMAND EXECUTION - Typed command interface
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Execute a JTAG command with automatic context injection
   *
   * @example
   * const users = await this.executeCommand<DataListParams, DataListResult>('data/list', {
   *   collection: 'users'
   * });
   */
  protected async executeCommand<P extends CommandParams, R extends CommandResult>(
    command: string,
    params?: Omit<P, 'context' | 'sessionId' | 'userId'>
  ): Promise<R> {
    if (!this.config.enableCommands) {
      throw new Error('Commands not enabled for this widget');
    }

    const client = (window as unknown as WindowWithJTAG).jtag;
    if (!client?.commands) {
      throw new Error('JTAG client not available');
    }

    // Auto-inject context
    const jtagClient = await JTAGClient.sharedInstance;
    const fullParams = {
      context: jtagClient.context,
      sessionId: jtagClient.sessionId,
      ...params
    } as P;

    // Execute and extract result
    const response = await client.commands[command](fullParams);

    if ('error' in response && response.error) {
      throw new Error(response.error as string);
    }

    return ('result' in response ? response.result : response) as R;
  }

  /**
   * Execute command with caching
   */
  protected async cachedCommand<P extends CommandParams, R extends CommandResult>(
    command: string,
    params?: Omit<P, 'context' | 'sessionId' | 'userId'>,
    ttl?: number
  ): Promise<R> {
    const cacheKey = `${command}:${JSON.stringify(params)}`;
    const cached = this.commandCache.get(cacheKey);
    const effectiveTTL = ttl ?? this.config.cacheTTL ?? 30000;

    if (cached && (Date.now() - cached.timestamp) < cached.ttl) {
      return cached.value as R;
    }

    const result = await this.executeCommand<P, R>(command, params);

    this.commandCache.set(cacheKey, {
      value: result,
      timestamp: Date.now(),
      ttl: effectiveTTL
    });

    return result;
  }

  /**
   * Clear command cache
   */
  protected clearCache(pattern?: string): void {
    if (pattern) {
      for (const key of this.commandCache.keys()) {
        if (key.includes(pattern)) {
          this.commandCache.delete(key);
        }
      }
    } else {
      this.commandCache.clear();
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // POSITRON CONTEXT - AI awareness
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Emit Positron context for AI awareness
   */
  protected emitContext(
    widget: {
      widgetType: string;
      section?: string;
      title?: string;
      metadata?: Record<string, unknown>;
    },
    interaction?: InteractionHint
  ): void {
    if (!this.config.enablePositron) return;

    PositronWidgetState.emit(
      {
        widgetType: widget.widgetType,
        section: widget.section,
        title: widget.title || this.config.widgetName,
        metadata: widget.metadata
      },
      interaction
    );
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // POSITRONIC STATE SYSTEM - Widget state for RAG context
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Register this widget's state with the Positronic state system
   *
   * Call this in onFirstRender() or onConnect() to enable:
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
    this.log(`Registered with Positronic state system as "${widgetType}"`);
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

  /**
   * Unregister widget state (called automatically on disconnect)
   */
  private unregisterWidgetState(): void {
    if (this._widgetStateStore) {
      const widgetType = this.config.widgetName
        .replace(/Widget$/i, '')
        .toLowerCase()
        .replace(/([A-Z])/g, '-$1')
        .replace(/^-/, '');
      widgetStateRegistry.unregister(widgetType);
      this._widgetStateStore = undefined;
      this.log(`Unregistered from Positronic state system`);
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // USER CONTEXT
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Get current user from JTAG session
   */
  protected get currentUser(): UserEntity | undefined {
    const client = (window as unknown as WindowWithJTAG).jtag;
    return client?.user?.entity as UserEntity | undefined;
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

      console.log(`🔍 ${this.config.widgetName}.loadUserContext: hasJtagClient=${!!jtagClient}, hasUser=${!!currentUser}`);

      if (!currentUser) {
        console.warn(`⚠️ ${this.config.widgetName}: No user in session`);
        return;
      }

      // Get userId - works for both BaseUser instances (getter) and plain objects (JSON deserialized)
      const userId = currentUser.id ?? (currentUser as any).entity?.id;
      console.log(`🔍 ${this.config.widgetName}.loadUserContext: userId=${userId?.slice?.(0, 8) || 'null'}`);

      if (!userId) {
        console.warn(`⚠️ ${this.config.widgetName}: User has no id`);
        return;
      }

      // Load user state from database - ALWAYS from server to get fresh contentState
      // localStorage cache may have stale openItems from previous sessions
      const stateResult = await this.executeCommand<DataListParams, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: COLLECTIONS.USER_STATES,
        filter: { userId },
        limit: 1,
        backend: 'server'  // Bypass localStorage cache for fresh data
      });

      console.log(`🔍 ${this.config.widgetName}.loadUserContext: Query result - success=${stateResult.success}, items=${stateResult.items?.length || 0}, hasContentState=${!!stateResult.items?.[0]?.contentState}, openItems=${stateResult.items?.[0]?.contentState?.openItems?.length || 0}`);

      if (stateResult.success && stateResult.items && stateResult.items.length > 0) {
        this._userState = stateResult.items[0];
      }
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Failed to load user context:`, error);
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // UTILITIES
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Request a re-render
   * Use sparingly - prefer reactive properties
   */
  protected refresh(): void {
    this.requestUpdate();
  }

  /**
   * Debug logging (uses config.debug flag)
   */
  protected log(...args: unknown[]): void {
    if (this.config.debug) {
      console.log(`🔄 ${this.config.widgetName}:`, ...args);
    }
  }

  /**
   * Verbose logging helper for browser widgets
   * Usage: this.verbose() && console.log('message');
   * Enable with: window.JTAG_VERBOSE = true
   */
  protected verbose(): boolean {
    return typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
  }

  /**
   * Set loading state and execute async operation
   */
  protected async withLoading<T>(operation: () => Promise<T>): Promise<T> {
    this.loading = true;
    this.error = null;
    try {
      return await operation();
    } catch (e) {
      this.error = e instanceof Error ? e.message : 'Unknown error';
      throw e;
    } finally {
      this.loading = false;
    }
  }
}

// Type for custom element registration
declare global {
  interface HTMLElementTagNameMap {
    // Subclasses will add their tag names here
  }
}
