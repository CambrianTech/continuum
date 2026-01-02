/**
 * WidgetContextService - Server-side store for widget context
 *
 * Bridges browser UI state to server-side RAG pipeline.
 * When users navigate or interact with widgets, this context
 * is captured and made available for AI awareness.
 *
 * Key principle: AI should know what the user is viewing when they ask questions.
 * "You're viewing Settings > AI Providers" enables contextually-aware responses.
 *
 * Usage:
 *   // Store context (from event subscription)
 *   WidgetContextService.setContext(sessionId, context);
 *
 *   // Retrieve for RAG (in ChatRAGBuilder)
 *   const context = WidgetContextService.getContext(sessionId);
 *   const ragString = WidgetContextService.toRAGContext(sessionId);
 */

import type { PositronicContext, WidgetContext } from '../../../widgets/shared/services/state/PositronWidgetState';
import { Events } from '../../core/shared/Events';
import { EVENT_METADATA_KEYS } from '../../events/shared/EventSystemConstants';

/**
 * Stored context with metadata
 */
interface StoredContext {
  context: PositronicContext;
  sessionId: string;
  receivedAt: number;
  expiresAt: number;
}

/**
 * Stored RAG string (pre-formatted context from PositronicRAGContext)
 */
interface StoredRAGString {
  ragString: string;
  sessionId: string;
  receivedAt: number;
  expiresAt: number;
}

/**
 * WidgetContextService - Singleton service for server-side widget context storage
 */
class WidgetContextServiceImpl {
  private contexts: Map<string, StoredContext> = new Map();
  private ragStrings: Map<string, StoredRAGString> = new Map(); // Pre-formatted RAG strings
  private readonly TTL_MS = 30 * 60 * 1000; // 30 minutes
  private cleanupInterval: NodeJS.Timeout | null = null;
  private initialized = false;

  /**
   * Initialize the service and start listening for context events
   */
  initialize(): void {
    if (this.initialized) return;
    this.initialized = true;

    // Subscribe to widget context changes from browser
    // Events are bridged with _originalContext containing the sessionId
    Events.subscribe<PositronicContext & { [key: string]: unknown }>(
      'positron:widget-context-changed',
      (payload) => {
        // Extract sessionId from bridge metadata (_originalContext)
        const sessionId = payload[EVENT_METADATA_KEYS.ORIGINAL_CONTEXT] as string | undefined;
        if (sessionId && payload.widget) {
          // Create clean context without bridge metadata
          const context: PositronicContext = {
            widget: payload.widget,
            interaction: payload.interaction,
            breadcrumb: payload.breadcrumb,
            dwellTimeMs: payload.dwellTimeMs
          };
          this.setContext(sessionId, context);
          console.log(`🧠 WidgetContextService: Stored context for session ${sessionId.slice(0, 8)}:`, payload.widget.widgetType);
        } else if (payload.widget) {
          // No sessionId - store with 'unknown' key as fallback
          const context: PositronicContext = {
            widget: payload.widget,
            interaction: payload.interaction,
            breadcrumb: payload.breadcrumb,
            dwellTimeMs: payload.dwellTimeMs
          };
          this.setContext('fallback', context);
          console.log('🧠 WidgetContextService: Stored context (no sessionId):', payload.widget.widgetType);
        }
      }
    );

    // Subscribe to pre-formatted RAG strings from PositronicRAGContext
    Events.subscribe<{ ragString: string; [key: string]: unknown }>(
      'positron:rag-context-updated',
      (payload) => {
        const sessionId = payload[EVENT_METADATA_KEYS.ORIGINAL_CONTEXT] as string | undefined;
        if (sessionId && payload.ragString) {
          this.setRAGString(sessionId, payload.ragString);
          console.log(`🧠 WidgetContextService: Stored RAG string for session ${sessionId.slice(0, 8)} (${payload.ragString.length} chars)`);
        } else if (payload.ragString) {
          this.setRAGString('fallback', payload.ragString);
          console.log('🧠 WidgetContextService: Stored RAG string (no sessionId)');
        }
      }
    );

    // Start cleanup interval
    this.cleanupInterval = setInterval(() => this.cleanup(), 5 * 60 * 1000);

    console.log('🧠 WidgetContextService: Initialized');
  }

  /**
   * Store widget context for a session
   */
  setContext(sessionId: string, context: PositronicContext): void {
    const now = Date.now();
    this.contexts.set(sessionId, {
      context,
      sessionId,
      receivedAt: now,
      expiresAt: now + this.TTL_MS
    });
  }

  /**
   * Store pre-formatted RAG string for a session
   * (from new PositronicRAGContext system)
   */
  setRAGString(sessionId: string, ragString: string): void {
    const now = Date.now();
    console.log(`🧠 WidgetContextService.setRAGString: Storing for session ${sessionId}, size=${this.ragStrings.size} before`);
    this.ragStrings.set(sessionId, {
      ragString,
      sessionId,
      receivedAt: now,
      expiresAt: now + this.TTL_MS
    });
    console.log(`🧠 WidgetContextService.setRAGString: size=${this.ragStrings.size} after, keys=[${Array.from(this.ragStrings.keys()).join(', ')}]`);
  }

  /**
   * Get pre-formatted RAG string for a session
   */
  getRAGString(sessionId: string): string | null {
    const stored = this.ragStrings.get(sessionId);
    if (!stored) return null;

    // Check expiry
    if (Date.now() > stored.expiresAt) {
      this.ragStrings.delete(sessionId);
      return null;
    }

    return stored.ragString;
  }

