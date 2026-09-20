/**
 * ConnectionMonitor - Independent Connection Health Monitoring
 *
 * Monitors WebSocket connection health independently from UI components.
 * Emits 'connection:status' events with color-coded status indicators.
 *
 * **ARCHITECTURAL REQUIREMENT**: This runs INDEPENDENTLY from widgets/UI.
 * Connection monitoring is a SYSTEM concern, not a UI concern.
 *
 * Color Scheme:
 * - Green (#00cc00): Connected and healthy
 * - Yellow/Orange (#ffaa00): Reconnecting/unstable
 * - Red (#ff0000): Disconnected/failed
 */

import { Events } from '../../shared/Events';
import { getWebSocketTransport } from './JTAGClientBrowser';

export type ConnectionState = 'connected' | 'reconnecting' | 'disconnected';

export interface ConnectionStatus {
  connected: boolean;
  state: ConnectionState;
  color: string;
  timestamp: number;
}

export class ConnectionMonitor {
  private monitorInterval: number | null = null;
  private lastConnectionState: ConnectionState = 'connected';
  private checkIntervalMs: number = 3000; // Check every 3 seconds

  /**
   * Start monitoring connection health
   */
  start(): void {
    if (this.monitorInterval !== null) {
      console.warn('⚠️ ConnectionMonitor: Already running');
      return;
    }

    console.log('🔍 ConnectionMonitor: Starting independent connection monitoring...');

    // Initial check
    this.checkConnection();

    // Schedule periodic checks
    this.monitorInterval = window.setInterval(() => {
      this.checkConnection();
    }, this.checkIntervalMs);

    console.log(`✅ ConnectionMonitor: Monitoring started (checking every ${this.checkIntervalMs}ms)`);
  }

  /**
   * Stop monitoring
   */
  stop(): void {
    if (this.monitorInterval !== null) {
      window.clearInterval(this.monitorInterval);
      this.monitorInterval = null;
      console.log('🛑 ConnectionMonitor: Monitoring stopped');
    }
  }

  /**
   * Check current connection state and emit event if changed
   */
  private checkConnection(): void {
    const transport = getWebSocketTransport();

    if (!transport) {
      this.handleStateChange('disconnected');
      return;
    }

    // Map WebSocket readyState to our connection states
    const state = this.getConnectionState(transport.readyState);

    // Only emit event if state changed
    if (state !== this.lastConnectionState) {
      this.handleStateChange(state);
    }
  }

  /**
   * Map WebSocket readyState to ConnectionState
   */
  private getConnectionState(readyState: number): ConnectionState {
    switch (readyState) {
      case WebSocket.OPEN:
        return 'connected';
      case WebSocket.CONNECTING:
        return 'reconnecting';
      case WebSocket.CLOSING:
      case WebSocket.CLOSED:
      default:
        return 'disconnected';
    }
  }

  /**
   * Get color for connection state
   */
  private getColorForState(state: ConnectionState): string {
    switch (state) {
      case 'connected':
        return '#00cc00';  // Green
      case 'reconnecting':
        return '#ffaa00';  // Yellow/Orange
      case 'disconnected':
        return '#ff0000';  // Red
      default:
        return '#999999';  // Gray (unknown)
    }
  }

  /**
   * Handle state change and emit event
   */
  private handleStateChange(newState: ConnectionState): void {
    const oldState = this.lastConnectionState;
    this.lastConnectionState = newState;

    const status: ConnectionStatus = {
      connected: newState === 'connected',
      state: newState,
      color: this.getColorForState(newState),
      timestamp: Date.now()
    };

    // Emit event
    Events.emit('connection:status', status);

    // Log state transition
    console.log(`🔄 ConnectionMonitor: ${oldState} → ${newState} (${status.color})`);
  }

  /**
   * Force immediate check (useful for testing)
   */
  forceCheck(): void {
    this.checkConnection();
  }

  /**
   * Get current connection state
   */
  getCurrentState(): ConnectionState {
    return this.lastConnectionState;
  }
}

// Create singleton instance
let monitorInstance: ConnectionMonitor | null = null;

/**
 * Get singleton ConnectionMonitor instance
 */
export function getConnectionMonitor(): ConnectionMonitor {
  if (!monitorInstance) {
    monitorInstance = new ConnectionMonitor();
  }
  return monitorInstance;
}

/**
 * Start connection monitoring (called from JTAGClientBrowser initialization)
 */
export function startConnectionMonitoring(): void {
  const monitor = getConnectionMonitor();
  monitor.start();
}

/**
 * Stop connection monitoring (for cleanup)
 */
export function stopConnectionMonitoring(): void {
  if (monitorInstance) {
    monitorInstance.stop();
  }
}
