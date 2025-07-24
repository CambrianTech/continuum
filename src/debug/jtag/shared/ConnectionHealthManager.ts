// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Connection Health Manager
 * 
 * Intelligent connection monitoring with exponential backoff reconnection,
 * health scoring, and event-driven coordination for reliable message delivery.
 * 
 * CORE ARCHITECTURE:
 * - Real-time connection state management
 * - Exponential backoff retry strategy
 * - Health scoring with latency and packet loss metrics
 * - Event-driven transport coordination
 * - Configurable ping intervals and timeouts
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Health scoring algorithms and state transitions
 * - Integration tests: Transport reconnection scenarios
 * - Network tests: Partition recovery and failover behavior
 * - Performance tests: High-frequency ping handling
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Health history tracking enables trend analysis
 * - Event system integration provides system-wide health awareness
 * - Configurable parameters support different network environments
 * - State machine prevents race conditions during reconnection
 */

import type { JTAGContext } from './JTAGTypes';
import type { EventsInterface } from './JTAGRouter';
import { TRANSPORT_EVENTS } from '../transports/TransportEvents';
import { SYSTEM_EVENTS } from './events/SystemEvents';
import { JTAG_ENDPOINTS } from './JTAGEndpoints';
import type { TimerHandle } from './CrossPlatformTypes';

export enum ConnectionState {
  DISCONNECTED = 'disconnected',
  CONNECTING = 'connecting', 
  CONNECTED = 'connected',
  RECONNECTING = 'reconnecting',
  FAILED = 'failed'
}

export interface HealthConfig {
  pingInterval: number;        // How often to ping (ms)
  pingTimeout: number;         // Ping response timeout (ms)
  maxReconnectAttempts: number; // Max reconnection attempts
  baseReconnectDelay: number;   // Base delay for exponential backoff (ms)
  maxReconnectDelay: number;    // Max delay cap (ms)
  healthCheckWindow: number;    // Window for calculating health score
}

export interface ConnectionHealth {
  state: ConnectionState;
  isHealthy: boolean;
  score: number;              // 0-100 health score
  latency: number;            // Average latency (ms)
  packetLoss: number;         // Packet loss percentage
  uptime: number;             // Connection uptime (ms)
  lastSeen: number;           // Last successful communication
  reconnectAttempts: number;
}

export class ConnectionHealthManager {
  private context: JTAGContext;
  private eventSystem: EventsInterface;
  private config: HealthConfig;
  private health: ConnectionHealth;
  private transport: any; // Transport interface
  
  private pingTimer?: TimerHandle;
  private reconnectTimer?: TimerHandle;
  private healthHistory: Array<{ timestamp: number; latency: number; success: boolean }> = [];
  private connectionStartTime = 0;

  constructor(
    context: JTAGContext, 
    eventSystem: EventsInterface,
    config: Partial<HealthConfig> = {}
  ) {
    this.context = context;
    this.eventSystem = eventSystem;
    this.config = {
      pingInterval: 30000,      // 30 seconds
      pingTimeout: 5000,        // 5 seconds  
      maxReconnectAttempts: 10,
      baseReconnectDelay: 1000, // 1 second
      maxReconnectDelay: 60000, // 1 minute
      healthCheckWindow: 300000, // 5 minutes
      ...config
    };

    this.health = {
      state: ConnectionState.DISCONNECTED,
      isHealthy: false,
      score: 0,
      latency: 0,
      packetLoss: 0,
      uptime: 0,
      lastSeen: 0,
      reconnectAttempts: 0
    };

    this.setupEventListeners();
  }

  /**
   * Set transport for health monitoring
   */
  setTransport(transport: any): void {
    this.transport = transport;
    console.log(`🔗 HealthManager[${this.context.environment}]: Transport set`);
  }

  /**
   * Start health monitoring
   */
  startMonitoring(): void {
    if (this.pingTimer) return;

    console.log(`💓 HealthManager[${this.context.environment}]: Started health monitoring`);
    
    this.pingTimer = setInterval(() => {
      this.performHealthCheck();
    }, this.config.pingInterval);

    // Initial health check
    this.performHealthCheck();
  }

