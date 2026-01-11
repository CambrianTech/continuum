/**
 * PositronWidgetState - Shared Widget Context for AI Awareness
 *
 * Implements Positron Layer 1 (Session State) for widget context.
 * Enables AI assistants to understand what the user is viewing/doing.
 *
 * Key principle: This feeds into AI inbox, enabling PROACTIVE assistance.
 * AIs aren't just reactive Q&A - they observe context and can autonomously
 * queue tasks, offer suggestions, or prepare relevant information.
 *
 * Usage:
 *   // Center widget emits its state
 *   PositronWidgetState.emit({
 *     widgetType: 'settings',
 *     section: 'ai-providers',
 *     metadata: { selectedProvider: 'anthropic' }
 *   });
 *
 *   // AssistantPanel subscribes for RAG context
 *   PositronWidgetState.subscribe((context) => {
 *     this.updateRAGContext(context);
 *   });
 *
 *   // PersonaUser inbox can also subscribe for autonomous tasks
 *   PositronWidgetState.subscribe((context) => {
 *     this.considerProactiveAssistance(context);
 *   });
 */

import { Events } from '@system/core/shared/Events';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

/**
 * Widget context - what's currently displayed in the center
 */
export interface WidgetContext {
  /** Widget type identifier */
  readonly widgetType: string;

  /** Current section/tab within the widget */
  readonly section?: string;

  /** Entity being viewed (roomId, userId, documentId, etc.) */
  readonly entityId?: string;

  /** Human-readable title */
  readonly title?: string;

  /** Widget-specific metadata */
  readonly metadata?: Record<string, unknown>;

  /** Timestamp when context was set */
  readonly timestamp: number;
}

/**
 * Interaction hint - what the user is doing
 */
export interface InteractionHint {
  /** Type of interaction */
  readonly action: 'viewing' | 'editing' | 'searching' | 'configuring' | 'debugging';

  /** What they're interacting with */
  readonly target?: string;

  /** Additional context */
  readonly details?: string;
}

/**
 * Full Positronic context for AI awareness
 */
export interface PositronicContext {
  /** Current widget context */
  readonly widget: WidgetContext;

  /** Recent interaction hint */
  readonly interaction?: InteractionHint;

  /** Navigation breadcrumb (where they came from) */
  readonly breadcrumb?: string[];

  /** Time spent on current context (for relevance weighting) */
  readonly dwellTimeMs?: number;
}

type ContextSubscriber = (context: PositronicContext) => void;

/**
 * Widget event handler type
 */
type WidgetEventHandler = (data: unknown) => void;

/**
 * Widget event subscription tracking
 */
interface WidgetSubscription {
  widgetType: string;
  event: string;
  handler: WidgetEventHandler;
}

/**
 * PositronWidgetState - Singleton state service
 *
 * Bridges UI state to AI cognition. When user navigates or interacts,
 * this state updates, and subscribed AIs can observe and respond.
 *
 * Also provides widget-to-widget pub/sub for reactive patterns.
 */
class PositronWidgetStateService {
  private currentContext: PositronicContext | null = null;
  private subscribers: Set<ContextSubscriber> = new Set();
  private contextHistory: WidgetContext[] = [];
  private readonly MAX_HISTORY = 10;
  private dwellStartTime: number = Date.now();

  // Widget-to-widget pub/sub registry
  // Map<widgetType, Map<eventName, Set<handler>>>
  private widgetEventSubscribers = new Map<string, Map<string, Set<WidgetEventHandler>>>();

  // Telemetry tracking
  private eventMetrics = {
    emitCount: 0,
    subscribeCount: 0,
    lastEmitLatency: 0
  };

  /**
   * Emit new widget context
   * Called by center widgets when they render or navigate
   */
  emit(widget: Omit<WidgetContext, 'timestamp'>, interaction?: InteractionHint): void {
    const now = Date.now();

    // Calculate dwell time on previous context
    const dwellTimeMs = this.currentContext
      ? now - this.dwellStartTime
      : 0;

    // Build breadcrumb from history
    const breadcrumb = this.contextHistory
      .slice(-3)
      .map(ctx => ctx.title || ctx.widgetType);

    // Add previous context to history (if exists and meaningful dwell time)
    if (this.currentContext?.widget && dwellTimeMs > 1000) {
      this.contextHistory.push(this.currentContext.widget);
      if (this.contextHistory.length > this.MAX_HISTORY) {
        this.contextHistory.shift();
      }
    }

    // Create new context
    const widgetWithTimestamp: WidgetContext = {
      ...widget,
      timestamp: now
    };

    this.currentContext = {
      widget: widgetWithTimestamp,
      interaction,
      breadcrumb: breadcrumb.length > 0 ? breadcrumb : undefined,
      dwellTimeMs
    };

    this.dwellStartTime = now;

    // Notify subscribers
    this.notifySubscribers();

    // Also emit as event for cross-component communication
    Events.emit('positron:widget-context-changed', this.currentContext);

    // Bridge context to server via command (bypasses event routing issues)
    // This ensures AI RAG context includes widget state
    this.bridgeToServer();

    verbose() && console.log('🧠 PositronWidgetState: Context updated', {
      widgetType: widget.widgetType,
      section: widget.section,
      interaction: interaction?.action
    });
  }