  /**
   * Get most recent RAG string (fallback when sessionId unknown)
   */
  getMostRecentRAGString(): string | null {
    let mostRecent: StoredRAGString | null = null;
    const now = Date.now();

    console.log(`🧠 WidgetContextService.getMostRecentRAGString: checking ${this.ragStrings.size} entries`);
    for (const stored of this.ragStrings.values()) {
      console.log(`🧠   - session ${stored.sessionId}: expired=${now > stored.expiresAt}, age=${now - stored.receivedAt}ms`);
      if (now > stored.expiresAt) continue;
      if (!mostRecent || stored.receivedAt > mostRecent.receivedAt) {
        mostRecent = stored;
      }
    }

    console.log(`🧠 WidgetContextService.getMostRecentRAGString: found=${!!mostRecent}`);
    return mostRecent?.ragString ?? null;
  }

  /**
   * Get widget context for a session
   */
  getContext(sessionId: string): PositronicContext | null {
    const stored = this.contexts.get(sessionId);
    if (!stored) return null;

    // Check expiry
    if (Date.now() > stored.expiresAt) {
      this.contexts.delete(sessionId);
      return null;
    }

    return stored.context;
  }

  /**
   * Get context for any user (fallback when sessionId unknown)
   * Returns most recent context if available
   */
  getMostRecentContext(): PositronicContext | null {
    let mostRecent: StoredContext | null = null;
    const now = Date.now();

    for (const stored of this.contexts.values()) {
      if (now > stored.expiresAt) continue;
      if (!mostRecent || stored.receivedAt > mostRecent.receivedAt) {
        mostRecent = stored;
      }
    }

    return mostRecent?.context ?? null;
  }

  /**
   * Generate RAG-friendly context string for a session
   * Used by ChatRAGBuilder to inject into AI prompts
   *
   * Priority order:
   * 1. Pre-formatted RAG string (from new PositronicRAGContext system)
   * 2. Formatted widget context (from old PositronWidgetState system)
   */
  toRAGContext(sessionId?: string): string | null {
    // Priority 1: Check for new-style pre-formatted RAG strings
    const ragString = sessionId
      ? this.getRAGString(sessionId)
      : this.getMostRecentRAGString();

    if (ragString) {
      return ragString;
    }

    // Priority 2: Fall back to old-style widget context
    const context = sessionId
      ? this.getContext(sessionId)
      : this.getMostRecentContext();

    if (!context) return null;

    return this.formatContextForRAG(context);
  }

  /**
   * Format PositronicContext as a string for RAG injection
   */
  private formatContextForRAG(context: PositronicContext): string {
    const { widget, interaction, breadcrumb } = context;
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

    // Breadcrumb for navigation context
    if (breadcrumb && breadcrumb.length > 0) {
      parts.push(`Navigation path: ${breadcrumb.join(' → ')} → ${widget.title || widget.widgetType}`);
    }

    // Selective metadata (avoid dumping everything)
    if (widget.metadata) {
      const relevantKeys = [
        'selectedProvider', 'selectedTheme', 'errorCount', 'logLevel', 'room',
        // Settings test results
        'lastTestedProvider', 'lastTestSuccess', 'lastTestStatus', 'lastTestMessage',
        'needsHelp', 'helpContext',
        // Provider info
        'configuredProviders', 'totalProviders', 'hasPendingChanges'
      ];
      const relevant = Object.entries(widget.metadata)
        .filter(([key]) => relevantKeys.includes(key))
        .map(([key, value]) => `${key}: ${value}`);
      if (relevant.length > 0) {
        parts.push(`Context: ${relevant.join(', ')}`);
      }

      // Add special formatting for help context
      if (widget.metadata.helpContext) {
        parts.push(`\n⚠️ User needs help: ${widget.metadata.helpContext}`);
      }
    }

    return parts.join('\n');
  }

  /**
   * Clean up expired contexts and RAG strings
   */
  private cleanup(): void {
    const now = Date.now();
    let cleanedContexts = 0;
    let cleanedRagStrings = 0;

    for (const [sessionId, stored] of this.contexts.entries()) {
      if (now > stored.expiresAt) {
        this.contexts.delete(sessionId);
        cleanedContexts++;
      }
    }

    for (const [sessionId, stored] of this.ragStrings.entries()) {
      if (now > stored.expiresAt) {
        this.ragStrings.delete(sessionId);
        cleanedRagStrings++;
      }
    }

    const total = cleanedContexts + cleanedRagStrings;
    if (total > 0) {
      console.log(`🧠 WidgetContextService: Cleaned up ${cleanedContexts} context(s), ${cleanedRagStrings} RAG string(s)`);
    }
  }

  /**
   * Shutdown the service
   */
  shutdown(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = null;
    }
    this.contexts.clear();
    this.ragStrings.clear();
    this.initialized = false;
  }

  /**
   * Get stats for debugging
   */
  getStats(): { activeContexts: number; activeRagStrings: number; initialized: boolean } {
    return {
      activeContexts: this.contexts.size,
      activeRagStrings: this.ragStrings.size,
      initialized: this.initialized
    };
  }
}

// Singleton export
export const WidgetContextService = new WidgetContextServiceImpl();
