/**
 * WebSocket Manager - Handles WebSocket connection and messaging
 * Manages connection lifecycle and message handling
 */

import { WebSocketMessage, ClientInitData } from '../types/WebSocketTypes';
import { ContinuumState } from '../types/BrowserClientTypes';

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

  private handleMessage(event: MessageEvent): void {
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
}