/**
 * WidgetStateRegistry - Dynamic widget state registry
 *
 * Part of the Positronic Reactive State Architecture (Site → Page → Widget → Control).
 *
 * Widgets register their state slices dynamically:
 * - settings: { section, provider, hasPendingChanges }
 * - browser: { url, title, canInteract }
 * - chat: { roomId, messageCount, typing }
 *
 * Features:
 * - Dynamic registration (widgets register on init, unregister on disconnect)
 * - Per-widget stores with their own subscribers
 * - Global listener for any widget state change (for RAG context)
 * - Automatic timestamping for staleness detection
 */

import { ReactiveStore } from './ReactiveStore';

/**
 * A widget's state slice in the registry
 */
export interface WidgetStateSlice {
  /** Widget type identifier (e.g., 'settings', 'chat', 'browser') */
  widgetType: string;
  /** Arbitrary widget state data */
  data: Record<string, unknown>;
  /** When this state was last updated */
  updatedAt: number;
}

/**
 * Listener for any widget state change
 */
export type GlobalWidgetStateListener = (widgetType: string, slice: WidgetStateSlice) => void;

/**
 * Widget State Registry implementation
 */
class WidgetStateRegistryImpl {
  private _slices = new Map<string, ReactiveStore<WidgetStateSlice>>();
  private _globalListeners = new Set<GlobalWidgetStateListener>();

  /**
   * Register a widget's state slice
   *
   * Call this in widget's connectedCallback() or onWidgetInitialize()
   *
   * @param widgetType - Unique identifier for this widget type
   * @param initialData - Initial state data
   * @returns The reactive store for this widget's state
   */
  register(widgetType: string, initialData: Record<string, unknown> = {}): ReactiveStore<WidgetStateSlice> {
    // Create store for this widget
    const store = new ReactiveStore<WidgetStateSlice>({
      widgetType,
      data: initialData,
      updatedAt: Date.now()
    });

    // Subscribe to forward changes to global listeners
    store.subscribe(slice => {
      this.notifyGlobalListeners(widgetType, slice);
    });

    this._slices.set(widgetType, store);
    console.log(`📊 WidgetStateRegistry: Registered "${widgetType}"`);

    return store;
  }

  /**
   * Unregister a widget's state slice
   *
   * Call this in widget's disconnectedCallback()
   */
  unregister(widgetType: string): void {
    const store = this._slices.get(widgetType);
    if (store) {
      store.clearSubscribers();
      this._slices.delete(widgetType);
      console.log(`📊 WidgetStateRegistry: Unregistered "${widgetType}"`);
    }
  }

  /**
   * Get a specific widget's state
   */
  get(widgetType: string): WidgetStateSlice | null {
    return this._slices.get(widgetType)?.get() || null;
  }

  /**
   * Get a widget's reactive store (for subscribing)
   */
  getStore(widgetType: string): ReactiveStore<WidgetStateSlice> | undefined {
    return this._slices.get(widgetType);
  }

  /**
   * Get all registered widget states
   */
  getAll(): Map<string, WidgetStateSlice> {
    const result = new Map<string, WidgetStateSlice>();
    for (const [type, store] of this._slices) {
      result.set(type, store.get());
    }
    return result;
  }

  /**
   * Get all widget states that are recent (not stale)
   *
   * @param maxAgeMs - Maximum age in milliseconds (default 60s)
   */
  getRecent(maxAgeMs = 60000): Map<string, WidgetStateSlice> {
    const now = Date.now();
    const result = new Map<string, WidgetStateSlice>();

    for (const [type, store] of this._slices) {
      const slice = store.get();
      if (now - slice.updatedAt < maxAgeMs) {
        result.set(type, slice);
      }
    }

    return result;
  }

  /**
   * Subscribe to any widget state change
   *
   * Useful for:
   * - RAG context updates
   * - Debug/monitoring tools
   * - Cross-widget coordination
   */
  subscribeAll(callback: GlobalWidgetStateListener): () => void {
    this._globalListeners.add(callback);

    // Immediately notify with current states
    for (const [type, store] of this._slices) {
      try {
        callback(type, store.get());
      } catch (error) {
        console.error('WidgetStateRegistry: Error in global listener:', error);
      }
    }

    return () => {
      this._globalListeners.delete(callback);
    };
  }

  /**
   * Check if a widget type is registered
   */
  has(widgetType: string): boolean {
    return this._slices.has(widgetType);
  }

  /**
   * Get count of registered widgets
   */
  get count(): number {
    return this._slices.size;
  }

  /**
   * Get list of registered widget types
   */
  get registeredTypes(): string[] {
    return Array.from(this._slices.keys());
  }

  /**
   * Clear all registered widgets (for testing/reset)
   */
  clear(): void {
    for (const store of this._slices.values()) {
      store.clearSubscribers();
    }
    this._slices.clear();
    this._globalListeners.clear();
  }

  /**
   * Notify global listeners of a widget state change
   */
  private notifyGlobalListeners(widgetType: string, slice: WidgetStateSlice): void {
    for (const listener of this._globalListeners) {
      try {
        listener(widgetType, slice);
      } catch (error) {
        console.error('WidgetStateRegistry: Error in global listener:', error);
      }
    }
  }
}

/**
 * Singleton instance
 */
export const widgetStateRegistry = new WidgetStateRegistryImpl();

/**
 * Export class type for external use
 */
export type { WidgetStateRegistryImpl as WidgetStateRegistry };
