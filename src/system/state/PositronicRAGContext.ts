/**
 * PositronicRAGContext - Unified RAG context builder
 *
 * Part of the Positronic Reactive State Architecture.
 *
 * Combines all state layers into RAG-ready context for AI prompts:
 * - Site state (user, theme, session)
 * - Page state (current view, entity)
 * - Widget states (what each widget is showing)
 *
 * This enables AIs to be fully aware of what users are viewing and doing,
 * enabling contextual assistance like:
 * - "I see you're in Settings looking at Anthropic configuration..."
 * - "You're browsing example.com - I can see the page content..."
 * - "The chat shows 5 unread messages in General..."
 */

import { siteState, type SiteStateData } from './SiteState';
import { pageState, type PageState } from './PageStateService';
import { widgetStateRegistry, type WidgetStateSlice } from './WidgetStateRegistry';

/**
 * RAG context data structure
 */
export interface PositronicContextData {
  site: SiteStateData;
  page: PageState | null;
  widgets: Map<string, WidgetStateSlice>;
  generatedAt: number;
}

/**
 * Listener for context changes (debounced)
 */
export type ContextChangeListener = (context: PositronicContextData) => void;

/**
 * PositronicRAGContext implementation
 */
class PositronicRAGContextImpl {
  private _changeListeners = new Set<() => void>();
  private _debounceTimeouts = new Map<() => void, number>();

  /**
   * Get full context data structure
   */
  getContext(): PositronicContextData {
    return {
      site: siteState.get(),
      page: pageState.getContent(),
      widgets: widgetStateRegistry.getAll(),
      generatedAt: Date.now()
    };
  }

  /**
   * Generate RAG context string for AI prompts
   *
   * This is the primary output - a formatted string that gets injected
   * into the AI's context window.
   *
   * IMPORTANT: This ALWAYS returns a non-empty string to ensure the
   * PositronicBridge sends updates even during initialization.
   */
  toRAGString(): string {
    const site = siteState.get();
    const page = pageState.getContent();
    const widgets = widgetStateRegistry.getRecent(60000); // Only recent (< 1 min)

    const lines: string[] = [];

    // === Session Header (always present) ===
    lines.push('## User Context');

    // === Site Context ===
    if (site.displayName) {
      lines.push(`User: ${site.displayName}`);
    } else {
      lines.push('User: (anonymous)');
    }
    lines.push(`Theme: ${site.theme}`);

    // === Page Context ===
    lines.push('');
    if (page) {
      lines.push(`Current View: ${page.contentType}`);
      if (page.entityId) {
        const displayName = page.resolved?.displayName || page.entityId;
        lines.push(`Viewing: ${displayName}`);
      }
    } else {
      lines.push('Current View: (initializing)');
    }

    // === Widget States ===
    if (widgets.size > 0) {
      for (const [type, slice] of widgets) {
        const data = slice.data;
        if (Object.keys(data).length === 0) continue;

        lines.push('');
        lines.push(`${this.formatWidgetName(type)}:`);

        for (const [key, value] of Object.entries(data)) {
          // Skip internal/noise fields
          if (key.startsWith('_') || key === 'updatedAt') continue;

          // Format the value appropriately
          const formatted = this.formatValue(key, value);
          if (formatted) {
            lines.push(`  ${key}: ${formatted}`);
          }
        }
      }
    }

    return lines.join('\n');
  }

  /**
   * Generate compact RAG context (for token-constrained scenarios)
   */
  toCompactRAGString(): string {
    const site = siteState.get();
    const page = pageState.getContent();
    const widgets = widgetStateRegistry.getRecent(30000); // Only very recent

    const parts: string[] = [];

    // User info
    if (site.displayName) {
      parts.push(`User:${site.displayName}`);
    }

    // Current view
    if (page) {
      const viewPart = page.entityId
        ? `${page.contentType}/${page.resolved?.displayName || page.entityId}`
        : page.contentType;
      parts.push(`View:${viewPart}`);
    }

    // Key widget states (only most important)
    for (const [type, slice] of widgets) {
      const highlights = this.getWidgetHighlights(type, slice.data);
      if (highlights) {
        parts.push(`${type}:{${highlights}}`);
      }
    }

    return parts.join(' | ');
  }

