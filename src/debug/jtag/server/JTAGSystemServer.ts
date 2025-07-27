/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system implementation with server daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '../shared/JTAGSystem';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SYSTEM_EVENTS } from '../shared/events/SystemEvents';
import type { DaemonBase, DaemonEntry } from '../shared/DaemonBase';
import { SERVER_DAEMONS } from './structure';
import { SYSTEM_SCOPES } from '../shared/SystemScopes';
import { WebSocketServerTransport } from '../transports/WebSocketTransport';

export class JTAGSystemServer extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return SERVER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }

  /**
   * Handle session handshake from browser - adopt browser's sessionId
   */
  private handleSessionHandshake(browserSessionId: string): void {
    console.log(`🤝 JTAG Server: Adopting browser sessionId: ${browserSessionId}`);
    
    // Update our own sessionId to match browser's
    this.sessionId = browserSessionId;
    
    // CRITICAL: Update the context UUID to match browser's sessionId
    // This ensures all future operations use the shared sessionId
    (this.context as any).uuid = browserSessionId;
    
    // Update console daemon with the shared sessionId
    this.updateConsoleDaemonSessionId();
    
    console.log(`✅ JTAG Server: Session coordination complete - using shared sessionId: ${browserSessionId}`);
  }

  /**
   * Setup session handshake handling with WebSocket transport
   */
  private setupSessionHandshake(): void {
    const transport = this.router.crossContextTransport;
    if (transport && transport instanceof WebSocketServerTransport) {
      transport.setSessionHandshakeHandler((sessionId: string) => {
        this.handleSessionHandshake(sessionId);
      });
      console.log(`🤝 JTAG Server: Session handshake handler configured`);
    } else {
      console.warn(`⚠️ JTAG Server: Transport is not WebSocketServerTransport, session handshake not available`);
    }
  }

  protected override getVersionString(): string {
    // Server environment - try to read package.json dynamically
    try {
      const pkg = require('../package.json') as { version: string };
      return `${pkg.version}-server`;
    } catch (error) {
      return this.config.version.fallback;
    }
  }

  private static instance: JTAGSystemServer | null = null;

  private constructor(context: JTAGContext, router: JTAGRouter, config?: JTAGSystemConfig) {
    super(context, router, {
      version: {
        fallback: 'unknown-server-version',
        enableLogging: true,
        ...config?.version
      },
      daemons: {
        enableParallelInit: true,
        initTimeout: 15000, // Server timeout longer
        ...config?.daemons
      },
      router: {
        queue: {
          maxSize: 2000, // Server - larger queue for handling more traffic
          flushInterval: 1000, // Server - less frequent flushing for efficiency
          ...config?.router?.queue
        },
        health: {
          healthCheckInterval: 45000, // Server - less frequent health checks
          connectionTimeout: 15000, // Server - longer timeout
          ...config?.router?.health
        },
        response: {
          correlationTimeout: 60000, // Server - longer response timeout
          ...config?.router?.response
        },
        ...config?.router
      }
    });
  }

  /**
   * Connect and auto-wire the server JTAG system
   */
  static async connect(config?: JTAGSystemConfig): Promise<JTAGSystemServer> {
    if (JTAGSystemServer.instance) {
      return JTAGSystemServer.instance;
    }

    // 1. Create server context using SYSTEM scope initially (before browser handshake)
    const context: JTAGContext = {
      uuid: SYSTEM_SCOPES.SYSTEM, // Use system scope until browser handshake
      environment: JTAG_ENVIRONMENTS.SERVER
    };

    console.log(`🔄 JTAG System: Connecting server environment...`);
    console.log(`🆔 JTAG System: Server starting with system scope, awaiting browser sessionId...`);

    // 2. Create universal router with config
    const routerConfig = config?.router ?? {};
    const router = new JTAGRouter(context, routerConfig);
    
    // Emit initializing event
    router.eventSystem.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing server environment`);
    
    await router.initialize();

    // 3. Create server system instance with config
    const system = new JTAGSystemServer(context, router, config);
    
    // 4. Setup daemons directly (no delegation needed)
    await system.setupDaemons();

    // 5. Setup cross-context transport
    await system.setupTransports();
    
    // 6. Setup session handshake handling
    system.setupSessionHandshake();
    
    // Emit transport ready event
    router.eventSystem.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
      context,
      timestamp: new Date().toISOString(),
      transportType: 'websocket-server'
    });
    console.log(`🔗 JTAG System: Transport ready event emitted`);

    JTAGSystemServer.instance = system;
    
    console.log(`✅ JTAG System: Connected server successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

    // Emit system ready event after full initialization
    router.eventSystem.emit(SYSTEM_EVENTS.READY, {
      version: '1.0.0',
      context,
      timestamp: new Date().toISOString(),
      components: Array.from(system.daemons.keys())
    });
    console.log(`🎉 JTAG System: System ready event emitted`);

    return system;
  }

}