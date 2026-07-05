/**
 * DOMInterestRegistry - Daemon-free registry of DOM event interest
 *
 * Tracks which event names have widgets listening for DOM CustomEvents.
 * Widgets register interest here (instead of importing EventsDaemonBrowser),
 * and EventsDaemonBrowser consults this registry to decide which bridged
 * events get dispatched to the document.
 *
 * Lives in widgets/shared so the browser widget bundle stays daemon-free.
 *
 * Reference-counted: the same event name can be registered by multiple
 * widgets/services; interest persists until every registration is released.
 */

export class DOMInterestRegistryImpl {
  // Interest count per event name — entry removed when the count drops to zero
  private readonly _counts = new Map<string, number>();

  /**
   * Register interest in receiving DOM CustomEvents for a specific event name.
   * Returns an unregister function that releases this registration exactly once.
   */
  register(eventName: string): () => void {
    this._counts.set(eventName, (this._counts.get(eventName) ?? 0) + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.unregister(eventName);
    };
  }

  /**
   * Release one registration for an event name.
   * Interest is removed only when all registrations are released.
   */
  unregister(eventName: string): void {
    const count = this._counts.get(eventName);
    if (count === undefined) return;
    if (count <= 1) {
      this._counts.delete(eventName);
    } else {
      this._counts.set(eventName, count - 1);
    }
  }

  /**
   * Check if at least one registration exists for this exact event name.
   */
  has(eventName: string): boolean {
    return this._counts.has(eventName);
  }

  /**
   * Current registration count for an event name (0 when none).
   */
  interestCount(eventName: string): number {
    return this._counts.get(eventName) ?? 0;
  }

  /**
   * Iterate registered event names (used by EventsDaemonBrowser for prefix matching).
   */
  eventNames(): IterableIterator<string> {
    return this._counts.keys();
  }
}

/**
 * Singleton instance
 */
export const domInterestRegistry = new DOMInterestRegistryImpl();

/**
 * Export class type for external use
 */
export type { DOMInterestRegistryImpl as DOMInterestRegistry };
