/**
 * ConnectionStatus - Online/Offline State Tracking
 *
 * Tracks whether the browser has a connection to the server.
 * Uses both browser online/offline events AND WebSocket state.
 * Triggers callbacks when connection is restored for sync queue processing.
 *
 * Part of the offline-first dual-storage ORM architecture.
 */

import { Events } from '../../../system/core/shared/Events';

/**
 * ConnectionStatus - Track online/offline state
 *
 * Usage:
 * ```typescript
 * const status = new ConnectionStatus();
 *
 * // Check current state
 * if (status.isOnline) {
 *   await syncToServer();
 * }
 *
 * // Register reconnect callback
 * status.onReconnect(() => {
 *   processSyncQueue();
 * });
 * ```
 */
export class ConnectionStatus {
  private _isOnline: boolean = true;
  private _isWebSocketConnected: boolean = false;
  private reconnectCallbacks: (() => void)[] = [];

  constructor() {
    // Check initial state
    this._isOnline = typeof navigator !== 'undefined' ? navigator.onLine : true;

    // Listen for browser online/offline events
    if (typeof window !== 'undefined') {
      window.addEventListener('online', () => this.handleBrowserOnline());
      window.addEventListener('offline', () => this.handleBrowserOffline());
    }

    // Listen for WebSocket state changes (our own event system)
    Events.subscribe('websocket:connected', () => this.handleWebSocketConnected());
    Events.subscribe('websocket:disconnected', () => this.handleWebSocketDisconnected());
  }

  /**
   * Whether we're currently online (browser + websocket)
   *
   * We consider ourselves online if:
   * 1. Browser reports online AND
   * 2. WebSocket is connected OR we haven't checked WebSocket yet
   */
  get isOnline(): boolean {
    return this._isOnline && (this._isWebSocketConnected || !this._isWebSocketConnected);
  }

  /**
   * Whether the WebSocket is currently connected
   */
  get isWebSocketConnected(): boolean {
    return this._isWebSocketConnected;
  }

  /**
   * Register a callback to be called when connection is restored
   *
   * Multiple callbacks can be registered.
   */
  onReconnect(callback: () => void): void {
    this.reconnectCallbacks.push(callback);
  }

  /**
   * Unregister a reconnect callback
   */
  offReconnect(callback: () => void): void {
    this.reconnectCallbacks = this.reconnectCallbacks.filter(cb => cb !== callback);
  }

  /**
   * Handle browser coming online
   */
  private handleBrowserOnline(): void {
    const wasOffline = !this._isOnline;
    this._isOnline = true;
    console.log('ConnectionStatus: Browser online');

    if (wasOffline) {
      this.triggerReconnectCallbacks();
    }
  }

  /**
   * Handle browser going offline
   */
  private handleBrowserOffline(): void {
    this._isOnline = false;
    console.log('ConnectionStatus: Browser offline');
  }

  /**
   * Handle WebSocket connected
   */
  private handleWebSocketConnected(): void {
    const wasDisconnected = !this._isWebSocketConnected;
    this._isWebSocketConnected = true;
    console.log('ConnectionStatus: WebSocket connected');

    if (wasDisconnected && this._isOnline) {
      this.triggerReconnectCallbacks();
    }
  }

  /**
   * Handle WebSocket disconnected
   */
  private handleWebSocketDisconnected(): void {
    this._isWebSocketConnected = false;
    console.log('ConnectionStatus: WebSocket disconnected');
  }

  /**
   * Trigger all reconnect callbacks
   */
  private triggerReconnectCallbacks(): void {
    console.log(`ConnectionStatus: Triggering ${this.reconnectCallbacks.length} reconnect callbacks`);
    this.reconnectCallbacks.forEach(callback => {
      try {
        callback();
      } catch (error) {
        console.error('ConnectionStatus: Reconnect callback error:', error);
      }
    });
  }

  /**
   * Debug: log current state
   */
  debug(): void {
    console.group('ConnectionStatus Debug');
    console.log('Browser online:', this._isOnline);
    console.log('WebSocket connected:', this._isWebSocketConnected);
    console.log('Effective online:', this.isOnline);
    console.log('Reconnect callbacks:', this.reconnectCallbacks.length);
    console.groupEnd();
  }
}
