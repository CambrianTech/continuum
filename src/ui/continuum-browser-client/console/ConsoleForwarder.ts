/**
 * Console Forwarder - Thin wrapper around unified logger daemon architecture
 * 
 * @deprecated Use ClientConsoleManager directly from logger daemon module
 * This wrapper maintained for backward compatibility
 */

import { ClientConsoleManager } from '../../../daemons/logger/client/ClientConsoleManager';
import type { ContinuumState } from '../types/BrowserClientTypes';

export class ConsoleForwarder {
  private consoleManager: ClientConsoleManager;

  constructor(private getState: () => ContinuumState, private getSessionId: () => string | null) {
    // Handle the case where session ID might be null for backward compatibility
    const sessionIdGetter = () => {
      const sessionId = this.getSessionId();
      if (!sessionId) {
        throw new Error('ConsoleForwarder requires a valid session ID');
      }
      return sessionId;
    };

    this.consoleManager = new ClientConsoleManager(this.getState, sessionIdGetter);
  }

  setExecuteCallback(callback: (command: string, params: Record<string, unknown>) => Promise<unknown>): void {
    this.consoleManager.setExecuteCallback(callback);
  }

  setWebSocketTransport(transport: any): void {
    this.consoleManager.setWebSocketTransport(transport);
  }

  processQueuedMessages(): void {
    this.consoleManager.processQueuedMessages();
  }

  getContext() {
    return this.consoleManager.getContext();
  }

  updateContext(newContext: any) {
    this.consoleManager.updateContext(newContext);
  }

  // Legacy methods for backward compatibility
  executeAndFlushConsoleMessageQueue(): void {
    this.processQueuedMessages();
  }

  performHealthCheck(): boolean {
    return this.consoleManager.getContext() !== null;
  }

  destroy(): void {
    this.consoleManager.destroy();
  }
}