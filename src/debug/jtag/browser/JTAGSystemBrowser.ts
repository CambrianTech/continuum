/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system implementation with browser daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '../shared/JTAGSystem';
import type { JTAGContext } from '../shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { SYSTEM_EVENTS } from '../shared/events/SystemEvents';
import type { DaemonBase, DaemonEntry } from '../shared/DaemonBase';
import { BROWSER_DAEMONS } from './structure';
import { generateUUID } from '../shared/CrossPlatformUUID';

export class JTAGSystemBrowser extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return BROWSER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }

  protected override getVersionString(): string {
    // Browser environment - version embedded in build
    return '{VERSION_STRING}-browser';
  }

  private static instance: JTAGSystemBrowser | null = null;

  private constructor(context: JTAGContext, router: JTAGRouter, config?: JTAGSystemConfig) {
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
   * Get or create persistent session UUID for browser
   */
  private static getOrCreateSessionId(): string {
    const STORAGE_KEY = 'jtag_session_id';
    
    if (typeof localStorage !== 'undefined') {
      // Try to get existing session from localStorage
      let sessionId = localStorage.getItem(STORAGE_KEY);
      if (!sessionId) {
        // Create new session UUID
        sessionId = generateUUID();
        localStorage.setItem(STORAGE_KEY, sessionId);
        console.log(`🆔 JTAG Browser: Created new session UUID: ${sessionId}`);
      } else {
        console.log(`🆔 JTAG Browser: Retrieved existing session UUID: ${sessionId}`);
      }
      return sessionId;
    } else {
      // Fallback for environments without localStorage
      const sessionId = generateUUID();
      console.log(`🆔 JTAG Browser: Generated ephemeral session UUID: ${sessionId}`);
      return sessionId;
    }
  }

  /**
   * Connect and auto-wire the browser JTAG system
   */
  static async connect(config?: JTAGSystemConfig): Promise<JTAGSystemBrowser> {
    if (JTAGSystemBrowser.instance) {
      return JTAGSystemBrowser.instance;
    }

    // 1. Get or create persistent session UUID
    const sessionId = JTAGSystemBrowser.getOrCreateSessionId();

    // 2. Create browser context with session UUID
    const context: JTAGContext = {
      uuid: sessionId,
      environment: JTAG_ENVIRONMENTS.BROWSER
    };

    console.log(`🔄 JTAG System: Connecting browser environment...`);
    console.log(`🆔 JTAG System: Using session UUID: ${sessionId}`);

    // 2. Create universal router with config including sessionId
    const routerConfig = {
      ...config?.router,
      sessionId: sessionId  // Pass sessionId for transport handshake
    };
    const router = new JTAGRouter(context, routerConfig);
    
    // Emit initializing event
    router.eventSystem.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing browser environment`);
    
    await router.initialize();

    // 3. Create browser system instance with config
    const system = new JTAGSystemBrowser(context, router, config);
    
    // 4. Setup daemons directly (no delegation needed)
    await system.setupDaemons();

    // 5. Setup cross-context transport
    await system.setupTransports();
    
    // Emit transport ready event
    router.eventSystem.emit(SYSTEM_EVENTS.TRANSPORT_READY, {
      context,
      timestamp: new Date().toISOString(),
      transportType: 'websocket-client'
    });
    console.log(`🔗 JTAG System: Transport ready event emitted`);

    JTAGSystemBrowser.instance = system;
    
    console.log(`✅ JTAG System: Connected browser successfully`);
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