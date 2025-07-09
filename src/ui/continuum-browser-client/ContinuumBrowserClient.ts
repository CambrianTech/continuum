/**
 * Continuum Browser Client - Main API Implementation
 * Lifecycle-aware single global API for browser-server communication
 */

import packageJson from '../../../package.json';
import type { ContinuumAPI, ContinuumState, CommandResult } from './types/BrowserClientTypes';
import type { CommandExecuteData } from './types/WebSocketTypes';
import { ConsoleForwarder } from './console/ConsoleForwarder';
import { WebSocketManager } from './connection/WebSocketManager';

export class ContinuumBrowserClient implements ContinuumAPI {
  public readonly version: string;
  public sessionId: string | null = null;
  public clientId: string | null = null;
  
  private _state: ContinuumState = 'initializing';
  private stateCallbacks: ((state: ContinuumState) => void)[] = [];
  private readyCallbacks: (() => void)[] = [];
  private dynamicMethods: Map<string, (...args: unknown[]) => unknown> = new Map();
  
  // Component modules
  private consoleForwarder: ConsoleForwarder;
  private webSocketManager: WebSocketManager;

  constructor() {
    this.version = packageJson.version;
    
    // Initialize component modules
    this.consoleForwarder = new ConsoleForwarder(
      () => this._state,
      () => this.sessionId
    );
    
    this.webSocketManager = new WebSocketManager(this.version);
    
    // Set up module callbacks
    this.setupModuleCallbacks();
    
    console.log(`🌐 Continuum v${this.version} lifecycle starting...`);
    
    // Initialize lifecycle
    this.setState('connecting');
    this.webSocketManager.initializeConnection();
  }

  private setupModuleCallbacks(): void {
    // Console forwarder callback
    this.consoleForwarder.setExecuteCallback((command, params) => {
      return this.execute(command, params);
    });

    // WebSocket manager callbacks
    this.webSocketManager.setCallbacks({
      onStateChange: (state) => this.setState(state),
      onClientId: (clientId) => { this.clientId = clientId; },
      onSessionId: (sessionId) => { this.sessionId = sessionId; },
      onMessage: (message) => this.handleCustomMessage(message)
    });
  }

  get state(): ContinuumState {
    return this._state;
  }

  private setState(newState: ContinuumState): void {
    if (this._state !== newState) {
      console.log(`🔄 Continuum state: ${this._state} → ${newState}`);
      this._state = newState;
      this.stateCallbacks.forEach(callback => callback(newState));
      
      // Handle state-specific logic
      this.handleStateChange(newState);
    }
  }

  private handleStateChange(state: ContinuumState): void {
    switch (state) {
      case 'connected':
        console.log('🔌 Continuum API connected to server');
        break;
      case 'ready':
        this.readyCallbacks.forEach(callback => callback());
        this.consoleForwarder.executeAndFlushConsoleMessageQueue();
        console.log('✅ Continuum API ready for use');
        this.consoleForwarder.performHealthCheck();
        break;
      case 'error':
        console.error('❌ Continuum API in error state');
        break;
    }
  }

  private handleCustomMessage(message: Record<string, unknown>): void {
    // Handle any custom messages that aren't standard protocol
    console.log('📨 Received custom message:', message);
  }

  isConnected(): boolean {
    return this.state === 'ready' && this.webSocketManager.isOpen();
  }

  async execute(command: string, params: Record<string, unknown> = {}): Promise<CommandResult> {
    if (!this.isConnected()) {
      throw new Error(`Continuum not ready (state: ${this.state})`);
    }

    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const timeout = setTimeout(() => {
        reject(new Error(`Command '${command}' timed out`));
      }, 10000);

      // Listen for response
      const responseHandler = (event: Event) => {
        const message = (event as CustomEvent).detail;
        if (message.requestId === requestId) {
          clearTimeout(timeout);
          document.removeEventListener('continuum:command_response', responseHandler);
          
          if (message.success) {
            resolve(message.data || { success: true });
          } else {
            reject(new Error(message.error || 'Command failed'));
          }
        }
      };

      document.addEventListener('continuum:command_response', responseHandler);

      // Send command
      const commandData: CommandExecuteData = {
        command,
        params: JSON.stringify(params),
        requestId,
        sessionId: this.sessionId
      };
      
      this.webSocketManager.sendMessage({
        type: 'execute_command',
        data: commandData,
        timestamp: new Date().toISOString(),
        clientId: this.clientId,
        sessionId: this.sessionId
      });
    });
  }

  // Dynamic method attachment
  attachMethod(name: string, method: (...args: unknown[]) => unknown): void {
    this.dynamicMethods.set(name, method);
    // Attach to the object dynamically
    (this as Record<string, unknown>)[name] = method.bind(this);
    console.log(`🔧 Dynamic method attached: ${name}`);
  }

  hasMethod(name: string): boolean {
    return this.dynamicMethods.has(name) || typeof (this as Record<string, unknown>)[name] === 'function';
  }

  // Lifecycle event handlers
  onStateChange(callback: (state: ContinuumState) => void): void {
    this.stateCallbacks.push(callback);
  }

  onReady(callback: () => void): void {
    if (this.state === 'ready') {
      callback();
    } else {
      this.readyCallbacks.push(callback);
    }
  }
}