/**
 * JTAG Client Factory - Eliminates duplicate client connection code across all tests
 * 
 * Provides standardized client connections with proper typing, error handling,
 * and consistent patterns. Replaces scattered connection logic throughout test suite.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGEnvironment } from '../../system/core/types/JTAGTypes';

// =============================================================================
// CLIENT CONNECTION CONSTANTS - No magic strings or numbers
// =============================================================================

export const CLIENT_CONSTANTS = {
  TIMEOUTS: {
    CONNECTION: 15000,        // 15s for client connection
    COMMAND_EXECUTION: 30000, // 30s for command execution
    BROWSER_WAIT: 5000,       // 5s for browser element waits
    SCREENSHOT: 10000         // 10s for screenshot capture
  },
  
  RETRY: {
    CONNECTION_ATTEMPTS: 3,
    DELAY_BETWEEN_ATTEMPTS: 1000
  },
  
  VALIDATION: {
    MIN_SESSION_ID_LENGTH: 10,
    REQUIRED_CLIENT_METHODS: ['commands', 'sessionId', 'disconnect']
  }
} as const;

// =============================================================================
// TYPESCRIPT INTERFACES - Rigid protocols for client operations
// =============================================================================

export interface ClientConnectionOptions {
  timeout?: number;
  retryAttempts?: number;
  environment?: JTAGEnvironment;
  validateConnection?: boolean;
}

export interface ClientConnectionResult {
  client: JTAGClient;
  sessionId: string;
  connectionTime: number;
  environment: string;
}

export interface CommandExecutionOptions {
  timeout?: number;
  validateResult?: boolean;
  logExecution?: boolean;
}

export interface CommandResult<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  executionTime: number;
}

// =============================================================================
// JTAG CLIENT FACTORY - Standardized connection management
// =============================================================================

export class JTAGClientFactory {
  private static instance: JTAGClientFactory;
  private connectedClients: Map<string, JTAGClient> = new Map();
  
  private constructor() {}
  
  static getInstance(): JTAGClientFactory {
    if (!JTAGClientFactory.instance) {
      JTAGClientFactory.instance = new JTAGClientFactory();
    }
    return JTAGClientFactory.instance;
  }
  
  /**
   * Create and connect JTAG client with standardized error handling
   * Replaces all duplicate JTAGClientServer.connect() patterns
   */
  async createClient(options: ClientConnectionOptions = {}): Promise<ClientConnectionResult> {
    const startTime = Date.now();
    const timeout = options.timeout || CLIENT_CONSTANTS.TIMEOUTS.CONNECTION;
    const retryAttempts = options.retryAttempts || CLIENT_CONSTANTS.RETRY.CONNECTION_ATTEMPTS;
    
    console.log(`🔌 JTAGClientFactory: Creating client connection (timeout: ${timeout}ms)`);
    
    let lastError: Error | null = null;
    
    for (let attempt = 1; attempt <= retryAttempts; attempt++) {
      try {
        console.log(`🔄 JTAGClientFactory: Connection attempt ${attempt}/${retryAttempts}`);
        
        // Use standardized connection method
        const { client } = await this.connectWithTimeout(timeout);
        
        if (!client) {
          throw new Error('Client connection returned null/undefined');
        }
        
        // Validate client connection if requested
        if (options.validateConnection !== false) {
          await this.validateClient(client);
        }
        
        const connectionTime = Date.now() - startTime;
        const result: ClientConnectionResult = {
          client,
          sessionId: client.sessionId,
          connectionTime,
          environment: options.environment || 'server'
        };
        
        // Store client for cleanup tracking
        this.connectedClients.set(client.sessionId, client);
        
        console.log(`✅ JTAGClientFactory: Client connected successfully (${connectionTime}ms)`);
        console.log(`📋 JTAGClientFactory: Session ID: ${client.sessionId}`);
        
        return result;
        
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        console.warn(`⚠️ JTAGClientFactory: Attempt ${attempt} failed:`, lastError.message);
        
        if (attempt < retryAttempts) {
          console.log(`🔄 JTAGClientFactory: Retrying in ${CLIENT_CONSTANTS.RETRY.DELAY_BETWEEN_ATTEMPTS}ms...`);
          await this.sleep(CLIENT_CONSTANTS.RETRY.DELAY_BETWEEN_ATTEMPTS);
        }
      }
    }
    
    const connectionTime = Date.now() - startTime;
    const errorMessage = `Client connection failed after ${retryAttempts} attempts (${connectionTime}ms): ${lastError?.message}`;
    
    console.error(`❌ JTAGClientFactory: ${errorMessage}`);
    throw new Error(errorMessage);
  }
  
  /**
   * Connect with timeout wrapper
   */
  private async connectWithTimeout(timeout: number): Promise<{ client: JTAGClient }> {
    return Promise.race([
      JTAGClientServer.connect(),
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(`Connection timeout after ${timeout}ms`)), timeout)
      )
    ]);
  }
  
  /**
   * Validate client has required methods and properties
   */
  private async validateClient(client: JTAGClient): Promise<void> {
    console.log(`🔍 JTAGClientFactory: Validating client connection`);
    
    // Check required properties
    if (!client.sessionId || client.sessionId.length < CLIENT_CONSTANTS.VALIDATION.MIN_SESSION_ID_LENGTH) {
      throw new Error(`Invalid session ID: ${client.sessionId}`);
    }
    
    if (!client.commands) {
      throw new Error('Client missing commands interface');
    }
    
    // Verify essential methods exist
    for (const method of CLIENT_CONSTANTS.VALIDATION.REQUIRED_CLIENT_METHODS) {
      if (!(method in client)) {
        console.warn(`⚠️ JTAGClientFactory: Client missing method: ${method}`);
      }
    }
    
    console.log(`✅ JTAGClientFactory: Client validation passed`);
  }
  
  /**
   * Execute command with standardized error handling and timing
   */
  async executeCommand<T = any>(
    client: JTAGClient,
    commandName: string,
    params: any = {},
    options: CommandExecutionOptions = {}
  ): Promise<CommandResult<T>> {
    const startTime = Date.now();
    const timeout = options.timeout || CLIENT_CONSTANTS.TIMEOUTS.COMMAND_EXECUTION;
    
    if (options.logExecution !== false) {
      console.log(`⚡ JTAGClientFactory: Executing ${commandName} (timeout: ${timeout}ms)`);
    }
    
    try {
      // Get command from client
      const command = this.getCommandFromClient(client, commandName);
      if (!command) {
        throw new Error(`Command not found: ${commandName}`);
      }
      
      // Execute with timeout
      const result = await Promise.race([
        command(params),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`Command timeout after ${timeout}ms`)), timeout)
        )
      ]);
      
      const executionTime = Date.now() - startTime;
      
      // Validate result if requested
      if (options.validateResult !== false && result && !result.success) {
        throw new Error(`Command failed: ${result.error || 'Unknown error'}`);
      }
      
      if (options.logExecution !== false) {
        console.log(`✅ JTAGClientFactory: ${commandName} completed (${executionTime}ms)`);
      }
      
      return {
        success: true,
        data: result,
        executionTime
      };
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`❌ JTAGClientFactory: ${commandName} failed (${executionTime}ms):`, errorMessage);
      
      return {
        success: false,
        error: errorMessage,
        executionTime
      };
    }
  }
  
  /**
   * Get command function from client with support for different patterns
   */
  private getCommandFromClient(client: JTAGClient, commandName: string): Function | null {
    // New pattern: client.commands['command/name'] or client.commands.commandName
    if (client.commands) {
      if (typeof client.commands[commandName] === 'function') {
        return client.commands[commandName];
      }
      
      // Try with dots replaced by slashes (theme/list vs theme.list)
      const slashCommand = commandName.replace('.', '/');
      if (typeof client.commands[slashCommand] === 'function') {
        return client.commands[slashCommand];
      }
    }
    
    // Old pattern: client.screenshot, client.exec, etc.
    if (typeof (client as any)[commandName] === 'function') {
      return (client as any)[commandName].bind(client);
    }
    
    return null;
  }
  
  /**
   * Cleanup client connection with proper error handling
   */
  async cleanupClient(client: JTAGClient | null): Promise<void> {
    if (!client) return;
    
    try {
      console.log(`🧹 JTAGClientFactory: Cleaning up client ${client.sessionId}`);
      
      // Remove from tracking
      this.connectedClients.delete(client.sessionId);
      
      // Disconnect if method exists
      if (client.disconnect && typeof client.disconnect === 'function') {
        await client.disconnect();
        console.log(`✅ JTAGClientFactory: Client disconnected successfully`);
      } else {
        console.log(`ℹ️ JTAGClientFactory: Client has no disconnect method`);
      }
      
    } catch (error) {
      console.warn(`⚠️ JTAGClientFactory: Error during client cleanup:`, error);
    }
  }
  
  /**
   * Cleanup all tracked clients
   */
  async cleanupAllClients(): Promise<void> {
    console.log(`🧹 JTAGClientFactory: Cleaning up ${this.connectedClients.size} clients`);
    
    const cleanupPromises = Array.from(this.connectedClients.values())
      .map(client => this.cleanupClient(client));
      
    await Promise.all(cleanupPromises);
    this.connectedClients.clear();
    
    console.log(`✅ JTAGClientFactory: All clients cleaned up`);
  }
  
  /**
   * Get count of active connections
   */
  getActiveConnectionCount(): number {
    return this.connectedClients.size;
  }
  
  /**
   * Sleep utility
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// =============================================================================
// CONVENIENCE FUNCTIONS - Simple interface for common patterns
// =============================================================================

/**
 * Quick client connection for simple tests
 * Replaces: const { client } = await JTAGClientServer.connect();
 */
