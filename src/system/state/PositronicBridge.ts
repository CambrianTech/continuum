/**
 * PositronicBridge - Connects browser-side reactive state to server RAG context
 *
 * This bridge:
 * 1. Subscribes to all reactive state changes (site, page, widget)
 * 2. Generates RAG context string using PositronicRAGContext
 * 3. Uses Commands.execute() to store RAG string in WidgetContextService
 *
 * The bridge is initialized once when the browser app starts.
 * It automatically keeps the server-side RAG context in sync with browser state.
 *
 * Note: We use Commands instead of Events because the Events system doesn't
 * reliably bridge browser→server during early initialization (no router available).
 */

import { positronicContext } from './PositronicRAGContext';

import { WidgetStateDebug } from '../../commands/development/debug/widget-state/shared/WidgetStateDebugTypes';
import { SessionGetId } from '../../commands/session/get-id/shared/SessionGetIdTypes';
import { jtagWindow } from '../core/types/GlobalAugmentations';
// Verbose logging helper for browser
const verbose = () => jtagWindow?.JTAG_VERBOSE === true;

/**
 * PositronicBridge - Singleton that bridges browser state to server RAG
 */
class PositronicBridgeImpl {
  private _initialized = false;
  private _unsubscribe?: () => void;
  private _lastRAGString: string = '';
  private _bridgeTimeout: ReturnType<typeof setTimeout> | null = null;

  /**
   * Initialize the bridge
   * Call this once when the browser app starts (e.g., in ContinuumWidget or main entry)
   */
  initialize(): void {
    verbose() && console.log('🌉 PositronicBridge.initialize: Starting...');
    if (this._initialized) {
      verbose() && console.log('🌉 PositronicBridge: Already initialized');
      return;
    }

    this._initialized = true;
    verbose() && console.log('🌉 PositronicBridge.initialize: Set initialized flag');

    try {
      // Subscribe to all state changes with debouncing
      verbose() && console.log('🌉 PositronicBridge.initialize: About to subscribe to changes...');
      this._unsubscribe = positronicContext.subscribeToChanges(() => {
        this.emitRAGContext();
      }, 500); // 500ms debounce to avoid flooding
      verbose() && console.log('🌉 PositronicBridge.initialize: Subscribed to changes');
    } catch (error) {
      console.error('🌉 PositronicBridge.initialize: Failed to subscribe:', error);
    }

    verbose() && console.log('🌉 PositronicBridge: Initialized - bridging state to server RAG');

    // Emit initial context after a short delay to ensure client is connected
    setTimeout(() => {
      verbose() && console.log('🌉 PositronicBridge.initialize: Timeout fired, calling emitRAGContext');
      this.emitRAGContext();
    }, 1000);
  }

  /**
   * Emit current RAG context to server via Commands
   */
  private emitRAGContext(): void {
    verbose() && console.log('🌉 PositronicBridge.emitRAGContext: called');
    const ragString = positronicContext.toRAGString();
    verbose() && console.log(`🌉 PositronicBridge.emitRAGContext: ragString length=${ragString.length}, preview="${ragString.slice(0, 50)}..."`);

    // Avoid duplicate emissions
    if (ragString === this._lastRAGString) {
      verbose() && console.log('🌉 PositronicBridge.emitRAGContext: skipping duplicate');
      return;
    }
    this._lastRAGString = ragString;

    // Only emit if we have meaningful content
    if (!ragString || ragString.trim().length === 0) {
      verbose() && console.log('🌉 PositronicBridge.emitRAGContext: skipping empty');
      return;
    }

    // Debounce the bridge command
    if (this._bridgeTimeout) {
      clearTimeout(this._bridgeTimeout);
    }

    this._bridgeTimeout = setTimeout(() => {
      this.executeBridgeCommand(ragString);
    }, 200); // 200ms debounce
  }

  /**
   * Execute the bridge command to store RAG string on server
   */
  private async executeBridgeCommand(ragString: string): Promise<void> {
    verbose() && console.log(`🌉 PositronicBridge.executeBridgeCommand: Starting, length=${ragString.length}`);
    try {
      verbose() && console.log(`🌉 PositronicBridge: Bridging RAG context to server (${ragString.length} chars)`);

      // Dynamic import to avoid circular dependencies
      const { Commands } = await import('../core/shared/Commands');
      const sessionId = await this.getSessionId();

      await WidgetStateDebug.execute({
        setRAGString: ragString,
        contextSessionId: sessionId
      } as any);

      verbose() && console.log(`🌉 PositronicBridge: RAG context stored for session ${sessionId.slice(0, 8)}`);
    } catch (error) {
      console.error('🌉 PositronicBridge: Bridge to server failed:', error);
    }
  }

  /**
   * Get current session ID for context association
   */
  private async getSessionId(): Promise<string> {
    try {
      const result = await SessionGetId.execute({} as any) as any;
      return result?.sessionId || 'unknown';
    } catch {
      return 'unknown';
    }
  }

  /**
   * Force emit current context (useful for debugging)
   */
  forceEmit(): void {
    this._lastRAGString = ''; // Clear dedup
    this.emitRAGContext();
  }

  /**
   * Get current stats
   */
  getStats(): { initialized: boolean; lastEmitLength: number } {
    return {
      initialized: this._initialized,
      lastEmitLength: this._lastRAGString.length
    };
  }

  /**
   * Shutdown the bridge
   */
  shutdown(): void {
    if (this._unsubscribe) {
      this._unsubscribe();
      this._unsubscribe = undefined;
    }
    if (this._bridgeTimeout) {
      clearTimeout(this._bridgeTimeout);
      this._bridgeTimeout = null;
    }
    this._initialized = false;
    this._lastRAGString = '';
  }
}

/**
 * Singleton instance
 */
export const positronicBridge = new PositronicBridgeImpl();
