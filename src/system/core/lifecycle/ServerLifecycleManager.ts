/**
 * Server Lifecycle Manager - Event-Driven Server State Management
 * 
 * Replaces polling-based port checking with proper event-driven architecture:
 * - WebSocket-based server readiness signaling
 * - Type-safe lifecycle state management
 * - Graceful degradation for network issues
 * - No timeout-based polling loops
 */

import { EventEmitter } from 'events';
import { WebSocket } from 'ws';
import { createServer, Server } from 'http';

interface ServerState {
  readonly websocketPort: number;
  readonly httpPort: number;
  readonly state: 'starting' | 'ready' | 'error' | 'stopped';
  readonly startedAt?: Date;
  readonly error?: string;
}

interface LifecycleEvents {
  'server-starting': (state: ServerState) => void;
  'server-ready': (state: ServerState) => void;  
  'server-error': (state: ServerState) => void;
  'server-stopped': (state: ServerState) => void;
}

export class ServerLifecycleManager extends EventEmitter {
  private currentState: ServerState | null = null;
  private readinessClient: WebSocket | null = null;
  private healthServer: Server | null = null;

  constructor() {
    super();
  }

  /**
   * Check server readiness through event-driven connection instead of polling
   */
  async checkServerReady(websocketPort: number, httpPort: number): Promise<ServerState> {
    console.log(`🔍 Checking server readiness: WebSocket=${websocketPort}, HTTP=${httpPort}`);

    // Try WebSocket connection for immediate readiness signal
    try {
      const state = await this.connectToReadinessSignal(websocketPort, httpPort);
      return state;
    } catch (error) {
      // Fallback to single HTTP health check (no polling loop)
      return await this.fallbackHealthCheck(websocketPort, httpPort);
    }
  }

  /**
   * Connect to server's readiness signaling system via WebSocket
   * No timeouts - either connects immediately or fails fast
   */
  private connectToReadinessSignal(websocketPort: number, httpPort: number): Promise<ServerState> {
    return new Promise((resolve, reject) => {
      try {
        // Connect to server's readiness WebSocket endpoint
        const ws = new WebSocket(`ws://localhost:${websocketPort}/system/readiness`);
        
        ws.on('open', () => {
          console.log(`✅ Connected to readiness signal on port ${websocketPort}`);
          // Request immediate status
          ws.send(JSON.stringify({ type: 'status-request' }));
        });

        ws.on('message', (data) => {
          try {
            const message = JSON.parse(data.toString());
            
            if (message.type === 'server-status') {
              const state: ServerState = {
                websocketPort,
                httpPort,
                state: message.ready ? 'ready' : 'starting',
                startedAt: message.startedAt ? new Date(message.startedAt) : undefined
              };
              
              this.currentState = state;
              this.emit('server-ready', state);
              ws.close();
              resolve(state);
            }
          } catch (parseError) {
            reject(new Error(`Invalid readiness message: ${parseError}`));
          }
        });

        ws.on('error', (error) => {
          reject(new Error(`WebSocket readiness check failed: ${error.message}`));
        });

        ws.on('close', (code, reason) => {
          if (code !== 1000) { // 1000 = normal closure
            reject(new Error(`WebSocket closed unexpectedly: ${code} ${reason}`));
          }
        });

      } catch (error) {
        reject(error);
      }
    });
  }

