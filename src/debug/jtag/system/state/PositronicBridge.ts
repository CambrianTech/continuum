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

import { Ping } from '../../commands/ping/shared/PingTypes';
import { WidgetStateDebug } from '../../commands/development/debug/widget-state/shared/WidgetStateDebugTypes';
import { SessionGetId } from '../../commands/session/get-id/shared/SessionGetIdTypes';
// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

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

    // DIAGNOSTIC: Immediately try to store a test string to verify Commands works
    this.testBridgeConnection();

    // Emit initial context after a short delay to ensure client is connected
    setTimeout(() => {
      verbose() && console.log('🌉 PositronicBridge.initialize: Timeout fired, calling emitRAGContext');
      this.emitRAGContext();
    }, 1000);
  }

  /**
   * Show visible diagnostic in the DOM for debugging
   */
  private showDiagnostic(message: string, isError: boolean = false): void {
    try {
      let diagnostic = document.getElementById('__positronic_diagnostic__');
      if (!diagnostic) {
        diagnostic = document.createElement('div');
        diagnostic.id = '__positronic_diagnostic__';
        diagnostic.style.cssText = `
          position: fixed;
          bottom: 10px;
          left: 10px;
          padding: 8px 12px;
          background: ${isError ? '#ff4444' : '#44ff44'};
          color: black;
          font-family: monospace;
          font-size: 12px;
          z-index: 99999;
          border-radius: 4px;
          max-width: 400px;
          word-wrap: break-word;
        `;
        document.body.appendChild(diagnostic);
      }
      diagnostic.textContent = `🌉 ${message}`;
      diagnostic.style.background = isError ? '#ff4444' : '#44ff44';
    } catch (e) {
      // Ignore DOM errors
    }
  }

  /**
   * Test the bridge connection by sending a test RAG string
   * This helps diagnose if Commands.execute() is working
   */
  private async testBridgeConnection(): Promise<void> {
    verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: Starting test...');
    this.showDiagnostic('Test starting...');

    // Store test results on window for debugging
    const testResults = {
      started: true,
      waitedForConnection: false,
      commandsImported: false,
      pingSucceeded: false,
      ragStringStored: false,
      error: null as string | null,
      timestamp: Date.now()
    };
    (window as any).__POSITRONIC_BRIDGE_TEST__ = testResults;

    // Wait for WebSocket connection to be established
    await this.waitForConnection(10000);
    testResults.waitedForConnection = true;
    this.showDiagnostic('Connection ready');
    verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: Connection wait complete');

    try {
      this.showDiagnostic('Importing Commands...');
      const { Commands } = await import('../core/shared/Commands');
      testResults.commandsImported = true;
      this.showDiagnostic('Commands imported');
      verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: Commands imported successfully');

      // First verify basic command routing works
      this.showDiagnostic('Testing ping...');
      verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: Testing with ping command...');
      const pingResult = await Ping.execute({} as any);
      testResults.pingSucceeded = true;
      this.showDiagnostic('Ping succeeded!');
      verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: Ping succeeded!', JSON.stringify(pingResult).slice(0, 100));

      // Now test the actual RAG string storage
      this.showDiagnostic('Storing RAG string...');
      const testString = `## Test RAG String\nGenerated at: ${new Date().toISOString()}\nSource: PositronicBridge.testBridgeConnection`;
      verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: About to call widget-state command...');

      const result = await WidgetStateDebug.execute({
        setRAGString: testString,
        contextSessionId: 'positronic-test'
      } as any);

      testResults.ragStringStored = true;
      this.showDiagnostic('SUCCESS! RAG stored');
      verbose() && console.log('🌉 PositronicBridge.testBridgeConnection: SUCCESS!', result);
    } catch (error) {
      testResults.error = error instanceof Error ? error.message : String(error);
      this.showDiagnostic(`FAILED: ${testResults.error}`, true);
      console.error('🌉 PositronicBridge.testBridgeConnection: FAILED!', error);
    }
  }

  /**
   * Wait for the JTAG client connection to be established
   * Note: globalThis.jtag is only set AFTER JTAGClientBrowser.connectLocal() completes,
   * so if it exists, the connection is established.
   */
  private async waitForConnection(timeoutMs: number): Promise<void> {
    const startTime = Date.now();
    const checkInterval = 100; // Check every 100ms for faster detection

    while (Date.now() - startTime < timeoutMs) {
      try {
        // CRITICAL: Check for jtag.commands, not just jtag existence
        // JTAGClient.sharedInstance checks for globalThis.jtag?.commands
        const jtag = (globalThis as any).jtag;
        if (typeof globalThis !== 'undefined' && jtag?.commands) {
          verbose() && console.log('🌉 PositronicBridge.waitForConnection: Client connected with commands!');
          return;
        }
        // Log what we found
        if (jtag && !jtag.commands) {
          verbose() && console.log('🌉 PositronicBridge.waitForConnection: jtag exists but no commands yet');
        }
      } catch (e) {
        // Ignore errors during check
      }

      if ((Date.now() - startTime) % 1000 < checkInterval) {
        verbose() && console.log(`🌉 PositronicBridge.waitForConnection: Waiting... (${Date.now() - startTime}ms)`);
      }
      await new Promise(resolve => setTimeout(resolve, checkInterval));
    }

    verbose() && console.log('🌉 PositronicBridge.waitForConnection: Timeout - proceeding anyway');
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
      const { Commands } = await import('../core/shared/Commands');
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
