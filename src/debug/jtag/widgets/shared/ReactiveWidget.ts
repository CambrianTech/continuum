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

import { LitElement, html, css, type TemplateResult, type CSSResultGroup, type PropertyDeclaration } from 'lit';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import type { CommandParams, CommandResult } from '../../system/core/types/JTAGTypes';
import type { UserEntity } from '../../system/data/entities/UserEntity';
import { PositronWidgetState, type InteractionHint } from './services/state/PositronWidgetState';

// Re-export Lit utilities for subclasses
export { html, css, type TemplateResult, type CSSResultGroup };
export type { InteractionHint };

/**
 * Property decorator that works with TC39 standard decorators
 * Use: @reactive() myProp = initialValue;
 */
export function reactive(options?: PropertyDeclaration) {
  return function(target: undefined, context: ClassFieldDecoratorContext) {
    const fieldName = String(context.name);
    context.addInitializer(function(this: unknown) {
      // Register as reactive property on the class
      const ctor = (this as { constructor: typeof ReactiveWidget }).constructor;
      ctor.createProperty(fieldName, {
        ...options,
        state: true // Internal state, not reflected to attribute
      });
    });
  };
}

/**
 * Attribute property decorator - reflects to/from HTML attribute
 * Use: @attr() label = 'default';
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
    this.onConnect();
  }

  /**
   * Called when widget is removed from DOM
   * Override for cleanup logic
   */
  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.log('Disconnected from DOM');
    this.onDisconnect();
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
  }

  // Hooks for subclasses (cleaner than overriding lifecycle methods)
  protected onConnect(): void {}
  protected onDisconnect(): void {}
  protected onFirstRender(): void {}

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
    params?: Omit<P, 'context' | 'sessionId'>
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
    params?: Omit<P, 'context' | 'sessionId'>,
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
  // USER CONTEXT
  // ═══════════════════════════════════════════════════════════════════════════

  /**
   * Get current user from JTAG session
   */
  protected get currentUser(): UserEntity | undefined {
    const client = (window as unknown as WindowWithJTAG).jtag;
    return client?.user?.entity as UserEntity | undefined;
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
   * Debug logging
   */
  protected log(...args: unknown[]): void {
    if (this.config.debug) {
      console.log(`🔄 ${this.config.widgetName}:`, ...args);
    }
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
