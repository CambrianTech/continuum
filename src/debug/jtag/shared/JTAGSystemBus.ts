/**
 * JTAG System Bus - Dynamic Command Registration
 * 
 * Base system that any component can extend or register commands with.
 * Makes wiring dead simple - just register your command!
 */

import { jtagRouter } from './JTAGRouter';
import { JTAGConfig } from './JTAGTypes';

export interface CommandDefinition {
  name: string;
  requiresEndpoint?: string; // 'browser', 'server', 'any'
  handler: (params: any) => Promise<any>;
  description?: string;
}

export class JTAGSystemBus {
  protected config: JTAGConfig;
  private connected = false;
  private transportReady = false;
  private messageQueue: any[] = [];
  private commands = new Map<string, CommandDefinition>();
  private instanceUUID: string;
  private isServer: boolean;
  protected isClient: boolean;

  constructor(config: Partial<JTAGConfig> = {}) {
    this.isServer = typeof require !== 'undefined' && typeof window === 'undefined';
    this.isClient = typeof window !== 'undefined';
    this.instanceUUID = this.generateUUID();
    
    this.config = {
      context: this.isServer ? 'server' : 'browser',
      jtagPort: 9001,
      enableRemoteLogging: true,
      enableConsoleOutput: true,
      maxBufferSize: 1000,
      ...config
    } as JTAGConfig;

    this.setupCoreCommands();
  }

  protected generateUUID(): string {
    return 'jtag_' + Date.now().toString(36) + '_' + Math.random().toString(36).substr(2, 9);
  }

  /**
   * Register a command dynamically - EASY WIRING!
   */
  registerCommand(command: CommandDefinition): void {
    this.commands.set(command.name, command);
    
    // Auto-create method on this instance
    (this as any)[command.name] = async (...args: any[]) => {
      return this.executeCommand(command.name, args);
    };
    
    console.log(`🔌 JTAG Bus: Registered command '${command.name}' ${command.requiresEndpoint ? `(requires ${command.requiresEndpoint})` : ''}`);
  }

  /**
   * Execute any registered command
   */
  async executeCommand(commandName: string, params: any[] = []): Promise<any> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Command '${commandName}' not registered`);
    }

    // CRITICAL: Check endpoint requirements
    if (command.requiresEndpoint) {
      await this.connect({ healthCheck: true, endpoint: command.requiresEndpoint });
    }

    console.log(`⚡ JTAG Bus: Executing '${commandName}'`);
    return command.handler(params);
  }

  /**
   * Simple connect method - can be enhanced
   */
  async connect(options: { healthCheck?: boolean; endpoint?: string } = {}): Promise<void> {
    if (this.connected) return;

    console.log(`🔗 JTAG Bus: Connecting to ${options.endpoint || 'default'} endpoint...`);
    
    // Simple connection simulation
    await new Promise(resolve => setTimeout(resolve, 50));
    
    this.connected = true;
    this.transportReady = true;
    
    // Flush any queued messages
    if (this.messageQueue.length > 0) {
      console.log(`🚀 JTAG Bus: Flushing ${this.messageQueue.length} queued messages...`);
      const queued = [...this.messageQueue];
      this.messageQueue = [];
      
      for (const msg of queued) {
        await this.routeMessage(msg);
      }
    }
    
    console.log(`✅ JTAG Bus: Connected to ${options.endpoint || 'default'} endpoint`);
  }

  /**
   * Route message through the universal router
   */
  async routeMessage(message: any): Promise<void> {
    // Server: route immediately
    if (this.isServer || this.transportReady) {
      try {
        await jtagRouter.routeMessage(message);
      } catch (error) {
        console.error('🚨 JTAG Bus: Routing failed:', error);
      }
    } else {
      // Client: queue until ready
      this.messageQueue.push(message);
      console.log(`🔄 JTAG Bus: Queued message, queue size: ${this.messageQueue.length}`);
    }
  }

  /**
   * Setup core commands that come with the bus
   */
  private setupCoreCommands(): void {
    // Core logging command
    this.registerCommand({
      name: 'log',
      requiresEndpoint: this.isClient ? 'server' : undefined,
      handler: async (params: any[]) => {
        const [component, message, data] = params;
        const logMessage = {
          id: this.generateUUID(),
          type: 'log' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
          payload: { level: 'log', component, message, data, timestamp: new Date().toISOString() }
        };
        await this.routeMessage(logMessage);
      },
      description: 'Send log message through router'
    });

    // Core error command
    this.registerCommand({
      name: 'error',
      requiresEndpoint: this.isClient ? 'server' : undefined,
      handler: async (params: any[]) => {
        const [component, message, data] = params;
        const errorMessage = {
          id: this.generateUUID(),
          type: 'log' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
          payload: { level: 'error', component, message, data, timestamp: new Date().toISOString() }
        };
        await this.routeMessage(errorMessage);
      },
      description: 'Send error message through router'
    });
  }

  /**
   * Get all registered commands
   */
  getCommands(): Map<string, CommandDefinition> {
    return new Map(this.commands);
  }

  /**
   * Check if command is registered
   */
  hasCommand(commandName: string): boolean {
    return this.commands.has(commandName);
  }
}

/**
 * JTAG Console Extension - adds console-specific commands
 */
export class JTAGConsole extends JTAGSystemBus {
  constructor(config?: Partial<JTAGConfig>) {
    super(config);
    this.setupConsoleCommands();
  }

  private setupConsoleCommands(): void {
    this.registerCommand({
      name: 'critical',
      requiresEndpoint: this.isClient ? 'server' : undefined,
      handler: async (params: any[]) => {
        const [component, message, data] = params;
        const criticalMessage = {
          id: this.generateUUID(),
          type: 'log' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
          payload: { level: 'critical', component, message, data, timestamp: new Date().toISOString() }
        };
        await this.routeMessage(criticalMessage);
      },
      description: 'Send critical message through router'
    });

    this.registerCommand({
      name: 'probe',
      requiresEndpoint: this.isClient ? 'server' : undefined,
      handler: async (params: any[]) => {
        const [component, metric, value, data] = params;
        const probeMessage = {
          id: this.generateUUID(),
          type: 'log' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
          payload: { level: 'probe', component, message: `${metric}: ${value}`, data, timestamp: new Date().toISOString() }
        };
        await this.routeMessage(probeMessage);
      },
      description: 'Send probe/metric data through router'
    });
  }
}

/**
 * JTAG Screenshot Extension - adds screenshot commands
 */
export class JTAGScreenshot extends JTAGSystemBus {
  constructor(config?: Partial<JTAGConfig>) {
    super(config);
    this.setupScreenshotCommands();
  }

  private setupScreenshotCommands(): void {
    this.registerCommand({
      name: 'screenshot',
      requiresEndpoint: 'browser', // Always needs browser
      handler: async (params: any[]) => {
        const [options] = params;
        console.log('📸 Taking screenshot...');
        
        const screenshotMessage = {
          id: this.generateUUID(),
          type: 'screenshot' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
          payload: { ...options, timestamp: new Date().toISOString() }
        };
        
        await this.routeMessage(screenshotMessage);
        
        return {
          success: true,
          filename: 'screenshot.png',
          timestamp: new Date().toISOString()
        };
      },
      description: 'Take screenshot via browser endpoint'
    });
  }
}