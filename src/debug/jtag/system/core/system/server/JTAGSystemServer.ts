/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system implementation with server daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '../shared/JTAGSystem';
import type { JTAGContext } from '../../types/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../../types/JTAGTypes';
import { JTAGRouterServer } from '../../router/server/JTAGRouterServer';
import { SYSTEM_EVENTS } from '../../../events';
import type { DaemonBase, DaemonEntry } from '../../../../daemons/command-daemon/shared/DaemonBase';
import { SERVER_DAEMONS } from '../../../../server/generated';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';

export class JTAGSystemServer extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return SERVER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouterServer): DaemonBase | null {
    // All daemon classes now use consistent (context, router) constructor pattern
    return new entry.daemonClass(context, router);
  }


  protected override getVersionString(): string {
    // Server environment - try to read package.json dynamically
    try {
      const pkg = require('../package.json') as { version: string };
      return `${pkg.version}-server`;
    } catch {
      return this.config.version.fallback;
    }
  }

  public static instance: JTAGSystemServer | null = null;

  private constructor(context: JTAGContext, router: JTAGRouterServer, config?: JTAGSystemConfig) {
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

    // 1. Create server context - use specific session if provided
    const sessionId = config?.connection?.sessionId ?? SYSTEM_SCOPES.SYSTEM;
    const context: JTAGContext = {
      uuid: sessionId,
      environment: JTAG_ENVIRONMENTS.SERVER
    };

    console.log(`🔄 JTAG System: Connecting server environment...`);
    if (config?.connection?.sessionId) {
      console.log(`🆔 JTAG System: Connecting to specific session: ${sessionId}`);
    } else {
      console.log(`🆔 JTAG System: Server starting with system scope, awaiting browser sessionId...`);
    }

    // 2. Create universal router with config and session
    const routerConfig = {
      sessionId: sessionId,
      ...config?.router
    };
    const router = new JTAGRouterServer(context, routerConfig);
    
    // Emit initializing event
    router.eventManager.events.emit(SYSTEM_EVENTS.INITIALIZING, {
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
    
    // 6. Session handling is now done via SessionDaemon through router messages
    
    // Emit transport ready event
    router.eventManager.events.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
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
    router.eventManager.events.emit(SYSTEM_EVENTS.READY, {
      version: '1.0.0',
      context,
      timestamp: new Date().toISOString(),
      components: Array.from(system.daemons.keys())
    });
    console.log(`🎉 JTAG System: System ready event emitted`);

    return system;
  }

}