  /**
   * Bridge context to server via command
   * The event system has routing issues browser→server, so use commands directly
   */
  private async bridgeToServer(): Promise<void> {
    if (!this.currentContext) return;

    // Debounce to avoid flooding server with every interaction
    if (this.bridgeTimeout) {
      clearTimeout(this.bridgeTimeout);
    }

    this.bridgeTimeout = setTimeout(() => {
      this.executeBridgeCommand();
    }, 200);  // 200ms debounce
  }

  /**
   * Execute the bridge command (separated for cleaner async handling)
   */
  private async executeBridgeCommand(): Promise<void> {
    if (!this.currentContext) return;

    try {
      verbose() && console.log('🧠 PositronWidgetState: Bridging context to server...');
      const { Commands } = await import('../../../../system/core/shared/Commands');
      const sessionId = await this.getSessionId();

      verbose() && console.log('🧠 PositronWidgetState: Calling widget-state command with session:', sessionId);
      const result = await Commands.execute('development/debug/widget-state', {
        setContext: this.currentContext,
        contextSessionId: sessionId
      } as any);

      verbose() && console.log('🧠 PositronWidgetState: Bridge result:', result);
    } catch (error) {
      console.error('🧠 PositronWidgetState: Bridge to server failed:', error);
    }
  }

  /**
   * Get current session ID for context association
   */
  private async getSessionId(): Promise<string> {
    try {
      const { Commands } = await import('../../../../system/core/shared/Commands');
      const result = await Commands.execute('session/get-id', {} as any) as any;
      return result?.sessionId || 'unknown';
    } catch {
      return 'unknown';
    }
  }

  private bridgeTimeout: NodeJS.Timeout | null = null;

  /**
   * Update interaction hint without changing widget context
   * For fine-grained activity tracking
   */
  updateInteraction(interaction: InteractionHint): void {
    if (!this.currentContext) return;

    this.currentContext = {
      ...this.currentContext,
      interaction
    };

    this.notifySubscribers();
  }

  /**
   * Subscribe to context changes
   * Returns unsubscribe function
   */
  subscribe(callback: ContextSubscriber): () => void {
    this.subscribers.add(callback);

    // Immediately call with current context if available
    if (this.currentContext) {
      callback(this.currentContext);
    }

    return () => {
      this.subscribers.delete(callback);
    };
  }

  /**
   * Get current context (for one-time reads)
   */
  get context(): PositronicContext | null {
    return this.currentContext;
  }

  /**
   * Get context history (for AI to understand navigation patterns)
   */
  get history(): readonly WidgetContext[] {
    return this.contextHistory;
  }

  /**
   * Generate RAG-friendly context string
   * Used by AssistantPanel to inject into AI prompts
   */
  toRAGContext(): string {
    if (!this.currentContext) {
      return 'User context: Unknown (no widget context available)';
    }

    const { widget, interaction, breadcrumb } = this.currentContext;
    const parts: string[] = [];

    // Widget context
    parts.push(`Current view: ${widget.title || widget.widgetType}`);
    if (widget.section) {
      parts.push(`Section: ${widget.section}`);
    }

    // Interaction hint
    if (interaction) {
      parts.push(`User is ${interaction.action}${interaction.target ? ` ${interaction.target}` : ''}`);
      if (interaction.details) {
        parts.push(`Details: ${interaction.details}`);
      }
    }

    // Breadcrumb for context
    if (breadcrumb && breadcrumb.length > 0) {
      parts.push(`Navigation path: ${breadcrumb.join(' → ')} → ${widget.title || widget.widgetType}`);
    }

    // Metadata (selective - avoid dumping everything)
    if (widget.metadata) {
      const relevantKeys = ['selectedProvider', 'selectedTheme', 'errorCount', 'logLevel'];
      const relevant = Object.entries(widget.metadata)
        .filter(([key]) => relevantKeys.includes(key))
        .map(([key, value]) => `${key}: ${value}`);
      if (relevant.length > 0) {
        parts.push(`Context: ${relevant.join(', ')}`);
      }
    }

    return parts.join('\n');
  }

  /**
   * Generate structured context for PersonaUser inbox
   * Enables proactive task generation
   */
  toInboxContext(): {
    widgetType: string;
    relevantDomains: string[];
    suggestedActions: string[];
  } | null {
    if (!this.currentContext) return null;

    const { widget, interaction } = this.currentContext;

    // Map widget types to cognitive domains
    const domainMap: Record<string, string[]> = {
      'settings': ['configuration', 'troubleshooting', 'onboarding'],
      'help': ['documentation', 'tutorials', 'faq'],
      'chat': ['conversation', 'collaboration'],
      'logs': ['debugging', 'monitoring', 'troubleshooting'],
      'theme': ['customization', 'accessibility'],
      'profile': ['identity', 'preferences'],
      'diagnostics': ['debugging', 'performance', 'monitoring']
    };

    // Map interactions to suggested actions
    const actionMap: Record<string, string[]> = {
      'configuring': ['validate-config', 'suggest-improvements', 'detect-issues'],
      'debugging': ['analyze-errors', 'suggest-fixes', 'explain-logs'],
      'viewing': ['provide-context', 'anticipate-questions'],
      'searching': ['help-find', 'suggest-related'],
      'editing': ['validate-changes', 'suggest-best-practices']
    };

    return {
      widgetType: widget.widgetType,
      relevantDomains: domainMap[widget.widgetType] || ['general'],
      suggestedActions: interaction
        ? actionMap[interaction.action] || []
        : ['observe', 'be-ready-to-help']
    };
  }

