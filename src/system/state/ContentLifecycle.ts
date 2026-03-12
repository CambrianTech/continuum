/**
 * ContentLifecycle - Widget lifecycle management for content tabs
 *
 * Like iOS UIViewController lifecycle (viewWillDisappear/viewDidDisappear)
 * or ASP.NET Page lifecycle. Widgets register themselves and get called
 * in order when their content tab is closing.
 *
 * ONE path for cleanup. No duplicate event listeners. No race conditions.
 *
 * Usage in widgets:
 *   connectedCallback() {
 *     ContentLifecycle.register(this.entityId, this);
 *   }
 *   disconnectedCallback() {
 *     ContentLifecycle.unregister(this.entityId, this);
 *   }
 *   async willClose() {
 *     await this.saveState();
 *     await this.disconnectAudioVideo();
 *     await this.notifyServer();
 *   }
 */

/**
 * Interface for widgets that participate in content lifecycle.
 * Widgets implement the hooks they need; all are optional.
 */
export interface ContentLifecycleParticipant {
  /**
   * Called BEFORE the tab is removed. Async — cleanup can await network calls.
   * This is the iOS viewWillDisappear equivalent.
   *
   * Use for:
   * - Saving state to server
   * - Disconnecting audio/video streams
   * - Notifying server of departure (live/leave, etc.)
   * - Releasing held resources
   *
   * @returns Promise that resolves when cleanup is complete
   */
  willClose?(): Promise<void>;

  /**
   * Called AFTER the tab has been removed from state.
   * Synchronous — for local-only cleanup that doesn't need async.
   *
   * Use for:
   * - Clearing local caches
   * - Removing DOM observers
   * - Resetting widget state
   */
  didClose?(): void;
}

/**
 * Registry of widgets participating in content lifecycle.
 * Keyed by entityId (the content being displayed), value is set of participants.
 * Multiple widgets can register for the same entityId.
 */
class ContentLifecycleRegistry {
  private _participants = new Map<string, Set<ContentLifecycleParticipant>>();

  /**
   * Register a widget for lifecycle callbacks on a content entity.
   * Call in connectedCallback() or onConnect().
   */
  register(entityId: string, participant: ContentLifecycleParticipant): void {
    if (!entityId) return;
    let set = this._participants.get(entityId);
    if (!set) {
      set = new Set();
      this._participants.set(entityId, set);
    }
    set.add(participant);
  }

  /**
   * Unregister a widget. Call in disconnectedCallback() or onDisconnect().
   * Safe to call multiple times or with unregistered participant.
   */
  unregister(entityId: string, participant: ContentLifecycleParticipant): void {
    if (!entityId) return;
    const set = this._participants.get(entityId);
    if (!set) return;
    set.delete(participant);
    if (set.size === 0) {
      this._participants.delete(entityId);
    }
  }

  /**
   * Notify all registered participants that content is about to close.
   * Called by ContentService.close() BEFORE removing the tab.
   *
   * All willClose() calls run concurrently with a safety timeout.
   * Failures are logged but don't block the close.
   */
  async notifyWillClose(entityId: string): Promise<void> {
    if (!entityId) return;
    const set = this._participants.get(entityId);
    if (!set || set.size === 0) return;

    const TIMEOUT_MS = 5000; // Don't let a stuck widget block tab close

    const promises = Array.from(set).map(async (participant) => {
      if (!participant.willClose) return;
      try {
        await Promise.race([
          participant.willClose(),
          new Promise<void>((_, reject) =>
            setTimeout(() => reject(new Error('willClose timeout')), TIMEOUT_MS)
          )
        ]);
      } catch (err) {
        console.warn('ContentLifecycle: willClose failed for participant:', err);
      }
    });

    await Promise.all(promises);
  }

  /**
   * Notify all registered participants that content has been closed.
   * Called by ContentService.close() AFTER removing the tab.
   * Synchronous — runs immediately.
   */
  notifyDidClose(entityId: string): void {
    if (!entityId) return;
    const set = this._participants.get(entityId);
    if (!set || set.size === 0) return;

    for (const participant of set) {
      if (!participant.didClose) continue;
      try {
        participant.didClose();
      } catch (err) {
        console.warn('ContentLifecycle: didClose failed for participant:', err);
      }
    }

    // Auto-cleanup: remove all participants for this entity
    this._participants.delete(entityId);
  }

  /**
   * Check if any participants are registered for an entity.
   * Useful for debugging.
   */
  hasParticipants(entityId: string): boolean {
    const set = this._participants.get(entityId);
    return !!set && set.size > 0;
  }

  /**
   * Get count of registered participants (for diagnostics).
   */
  get registeredCount(): number {
    let count = 0;
    for (const set of this._participants.values()) {
      count += set.size;
    }
    return count;
  }
}

/** Singleton — import and use directly */
export const ContentLifecycle = new ContentLifecycleRegistry();