  /**
   * Fallback single HTTP health check - no polling, no timeouts
   * Either responds immediately or fails fast
   */
  private async fallbackHealthCheck(websocketPort: number, httpPort: number): Promise<ServerState> {
    console.log(`🔄 Fallback: Single HTTP health check on port ${httpPort}`);

    return new Promise((resolve) => {
      try {
        const http = require('http');
        
        // Single request - no retry loops, no timeouts
        const req = http.request({
          hostname: 'localhost',
          port: httpPort,
          path: '/health',
          method: 'GET',
          headers: { 'Connection': 'close' } // Ensure immediate close
        }, (res: any) => {
          let body = '';
          res.on('data', (chunk: any) => body += chunk);
          
          res.on('end', () => {
            const isHealthy = res.statusCode >= 200 && res.statusCode < 300;
            
            const state: ServerState = {
              websocketPort,
              httpPort,
              state: isHealthy ? 'ready' : 'error',
              startedAt: isHealthy ? new Date() : undefined,
              error: isHealthy ? undefined : `HTTP ${res.statusCode}: ${body}`
            };
            
            this.currentState = state;
            
            if (isHealthy) {
              this.emit('server-ready', state);
            } else {
              this.emit('server-error', state);
            }
            
            resolve(state);
          });
        });

        req.on('error', (error: Error) => {
          const errorState: ServerState = {
            websocketPort,
            httpPort,
            state: 'error',
            error: `HTTP request failed: ${error.message}`
          };
          
          this.currentState = errorState;
          this.emit('server-error', errorState);
          resolve(errorState);
        });

        // Send request
        req.end();

      } catch (error) {
        const errorState: ServerState = {
          websocketPort,
          httpPort,
          state: 'error',
          error: error instanceof Error ? error.message : String(error)
        };
        
        this.currentState = errorState;
        this.emit('server-error', errorState);
        resolve(errorState);
      }
    });
  }

  /**
   * Get current server state without any network calls
   */
  getCurrentState(): ServerState | null {
    return this.currentState;
  }

  /**
   * Wait for server ready event with proper TypeScript typing
   * Uses event-driven approach instead of polling
   */
  waitForReady(): Promise<ServerState> {
    return new Promise((resolve) => {
      if (this.currentState?.state === 'ready') {
        resolve(this.currentState);
        return;
      }

      const onReady = (state: ServerState) => {
        this.removeListener('server-error', onError);
        resolve(state);
      };

      const onError = (state: ServerState) => {
        this.removeListener('server-ready', onReady);
        resolve(state); // Return error state instead of rejecting
      };

      this.once('server-ready', onReady);
      this.once('server-error', onError);
    });
  }

  /**
   * Clean shutdown of lifecycle manager with proper error handling
   */
  async shutdown(): Promise<void> {
    console.log('🧹 Shutting down ServerLifecycleManager...');

    const shutdownPromises: Promise<void>[] = [];

    // Close WebSocket client gracefully
    if (this.readinessClient) {
      shutdownPromises.push(new Promise<void>((resolve) => {
        try {
          this.readinessClient!.close();
          this.readinessClient!.on('close', () => resolve());
          this.readinessClient!.on('error', () => resolve()); // Handle errors during close
          
          // Force close after 1 second if graceful close fails
          setTimeout(() => {
            if (this.readinessClient && this.readinessClient.readyState !== WebSocket.CLOSED) {
              this.readinessClient.terminate();
            }
            resolve();
          }, 1000);
        } catch (error) {
          console.error('❌ Error closing WebSocket client:', error instanceof Error ? error.message : error);
          resolve();
        } finally {
          this.readinessClient = null;
        }
      }));
    }

    // Close HTTP server gracefully  
    if (this.healthServer) {
      shutdownPromises.push(new Promise<void>((resolve) => {
        try {
          this.healthServer!.close((error) => {
            if (error) {
              console.error('❌ Error closing HTTP server:', error.message);
            }
            resolve();
          });
          
          // Force close after 1 second
          setTimeout(() => resolve(), 1000);
        } catch (error) {
          console.error('❌ Error during HTTP server close:', error instanceof Error ? error.message : error);
          resolve();
        } finally {
          this.healthServer = null;
        }
      }));
    }

    try {
      // Wait for all shutdown operations to complete
      await Promise.all(shutdownPromises);
    } catch (error) {
      console.error('❌ Error during lifecycle manager shutdown:', error instanceof Error ? error.message : error);
    } finally {
      // Always clean up state, even if shutdown operations failed
      try {
        this.removeAllListeners();
      } catch (error) {
        console.error('❌ Error removing listeners:', error instanceof Error ? error.message : error);
      }
      
      this.currentState = null;
      console.log('✅ ServerLifecycleManager shutdown complete');
    }
  }
}

// Export typed event emitter interface
export interface ServerLifecycleManager extends EventEmitter {
  on<K extends keyof LifecycleEvents>(event: K, listener: LifecycleEvents[K]): this;
  emit<K extends keyof LifecycleEvents>(event: K, ...args: Parameters<LifecycleEvents[K]>): boolean;
}