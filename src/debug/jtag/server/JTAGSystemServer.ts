/**
 * JTAG System - Server Implementation
 * 
 * Server-specific JTAG system implementation with server daemons and transports.
 */

import { JTAGSystem } from '../shared/JTAGSystem';
import { JTAGContext, JTAGEnvironment } from '../shared/JTAGTypes';
import { JTAGRouter } from '../shared/JTAGRouter';
import { JTAGServer } from './JTAGServer';

export class JTAGSystemServer extends JTAGSystem {
  private static instance: JTAGSystemServer | null = null;

  private constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Connect and auto-wire the server JTAG system
   */
  static async connect(): Promise<JTAGSystemServer> {
    if (JTAGSystemServer.instance) {
      return JTAGSystemServer.instance;
    }

    // 1. Create server context
    const context: JTAGContext = {
      uuid: `jtag_server_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      environment: 'server' as JTAGEnvironment
    };

    console.log(`🔄 JTAG System: Connecting server environment...`);

    // 2. Create universal router
    const router = new JTAGRouter(context);
    await router.initialize();

    // 3. Create server system instance
    const system = new JTAGSystemServer(context, router);
    
    // 4. Use existing JTAGServer for daemon setup
    const jtagServer = new JTAGServer(context, router);
    await jtagServer.setupDaemons();
    
    // Copy daemons from JTAGServer
    system.daemons = jtagServer.daemons;

    // 5. Setup cross-context transport
    await system.setupTransports();

    JTAGSystemServer.instance = system;
    
    console.log(`✅ JTAG System: Connected server successfully`);
    console.log(`   Context UUID: ${context.uuid}`);
    console.log(`   Daemons: ${Array.from(system.daemons.keys()).join(', ')}`);

    return system;
  }

  /**
   * Setup server-specific transports
   */
  async setupTransports(): Promise<void> {
    await this.router.setupCrossContextTransport();
    console.log(`🔗 JTAG System: Cross-context transport configured`);
  }

  /**
   * Setup server daemons - delegated to JTAGServer
   */
  async setupDaemons(): Promise<void> {
    // Handled in connect() method via JTAGServer
  }
}