export async function connectJTAGClient(options?: ClientConnectionOptions): Promise<ClientConnectionResult> {
  const factory = JTAGClientFactory.getInstance();
  return factory.createClient(options);
}

/**
 * Execute single command with automatic client management
 * Replaces: await client.commands.screenshot(...)
 */
export async function executeJTAGCommand<T = any>(
  commandName: string,
  params: any = {},
  options?: CommandExecutionOptions & ClientConnectionOptions
): Promise<CommandResult<T>> {
  const factory = JTAGClientFactory.getInstance();
  
  let client: JTAGClient | null = null;
  
  try {
    const connection = await factory.createClient(options);
    client = connection.client;
    
    return await factory.executeCommand(client, commandName, params, options);
    
  } finally {
    if (client) {
      await factory.cleanupClient(client);
    }
  }
}

/**
 * Screenshot with automatic client management
 * Replaces duplicate screenshot patterns across tests
 */
export async function takeJTAGScreenshot(
  selector: string = 'body',
  filename?: string,
  options?: CommandExecutionOptions & ClientConnectionOptions
): Promise<CommandResult<any>> {
  const screenshotParams = {
    querySelector: selector,
    ...(filename && { filename })
  };
  
  return executeJTAGCommand('screenshot', screenshotParams, options);
}

// Export singleton instance for direct access
export const jtagClientFactory = JTAGClientFactory.getInstance();