  /**
   * Subscribe to any state change (debounced)
   *
   * Combines subscriptions to all three state layers.
   * Debounces notifications to avoid flooding on rapid changes.
   *
   * @param callback - Called when any state changes
   * @param debounceMs - Debounce interval (default 200ms)
   * @returns Unsubscribe function
   */
  subscribeToChanges(callback: () => void, debounceMs = 200): () => void {
    const debounced = () => {
      // Clear existing timeout
      const existing = this._debounceTimeouts.get(callback);
      if (existing) {
        clearTimeout(existing);
      }

      // Set new timeout
      const timeout = setTimeout(() => {
        this._debounceTimeouts.delete(callback);
        callback();
      }, debounceMs) as unknown as number;

      this._debounceTimeouts.set(callback, timeout);
    };

    // Subscribe to all state sources
    const unsubs: Array<() => void> = [];

    unsubs.push(siteState.subscribe(debounced));
    unsubs.push(pageState.subscribe(debounced));
    unsubs.push(widgetStateRegistry.subscribeAll(debounced));

    this._changeListeners.add(callback);

    // Return combined unsubscribe
    return () => {
      unsubs.forEach(fn => fn());
      this._changeListeners.delete(callback);

      // Clear any pending debounce
      const timeout = this._debounceTimeouts.get(callback);
      if (timeout) {
        clearTimeout(timeout);
        this._debounceTimeouts.delete(callback);
      }
    };
  }

  /**
   * Get summary stats (for debugging)
   */
  getStats(): {
    siteAuthenticated: boolean;
    pageContentType: string | undefined;
    widgetCount: number;
    recentWidgetCount: number;
  } {
    return {
      siteAuthenticated: siteState.isAuthenticated,
      pageContentType: pageState.getContent()?.contentType,
      widgetCount: widgetStateRegistry.count,
      recentWidgetCount: widgetStateRegistry.getRecent().size
    };
  }

  /**
   * Format widget type name for display
   */
  private formatWidgetName(type: string): string {
    // Convert kebab-case or camelCase to Title Case
    return type
      .replace(/-/g, ' ')
      .replace(/([A-Z])/g, ' $1')
      .replace(/widget$/i, '')
      .trim()
      .split(' ')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
      .join(' ')
      .trim();
  }

  /**
   * Format a value for RAG display
   */
  private formatValue(key: string, value: unknown): string | null {
    if (value === null || value === undefined) return null;
    if (value === '') return null;
    if (Array.isArray(value) && value.length === 0) return null;

    if (typeof value === 'string') {
      // Truncate long strings
      if (value.length > 200) {
        return value.slice(0, 200) + '...';
      }
      return value;
    }

    if (typeof value === 'boolean') {
      return value ? 'yes' : 'no';
    }

    if (typeof value === 'number') {
      return String(value);
    }

    if (Array.isArray(value)) {
      if (value.length <= 3) {
        return value.join(', ');
      }
      return `${value.slice(0, 3).join(', ')} (+${value.length - 3} more)`;
    }

    if (typeof value === 'object') {
      // Summarize objects
      const keys = Object.keys(value as object);
      if (keys.length <= 3) {
        return JSON.stringify(value);
      }
      return `{${keys.slice(0, 3).join(', ')}...}`;
    }

    return String(value);
  }

  /**
   * Get highlight values for a widget (compact representation)
   */
  private getWidgetHighlights(type: string, data: Record<string, unknown>): string | null {
    const highlights: string[] = [];

    // Widget-specific highlight extraction
    switch (type) {
      case 'settings':
        if (data.section) highlights.push(`section:${data.section}`);
        if (data.selectedProvider) highlights.push(`provider:${data.selectedProvider}`);
        break;

      case 'browser':
      case 'web-view':
        if (data.url) highlights.push(`url:${String(data.url).slice(0, 50)}`);
        if (data.title) highlights.push(`title:${String(data.title).slice(0, 30)}`);
        break;

      case 'chat':
        if (data.roomId) highlights.push(`room:${data.roomId}`);
        if (data.messageCount) highlights.push(`msgs:${data.messageCount}`);
        break;

      default:
        // Generic: take first 2 non-empty values
        for (const [key, value] of Object.entries(data)) {
          if (value && !key.startsWith('_') && highlights.length < 2) {
            highlights.push(`${key}:${String(value).slice(0, 20)}`);
          }
        }
    }

    return highlights.length > 0 ? highlights.join(',') : null;
  }
}

/**
 * Singleton instance
 */
export const positronicContext = new PositronicRAGContextImpl();

/**
 * Export class type for external use
 */
export type { PositronicRAGContextImpl as PositronicRAGContext };
