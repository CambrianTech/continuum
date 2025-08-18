/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system implementation with browser daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '../shared/JTAGSystem';
import type { JTAGContext } from '../../types/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../../types/JTAGTypes';
import { JTAGRouterDynamicBrowser } from '../../router/browser/JTAGRouterDynamicBrowser';
import { SYSTEM_EVENTS } from '../../../events';
import type { DaemonBase, DaemonEntry } from '../../../../daemons/command-daemon/shared/DaemonBase';
import { BROWSER_DAEMONS } from '../../../../browser/generated';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import type { UUID } from '../../types/CrossPlatformUUID';
import { generateUUID } from '../../types/CrossPlatformUUID';

export class JTAGSystemBrowser extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return BROWSER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouterDynamicBrowser): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }

  protected override getVersionString(): string {
    // Browser environment - version embedded in build
    return '{VERSION_STRING}-browser';
  }


  public static instance: JTAGSystemBrowser | null = null;

  private constructor(context: JTAGContext, router: JTAGRouterDynamicBrowser, config?: JTAGSystemConfig) {
    super(context, router, {
      version: {
        fallback: '{VERSION_STRING}-browser',
        enableLogging: true,
        ...config?.version
      },
      daemons: {
        enableParallelInit: true,
        initTimeout: 5000, // Browser timeout shorter
        ...config?.daemons
      },
      router: {
        sessionId: SYSTEM_SCOPES.SYSTEM, // Router already created with proper sessionId
        queue: {
          flushInterval: 300, // Browser - faster flush for UI responsiveness
          maxSize: 500, // Browser - smaller queue for memory efficiency
          ...config?.router?.queue
        },
        health: {
          healthCheckInterval: 20000, // Browser - more frequent health checks
          connectionTimeout: 5000, // Browser - shorter timeout
          ...config?.router?.health
        },
        ...config?.router
      }
    });
  }

  /**
   * Connect and auto-wire the browser JTAG system
   */
  static async connect(config?: JTAGSystemConfig): Promise<JTAGSystemBrowser> {
    if (JTAGSystemBrowser.instance) {
      return JTAGSystemBrowser.instance;
    }

    // 1. Create browser context - generate unique context ID (NOT sessionId)
    // Import the context creation utility and use it here
    const { createClientContext } = await import('../../context/SecureJTAGContext');
    const context: JTAGContext = createClientContext();

    console.log(`🔄 JTAG System: Connecting browser environment...`);
    console.log(`🆔 JTAG System: Starting with system scope, will get session from SessionDaemon`);

    // 3. Create universal router with config and system session (initial)
    const sessionId = config?.connection?.sessionId ?? SYSTEM_SCOPES.SYSTEM;
    const routerConfig = {
      sessionId: sessionId,
      ...config?.router,
      transport: {
        ...config?.router?.transport,
        role: 'client' as const // Browser router always connects as client to server
      }
    };
    const router = new JTAGRouterDynamicBrowser(context, routerConfig);
    
    // Emit initializing event
    router.eventManager.events.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing browser environment`);
    
    await router.initialize();

    // 3. Create browser system instance with config
    const system = new JTAGSystemBrowser(context, router, config);
    
    // 4. Setup daemons directly (no delegation needed)
    await system.setupDaemons();

    // 5. Get session ID from SessionDaemon and update context
    // Session management handled by JTAGClient, not JTAGSystem

    // 6. Setup cross-context transport
    await system.setupTransports();
    
    // Emit transport ready event
    router.eventManager.events.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
      context,
      timestamp: new Date().toISOString(),
      transportType: 'websocket-client'
    });
    console.log(`🔗 JTAG System: Transport ready event emitted`);

    JTAGSystemBrowser.instance = system;
    
    console.log(`✅ JTAG System: Connected browser successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${system.daemons.map(d => d.name).join(', ')}`);

    // Emit system ready event after full initialization
    router.eventManager.events.emit(SYSTEM_EVENTS.READY, {
      version: '1.0.0',
      context,
      timestamp: new Date().toISOString(),
      components: system.daemons.map(d => d.name)
    });
    console.log(`🎉 JTAG System: System ready event emitted`);

    return system;
  }
}