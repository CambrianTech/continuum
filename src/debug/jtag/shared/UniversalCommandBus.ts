/**
 * Universal Command Bus - Unified System for All Commands
 * 
 * JTAG console/screenshot + Continuum widgets/commands all on same bus
 * with promise chaining and cross-system communication
 */

import { jtagRouter } from './JTAGRouter';
import { JTAGConfig, JTAGMessageFactory } from './JTAGTypes';

export interface CommandDefinition {
  name: string;
  namespace?: string; // 'jtag', 'continuum', 'widget', etc.
  requiresEndpoint?: string[];
  handler: (params: any, context: CommandContext) => Promise<any>;
  description?: string;
  chainable?: boolean; // Can be used in promise chains
}

export interface CommandContext {
  source: 'browser' | 'server';
  instanceId: string;
  requestId: string;
  chainData?: any; // Data from previous command in chain
}

export class UniversalCommandBus {
  private commands = new Map<string, CommandDefinition>();
  private activeConnections = new Set<string>(); // Track healthy endpoints
  private config: JTAGConfig;
  private instanceId: string;
  private isServer: boolean;
  private isClient: boolean;

  constructor(config: Partial<JTAGConfig> = {}) {
    this.isServer = typeof require !== 'undefined' && typeof window === 'undefined';
    this.isClient = typeof window !== 'undefined';
    this.instanceId = this.generateId();
    
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

  private generateId(): string {
    return 'cmd_' + Date.now().toString(36) + '_' + Math.random().toString(36).substr(2, 9);
  }

  /**
   * Register command from any system - JTAG, Continuum, Widgets, etc.
   */
  registerCommand(command: CommandDefinition): void {
    const fullName = command.namespace ? `${command.namespace}.${command.name}` : command.name;
    this.commands.set(fullName, command);
    
    // Auto-create chainable method
    this.createCommandMethod(fullName, command);
    
    console.log(`🔌 Universal Bus: Registered '${fullName}' ${command.requiresEndpoint ? `(needs ${command.requiresEndpoint.join(', ')})` : ''}`);
  }

  /**
   * Create dynamic method that can be chained
   */
  private createCommandMethod(commandName: string, command: CommandDefinition): void {
    const [namespace, method] = commandName.includes('.') ? commandName.split('.') : [null, commandName];
    
    // Create namespace object if needed
    if (namespace && !(this as any)[namespace]) {
      (this as any)[namespace] = {};
    }
    
    const target = namespace ? (this as any)[namespace] : this;
    
    // Create the command method
    target[method] = async (...args: any[]) => {
      return this.execute(commandName, args);
    };
  }

  /**
   * Execute command with full endpoint checking and promise chaining
   */
  async execute(commandName: string, params: any[] = [], chainData?: any): Promise<any> {
    const command = this.commands.get(commandName);
    if (!command) {
      throw new Error(`Command '${commandName}' not registered`);
    }

    // CRITICAL: Verify all required endpoints are healthy
    if (command.requiresEndpoint) {
      await this.ensureEndpoints(command.requiresEndpoint);
    }

    const context: CommandContext = {
      source: this.config.context as 'browser' | 'server',
      instanceId: this.instanceId,
      requestId: this.generateId(),
      chainData
    };

    console.log(`⚡ Universal Bus: Executing '${commandName}'${chainData ? ' (chained)' : ''}`);
    
    try {
      const result = await command.handler(params, context);
      
      // Enable promise chaining
      if (command.chainable && result) {
        result._chain = (nextCommand: string, ...nextParams: any[]) => {
          return this.execute(nextCommand, nextParams, result);
        };
      }
      
      return result;
    } catch (error) {
      console.error(`🚨 Universal Bus: Command '${commandName}' failed:`, error);
      throw error;
    }
  }

  /**
   * Ensure required endpoints are healthy
   */
  private async ensureEndpoints(endpoints: string[]): Promise<void> {
    for (const endpoint of endpoints) {
      if (!this.activeConnections.has(endpoint)) {
        console.log(`🔗 Universal Bus: Connecting to '${endpoint}' endpoint...`);
        
        // Simple health check simulation
        await new Promise(resolve => setTimeout(resolve, 50));
        
        this.activeConnections.add(endpoint);
        console.log(`✅ Universal Bus: Connected to '${endpoint}'`);
      }
    }
  }

  /**
   * Setup core JTAG commands
   */
  private setupCoreCommands(): void {
    // JTAG Console Commands
    this.registerCommand({
      name: 'log',
      namespace: 'jtag',
      requiresEndpoint: this.isClient ? ['server'] : [],
      handler: async (params, context) => {
        const [component, message, data] = params;
        const logMessage = JTAGMessageFactory.createEvent('log', context.source, {
          level: 'log' as const,
          component,
          message,
          data
        });
        await jtagRouter.routeMessage(logMessage);
        return { logged: true, component, message };
      },
      chainable: true,
      description: 'JTAG logging command'
    });

    this.registerCommand({
      name: 'screenshot',
      namespace: 'jtag',
      requiresEndpoint: ['browser'],
      handler: async (params, context) => {
        const [options = {}] = params;
        console.log('📸 JTAG: Taking screenshot...');
        
        const screenshotMessage = {
          id: context.requestId,
          type: 'screenshot' as const,
          version: '1.0.0',
          timestamp: new Date().toISOString(),
          source: context.source,
          payload: { ...options, timestamp: new Date().toISOString() }
        };
        
        await jtagRouter.routeMessage(screenshotMessage);
        
        return {
          success: true,
          filename: options.filename || 'screenshot.png',
          timestamp: new Date().toISOString(),
          requestId: context.requestId
        };
      },
      chainable: true,
      description: 'JTAG screenshot command'
    });
  }

  /**
   * Register Continuum widget commands
   */
  registerWidgetCommands(): void {
    this.registerCommand({
      name: 'create',
      namespace: 'widget',
      requiresEndpoint: ['browser'],
      handler: async (params, context) => {
        const [widgetType, config = {}] = params;
        console.log(`🎛️ Widget: Creating ${widgetType}...`);
        
        // Chain data from previous command if available
        const widgetConfig = context.chainData ? { ...config, chainData: context.chainData } : config;
        
        return {
          widgetId: `widget_${context.requestId}`,
          type: widgetType,
          config: widgetConfig,
          created: new Date().toISOString()
        };
      },
      chainable: true,
      description: 'Create widget in browser'
    });

    this.registerCommand({
      name: 'update',
      namespace: 'widget',
      requiresEndpoint: ['browser'],
      handler: async (params, context) => {
        const [widgetId, updates] = params;
        console.log(`🔄 Widget: Updating ${widgetId}...`);
        
        return {
          widgetId,
          updates,
          updated: new Date().toISOString(),
          success: true
        };
      },
      chainable: true,
      description: 'Update widget state'
    });
  }

  /**
   * Register Continuum system commands
   */
  registerContinuumCommands(): void {
    this.registerCommand({
      name: 'execute',
      namespace: 'continuum',
      requiresEndpoint: ['server'],
      handler: async (params, context) => {
        const [command, args = []] = params;
        console.log(`🔧 Continuum: Executing ${command}...`);
        
        return {
          command,
          args,
          executed: new Date().toISOString(),
          result: `Mock result for ${command}`,
          requestId: context.requestId
        };
      },
      chainable: true,
      description: 'Execute Continuum command'
    });

    this.registerCommand({
      name: 'fileSave',
      namespace: 'continuum',
      requiresEndpoint: ['server'],
      handler: async (params, context) => {
        const [filename, content] = params;
        console.log(`💾 Continuum: Saving file ${filename}...`);
        
        // Could use chain data from screenshot for example
        const finalContent = context.chainData?.filename ? 
          `${content}\n// Generated from: ${context.chainData.filename}` : content;
        
        return {
          filename,
          size: finalContent.length,
          saved: new Date().toISOString(),
          success: true
        };
      },
      chainable: true,
      description: 'Save file via Continuum'
    });
  }

  /**
   * Get all commands, optionally filtered by namespace
   */
  getCommands(namespace?: string): Map<string, CommandDefinition> {
    if (!namespace) return new Map(this.commands);
    
    const filtered = new Map();
    for (const [name, def] of this.commands) {
      if (def.namespace === namespace) {
        filtered.set(name, def);
      }
    }
    return filtered;
  }

  /**
   * List all available namespaces
   */
  getNamespaces(): string[] {
    const namespaces = new Set<string>();
    for (const [, command] of this.commands) {
      if (command.namespace) {
        namespaces.add(command.namespace);
      }
    }
    return Array.from(namespaces);
  }
}