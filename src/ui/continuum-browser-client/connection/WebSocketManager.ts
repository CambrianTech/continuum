// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * WebSocket Manager - Handles WebSocket connection and messaging
 * Manages connection lifecycle and message handling
 * 
 * ✅ CLEANED UP: Removed god object pattern - now delegates to modular command handlers (2025-07-13)
 * ✅ CLEANED UP: Uses event system for remote execution instead of hardcoded switch (2025-07-13)
 */

import type { WebSocketMessage, ClientInitData, RemoteExecutionRequest, RemoteExecutionResponse } from '../types/WebSocketTypes';
import type { ContinuumState } from '../types/BrowserClientTypes';

export class WebSocketManager {
  private ws: WebSocket | null = null;
  private version: string;
  
  // Callbacks
  private onStateChange: ((state: ContinuumState) => void) | undefined;
  private onClientId: ((clientId: string) => void) | undefined;
  private onSessionId: ((sessionId: string) => void) | undefined;
  private onMessage: ((message: Record<string, unknown>) => void) | undefined;

  constructor(version: string) {
    this.version = version;
  }

  setCallbacks(callbacks: {
    onStateChange?: (state: ContinuumState) => void;
    onClientId?: (clientId: string) => void;
    onSessionId?: (sessionId: string) => void;
    onMessage?: (message: Record<string, unknown>) => void;
  }): void {
    this.onStateChange = callbacks.onStateChange;
    this.onClientId = callbacks.onClientId;
    this.onSessionId = callbacks.onSessionId;
    this.onMessage = callbacks.onMessage;
  }

  initializeConnection(): void {
    try {
      this.ws = new WebSocket('ws://localhost:9000');
      
      this.ws.onopen = () => {
        console.log('🔌 WebSocket connected');
        this.onStateChange?.('connected');
        
        // Send client init
        const clientInitData: ClientInitData = {
          userAgent: navigator.userAgent,
          url: window.location.href,
          timestamp: new Date().toISOString(),
          version: this.version,
          mode: 'join_existing'
        };
        
        this.sendMessage({
          type: 'client_init',
          data: clientInitData,
          timestamp: new Date().toISOString()
        });
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event);
      };

      this.ws.onclose = () => {
        console.log('🔌 WebSocket disconnected');
        this.onStateChange?.('error');
      };

      this.ws.onerror = (error) => {
        console.error('🔌 WebSocket error:', error);
        this.onStateChange?.('error');
      };

    } catch (error) {
      console.error('❌ Failed to initialize WebSocket:', error);
      this.onStateChange?.('error');
    }
  }

  private async handleMessage(event: MessageEvent): Promise<void> {
    try {
      const message = JSON.parse(event.data);
      
      if (message.type === 'connection_confirmed') {
        const clientId = message.data?.clientId;
        if (clientId) {
          console.log(`🔌 Client ID: ${clientId}`);
          this.onClientId?.(clientId);
        }
        return;
      }

      if (message.type === 'session_ready') {
        const sessionId = message.data?.sessionId;
        if (sessionId) {
          console.log(`🎯 Session: ${sessionId}`);
          this.onSessionId?.(sessionId);
          this.onStateChange?.('ready');
        }
        return;
      }

      // Handle command responses
      if (message.type === 'execute_command_response') {
        const event = new CustomEvent('continuum:command_response', { 
          detail: message 
        });
        document.dispatchEvent(event);
        return;
      }

      // Handle remote execution requests from server - delegate to command registry
      if (message.type === 'remote_execution_request') {
        await this.delegateRemoteExecution(message.data);
        return;
      }

      // Handle execute_js messages from server
      if (message.type === 'execute_js') {
        console.log('🚀 Executing JavaScript from server:', message.script);
        try {
          // Wrap in async function to handle return statements and promises
          const wrappedScript = `(async function() { ${message.script} })()`;
          const result = await eval(wrappedScript);
          console.log('✅ JavaScript execution result:', result);
        } catch (error) {
          console.error('❌ JavaScript execution error:', error);
        }
        return;
      }

      // Pass other messages to callback
      this.onMessage?.(message);

    } catch (error) {
      console.error('❌ Error parsing message:', error);
    }
  }

  sendMessage(message: WebSocketMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        ...message,
        timestamp: new Date().toISOString()
      }));
    }
  }

  isOpen(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Delegate remote execution to proper command handler - modular approach
   */
  private async delegateRemoteExecution(data: RemoteExecutionRequest): Promise<void> {
    const startTime = Date.now();
    console.log(`🔍 Browser delegating remote execution: ${data.command}`, data);

    try {
      // Use event system for modular command handling
      const event = new CustomEvent('continuum:remote_execution', {
        detail: {
          request: data,
          respond: (response: RemoteExecutionResponse) => {
            this.sendMessage({
              type: 'remote_execution_response',
              data: response
            });
          }
        }
      });

      document.dispatchEvent(event);

      // Fallback if no handler responds within timeout
      setTimeout(() => {
        const errorResponse: RemoteExecutionResponse = {
          success: false,
          error: `No handler registered for command: ${data.command}`,
          requestId: data.requestId,
          clientMetadata: {
            userAgent: navigator.userAgent,
            timestamp: Date.now(),
            executionTime: Date.now() - startTime
          }
        };

        this.sendMessage({
          type: 'remote_execution_response',
          data: errorResponse,
          timestamp: new Date().toISOString()
        });
      }, 1000); // 1 second timeout for handler registration

    } catch (error) {
      console.error(`❌ Remote execution delegation failed: ${error}`);
      
      const response: RemoteExecutionResponse = {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        requestId: data.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };

      this.sendMessage({
        type: 'remote_execution_response',
        data: response
      });
    }
  }
}