  /**
   * Stop health monitoring
   */
  stopMonitoring(): void {
    if (this.pingTimer) {
      clearInterval(this.pingTimer);
      this.pingTimer = undefined;
    }
    
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }

    console.log(`⏹️ HealthManager[${this.context.environment}]: Stopped health monitoring`);
  }

  /**
   * Handle connection established
   */
  onConnected(): void {
    console.log(`✅ HealthManager[${this.context.environment}]: Connection established`);
    
    this.health.state = ConnectionState.CONNECTED;
    this.health.reconnectAttempts = 0;
    this.connectionStartTime = Date.now();
    this.health.lastSeen = Date.now();
    
    this.updateHealthScore();
    this.emitHealthUpdate();
  }

  /**
   * Handle connection lost
   */
  onDisconnected(): void {
    console.log(`❌ HealthManager[${this.context.environment}]: Connection lost`);
    
    if (this.health.state === ConnectionState.CONNECTED) {
      this.health.state = ConnectionState.DISCONNECTED;
      this.scheduleReconnection();
    }
    
    this.updateHealthScore();
    this.emitHealthUpdate();
  }

  /**
   * Get current health status
   */
  getHealth(): ConnectionHealth {
    this.updateHealthScore();
    return { ...this.health };
  }

  /**
   * Force reconnection attempt
   */
  forceReconnect(): void {
    console.log(`🔄 HealthManager[${this.context.environment}]: Forcing reconnection`);
    this.health.reconnectAttempts = 0;
    this.scheduleReconnection();
  }

  /**
   * Perform active health check with ping
   */
  private async performHealthCheck(): Promise<void> {
    if (!this.transport || this.health.state !== ConnectionState.CONNECTED) {
      return;
    }

    const pingStart = Date.now();
    const pingId = `ping_${pingStart}_${Math.random().toString(36).substr(2, 6)}`;

    try {
      // Create ping message - route to health daemon  
      const pingMessage = {
        context: this.context,
        origin: `${this.context.environment}/health`,
        endpoint: JTAG_ENDPOINTS.HEALTH.BASE,
        payload: { 
          type: 'ping',
          id: pingId, 
          timestamp: pingStart 
        }
      };

      // Send ping with timeout
      const timeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error('Ping timeout')), this.config.pingTimeout);
      });

      await Promise.race([
        this.transport.send(pingMessage),
        timeoutPromise
      ]);

      // Record successful ping
      const latency = Date.now() - pingStart;
      this.recordHealthCheck(latency, true);
      this.health.lastSeen = Date.now();

      console.log(`💓 HealthManager: Ping successful (${latency}ms)`);

    } catch (error) {
      // Record failed ping
      this.recordHealthCheck(this.config.pingTimeout, false);
      console.warn(`💔 HealthManager: Ping failed:`, error);
      
      // Consider connection lost after multiple failures
      const recentFailures = this.healthHistory
        .filter(h => Date.now() - h.timestamp < 60000) // Last minute
        .filter(h => !h.success).length;
        
      if (recentFailures >= 3) {
        this.onDisconnected();
      }
    }
  }

  /**
   * Record health check result
   */
  private recordHealthCheck(latency: number, success: boolean): void {
    this.healthHistory.push({
      timestamp: Date.now(),
      latency,
      success
    });

    // Keep only recent history
    const cutoff = Date.now() - this.config.healthCheckWindow;
    this.healthHistory = this.healthHistory.filter(h => h.timestamp > cutoff);
  }

  /**
   * Update health score based on recent history
   */
  private updateHealthScore(): void {
    if (this.healthHistory.length === 0) {
      // Fresh connection should be considered healthy until proven otherwise
      this.health.score = this.health.state === ConnectionState.CONNECTED ? 80 : 0;
      this.health.isHealthy = this.health.score > 70;
      return;
    }

    const recent = this.healthHistory.filter(h => 
      Date.now() - h.timestamp < this.config.healthCheckWindow
    );

    if (recent.length === 0) {
      this.health.score = 0;
      this.health.isHealthy = false;
      return;
    }

    // Calculate metrics
    const successCount = recent.filter(h => h.success).length;
    const successRate = (successCount / recent.length) * 100;
    const avgLatency = recent.reduce((sum, h) => sum + h.latency, 0) / recent.length;
    const packetLoss = ((recent.length - successCount) / recent.length) * 100;

    // Calculate uptime
    const uptime = this.connectionStartTime ? Date.now() - this.connectionStartTime : 0;

    // Update health metrics
    this.health.latency = Math.round(avgLatency);
    this.health.packetLoss = Math.round(packetLoss * 100) / 100;
    this.health.uptime = uptime;

    // Calculate composite score (0-100)
    let score = 0;
    score += successRate * 0.4;        // 40% weight on success rate
    score += Math.max(0, 100 - (avgLatency / 10)) * 0.3; // 30% weight on latency
    score += (this.health.state === ConnectionState.CONNECTED ? 30 : 0); // 30% weight on connection state

    this.health.score = Math.round(Math.max(0, Math.min(100, score)));
    this.health.isHealthy = this.health.score > 70 && this.health.state === ConnectionState.CONNECTED;
  }

  /**
   * Schedule reconnection with exponential backoff
   */
  private scheduleReconnection(): void {
    if (this.health.reconnectAttempts >= this.config.maxReconnectAttempts) {
      console.error(`💀 HealthManager[${this.context.environment}]: Max reconnect attempts reached`);
      this.health.state = ConnectionState.FAILED;
      this.emitHealthUpdate();
      return;
    }

    this.health.state = ConnectionState.RECONNECTING;
    this.health.reconnectAttempts++;

    // Calculate exponential backoff delay
    const delay = Math.min(
      this.config.baseReconnectDelay * Math.pow(2, this.health.reconnectAttempts - 1),
      this.config.maxReconnectDelay
    );

    console.log(`🔄 HealthManager[${this.context.environment}]: Reconnecting in ${delay}ms (attempt ${this.health.reconnectAttempts})`);

    this.reconnectTimer = setTimeout(() => {
      this.attemptReconnection();
    }, delay);

    this.emitHealthUpdate();
  }

  /**
   * Attempt to reconnect transport
   */
  private async attemptReconnection(): Promise<void> {
    if (!this.transport) return;

    try {
      console.log(`🔌 HealthManager[${this.context.environment}]: Attempting reconnection...`);
      
      // Attempt to reconnect transport
      await this.transport.reconnect?.();
      
      // Connection will be confirmed via onConnected() callback
      
    } catch (error) {
      console.error(`❌ HealthManager: Reconnection failed:`, error);
      this.scheduleReconnection(); // Try again
    }
  }

  /**
   * Setup event listeners for transport events
   */
  private setupEventListeners(): void {
    this.eventSystem.on(TRANSPORT_EVENTS.CONNECTED, () => {
      this.onConnected();
    });

    this.eventSystem.on(TRANSPORT_EVENTS.DISCONNECTED, () => {
      this.onDisconnected();
    });

    this.eventSystem.on(TRANSPORT_EVENTS.ERROR, (error: any) => {
      console.warn(`⚠️ HealthManager: Transport error:`, error);
      this.recordHealthCheck(this.config.pingTimeout, false);
    });
  }

  /**
   * Emit health update event
   */
  private emitHealthUpdate(): void {
    const health = this.getHealth();
    this.eventSystem.emit(SYSTEM_EVENTS.HEALTH_UPDATE, {
      status: health.isHealthy ? 'healthy' : 'degraded',
      checks: {
        connection: health.isHealthy,
        latency: health.latency < 1000,
        packetLoss: health.packetLoss < 0.1
      },
      timestamp: new Date().toISOString(),
      details: `Score: ${health.score}, Latency: ${health.latency}ms, Loss: ${health.packetLoss}%`
    });
  }
}