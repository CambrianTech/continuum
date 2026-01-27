/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system implementation with server daemons and transports.
 * Follows the symmetric daemon architecture pattern established in CommandDaemon.
 */

import { JTAGSystem, type JTAGSystemConfig } from '../shared/JTAGSystem';
import type { JTAGContext } from '../../types/JTAGTypes';
import { JTAG_ENVIRONMENTS } from '../../types/JTAGTypes';
import { JTAGRouterDynamicServer } from '../../router/server/JTAGRouterDynamicServer';
import { SYSTEM_EVENTS } from '../../../events';
import type { DaemonBase, DaemonEntry } from '../../../../daemons/command-daemon/shared/DaemonBase';
import { SERVER_DAEMONS } from '../../../../server/generated';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import { generateUUID } from '../../types/CrossPlatformUUID';
import { CommandRouterServer } from '@shared/ipc/archive-worker/CommandRouterServer';
import { startVoiceServer, getVoiceWebSocketServer } from '../../../voice/server';

export class JTAGSystemServer extends JTAGSystem {
  private commandRouter: CommandRouterServer | null = null;
  private voiceServerStarted: boolean = false;

  protected override get daemonEntries(): DaemonEntry[] { return SERVER_DAEMONS; }

  protected override createDaemon(entry: DaemonEntry, context: JTAGContext, router: JTAGRouterDynamicServer): DaemonBase | null {
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

  private constructor(context: JTAGContext, router: JTAGRouterDynamicServer, config?: JTAGSystemConfig) {
    super(context, router, {
      version: {
        fallback: 'unknown-server-version',
        enableLogging: true,
        ...config?.version
      },
      daemons: {
        enableParallelInit: true,
        initTimeout: 5000, // Server timeout longer
        ...config?.daemons
      },
      router: {
        sessionId: SYSTEM_SCOPES.SYSTEM, // Router already created with proper sessionId
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
          correlationTimeout: 5000, // Server - longer response timeout
          ...config?.router?.response
        },
        ...config?.router
      }
    });
  }

  /**
   * Register this JTAG system process in the ProcessRegistry
   * This prevents false positives during cleanup operations
   */
  private async registerSystemProcess(): Promise<void> {
    try {
      // Create ProcessRegistryServerCommand directly (same pattern as cleanup script)
      const { ProcessRegistryServerCommand } = await import('../../../../commands/process-registry/server/ProcessRegistryServerCommand');
      
      // Create mock commander for direct command usage
      const mockCommander = {
        subpath: 'system-registration',
        router: null as any,
        commands: new Map()
      };

      // Create the process registry server command
      const processRegistryCommand = new ProcessRegistryServerCommand(this.context, 'process-registry', mockCommander);

      // Register this process 
      const result = await processRegistryCommand.registerProcess({
        context: this.context,
        sessionId: 'system-registration' as any,
        processType: 'server',
        description: `JTAG System Server (${this.context.uuid})`,
        capabilities: [
          'websocket-server',
          'command-execution', 
          'file-operations',
          'console-logging',
          'screenshot',
          'browser-automation'
        ],
        ports: [] // Will be auto-detected by command
      });

      if (result.success) {
        console.log(`🏷️  JTAG System: Process registered in registry as ${result.processId}`);
      } else {
        console.warn(`⚠️  JTAG System: Process registration failed - cleanup may have false positives`);
        console.warn(`   Error: ${result.error}`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.warn(`⚠️  JTAG System: Process registration error: ${errorMsg}`);
    }
  }

  /**
   * Connect and auto-wire the server JTAG system
   */
  static async connect(config?: JTAGSystemConfig): Promise<JTAGSystemServer> {
    if (JTAGSystemServer.instance) {
      return JTAGSystemServer.instance;
    }

    // 1. Create server context with configuration
    const sessionId = config?.connection?.sessionId ?? SYSTEM_SCOPES.SYSTEM;
    
    // Load configuration and create context
    const { createJTAGConfig } = await import('../../../shared/BrowserSafeConfig');
    const { createServerContext } = await import('../../context/SecureJTAGContext');
    
    const jtagConfig = createJTAGConfig();
    const context: JTAGContext = createServerContext(jtagConfig);

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
    const router = new JTAGRouterDynamicServer(context, routerConfig);
    
    // Emit initializing event
    router.eventManager.events.emit(SYSTEM_EVENTS.INITIALIZING, {
      context,
      timestamp: new Date().toISOString()
    });
    console.log(`🎬 JTAG System: Initializing server environment`);
    
    await router.initialize();

    // 3. Create server system instance with config
    const system = new JTAGSystemServer(context, router, config);

    // 3.5. Set static instance BEFORE daemon setup so PersonaUsers can use local router
    JTAGSystemServer.instance = system;
    console.log(`✅ JTAGSystemServer.instance SET at ${new Date().toISOString()}`);

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

    // 7. Start CommandRouterServer for Rust worker bidirectional communication
    try {
      system.commandRouter = new CommandRouterServer('/tmp/jtag-command-router.sock');
      await system.commandRouter.start();
      console.log(`🦀 JTAG System: Command Router ready for Rust workers`);
    } catch (error) {
      console.warn(`⚠️  JTAG System: Command Router failed to start (Rust workers will not work):`, error);
    }

    // 7.5. Start Voice WebSocket Server
    try {
      await startVoiceServer();
      system.voiceServerStarted = true;
      console.log(`🎙️  JTAG System: Voice WebSocket Server started`);
    } catch (error) {
      console.warn(`⚠️  JTAG System: Voice Server failed to start:`, error);
    }

    // 8. Register this process in the ProcessRegistry to prevent cleanup false positives
    await system.registerSystemProcess();
    
    console.log(`✅ JTAG System: Connected server successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

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

  /**
   * Override shutdown to cleanup CommandRouterServer
   */
  override async shutdown(): Promise<void> {
    console.log(`🔄 JTAG System Server: Shutting down...`);

    // Stop Voice WebSocket Server
    if (this.voiceServerStarted) {
      try {
        const voiceServer = getVoiceWebSocketServer();
        if (voiceServer) {
          await voiceServer.stop();
          console.log(`🎙️  JTAG System Server: Voice Server stopped`);
        }
      } catch (error) {
        console.warn(`⚠️  JTAG System Server: Error stopping Voice Server:`, error);
      }
    }

    // Stop CommandRouterServer
    if (this.commandRouter) {
      try {
        await this.commandRouter.stop();
        console.log(`🦀 JTAG System Server: Command Router stopped`);
      } catch (error) {
        console.warn(`⚠️  JTAG System Server: Error stopping Command Router:`, error);
      }
    }

    // Call base shutdown
    await super.shutdown();
  }

}