  private notifySubscribers(): void {
    if (!this.currentContext) return;

    for (const subscriber of this.subscribers) {
      try {
        subscriber(this.currentContext);
      } catch (error) {
        console.error('PositronWidgetState: Subscriber error', error);
      }
    }
  }

  // ============================================
  // WIDGET-TO-WIDGET REACTIVE PUB/SUB
  // ============================================

  /**
   * Emit a named event from a widget type
   * Other widgets can subscribe to these events for reactive updates
   *
   * @param widgetType - The emitting widget's type (e.g., 'profile', 'chat')
   * @param event - Event name (e.g., 'status:changed', 'member:joined')
   * @param data - Event payload
   */
  emitWidgetEvent(widgetType: string, event: string, data: unknown): void {
    const start = performance.now();
    this.eventMetrics.emitCount++;

    const widgetSubs = this.widgetEventSubscribers.get(widgetType);
    if (!widgetSubs) {
      return; // No subscribers for this widget type
    }

    const eventSubs = widgetSubs.get(event);
    if (!eventSubs || eventSubs.size === 0) {
      return; // No subscribers for this event
    }

    // Notify all subscribers
    for (const handler of eventSubs) {
      try {
        handler(data);
      } catch (error) {
        console.error(`PositronWidgetState: Widget event handler error [${widgetType}:${event}]`, error);
      }
    }

    this.eventMetrics.lastEmitLatency = performance.now() - start;
    verbose() && console.log(`🔄 PositronWidgetState: ${widgetType}:${event} → ${eventSubs.size} subscribers (${this.eventMetrics.lastEmitLatency.toFixed(1)}ms)`);
  }

  /**
   * Subscribe to a widget type's events
   * Returns unsubscribe function for cleanup
   *
   * @param widgetType - The widget type to subscribe to (e.g., 'profile')
   * @param event - Event name to subscribe to (e.g., 'status:changed')
   * @param handler - Callback function
   * @returns Unsubscribe function
   */
  subscribeToWidget(widgetType: string, event: string, handler: WidgetEventHandler): () => void {
    this.eventMetrics.subscribeCount++;

    // Ensure widget type map exists
    if (!this.widgetEventSubscribers.has(widgetType)) {
      this.widgetEventSubscribers.set(widgetType, new Map());
    }

    const widgetSubs = this.widgetEventSubscribers.get(widgetType)!;

    // Ensure event set exists
    if (!widgetSubs.has(event)) {
      widgetSubs.set(event, new Set());
    }

    widgetSubs.get(event)!.add(handler);

    verbose() && console.log(`🔗 PositronWidgetState: Subscribed to ${widgetType}:${event} (total: ${widgetSubs.get(event)!.size})`);

    // Return unsubscribe function
    return () => {
      const subs = this.widgetEventSubscribers.get(widgetType)?.get(event);
      if (subs) {
        subs.delete(handler);
        verbose() && console.log(`🔓 PositronWidgetState: Unsubscribed from ${widgetType}:${event} (remaining: ${subs.size})`);
      }
    };
  }

  /**
   * Batch unsubscribe - clean up all subscriptions for a list of widget types
   * Used by widgets in disconnectedCallback
   *
   * @param subscriptions - Array of subscription info to clean up
   */
  unsubscribeAll(subscriptions: WidgetSubscription[]): void {
    for (const sub of subscriptions) {
      const subs = this.widgetEventSubscribers.get(sub.widgetType)?.get(sub.event);
      if (subs) {
        subs.delete(sub.handler);
      }
    }
    verbose() && console.log(`🧹 PositronWidgetState: Cleaned up ${subscriptions.length} subscriptions`);
  }

  /**
   * Get event metrics for telemetry/debugging
   */
  getEventMetrics(): typeof this.eventMetrics {
    return { ...this.eventMetrics };
  }

  /**
   * Get subscription count for a widget type (debugging)
   */
  getSubscriptionCount(widgetType: string): number {
    const widgetSubs = this.widgetEventSubscribers.get(widgetType);
    if (!widgetSubs) return 0;

    let count = 0;
    for (const eventSubs of widgetSubs.values()) {
      count += eventSubs.size;
    }
    return count;
  }
}

// Singleton export
export const PositronWidgetState = new PositronWidgetStateService();

// Type export for external use
export type { ContextSubscriber, WidgetEventHandler, WidgetSubscription };
