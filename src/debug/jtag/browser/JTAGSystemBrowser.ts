/**
 * JTAG System - Browser Implementation
 * 
 * Browser-specific JTAG system implementation with browser daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '@shared/JTAGSystem';
import type { JTAGContext } from '@shared/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '@shared/JTAGTypes';
import { JTAGRouter } from '@shared/JTAGRouter';
import { SYSTEM_EVENTS } from '@systemEvents';
import type { DaemonBase, DaemonEntry } from '@shared/DaemonBase';
import { BROWSER_DAEMONS } from './generated';
import { SYSTEM_SCOPES } from '@shared/SystemScopes';
import { SessionDaemonBrowser } from '@daemonsSessionDaemon/browser/SessionDaemonBrowser';

export class JTAGSystemBrowser extends JTAGSystem {
  protected override get daemonEntries(): DaemonEntry[] { return BROWSER_DAEMONS; }
  
  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouter): DaemonBase | null {
    return new entry.daemonClass(context, router);
  }

  protected override getVersionString(): string {
    // Browser environment - version embedded in build
    return '{VERSION_STRING}-browser';
  }

  /**
   * Initialize session from SessionDaemon and update context UUID
   */
  private async initializeSessionFromDaemon(): Promise<void> {
    const sessionDaemon = this.daemons.find(d => d instanceof SessionDaemonBrowser);
    if (!sessionDaemon) {
      console.warn(`⚠️ ${this.toString()}: No SessionDaemon available - keeping system scope`);
      return;
    }

    try {
      console.log(`🏷️ ${this.toString()}: Getting session ID from SessionDaemon...`);
      
      // Set a shorter timeout for session creation to avoid blocking system initialization
      const sessionPromise = sessionDaemon.getOrCreateSession();
      const timeoutPromise = new Promise<never>((_, reject) => 
        setTimeout(() => reject(new Error('Session creation timeout')), 3000)
      );
      
      const sessionId = await Promise.race([sessionPromise, timeoutPromise]);
      
      // Update context UUID to use session ID from SessionDaemon
      this.context.uuid = sessionId;
      this.sessionId = sessionId;
      
      // Update ConsoleDaemon with the session ID
      this.updateConsoleDaemonSessionId();
      
      console.log(`✅ ${this.toString()}: Session initialized from daemon - ${sessionId}`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Error getting session from daemon:`, error);
      // Keep system scope if session daemon fails
      console.log(`🔄 ${this.toString()}: Falling back to system scope`);
    }
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
   * Connect and auto-wire the browser JTAG system
   */
  static async connect(config?: JTAGSystemConfig): Promise<JTAGSystemBrowser> {
    if (JTAGSystemBrowser.instance) {
      return JTAGSystemBrowser.instance;
    }

    // 1. Start with system scope for initial daemon setup
    const context: JTAGContext = {
      uuid: SYSTEM_SCOPES.SYSTEM, // Use system scope until SessionDaemon provides real session
      environment: JTAG_ENVIRONMENTS.BROWSER
    };

    console.log(`🔄 JTAG System: Connecting browser environment...`);
    console.log(`🆔 JTAG System: Starting with system scope, will get session from SessionDaemon`);

    // 3. Create universal router with config (no sessionId - will receive from server)
    const routerConfig = {
      ...config?.router
    };
    const router = new JTAGRouter(context, routerConfig);
    
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
    await system.initializeSessionFromDaemon();

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
      components: Array.from(system.daemons.keys())
    });
    console.log(`🎉 JTAG System: System ready event emitted`);

    return system;
  }
}