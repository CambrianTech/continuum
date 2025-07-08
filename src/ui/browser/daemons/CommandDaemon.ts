/**
 * Browser Command Daemon - Extracted from continuum-browser.ts
 * 
 * Handles command execution, discovery, and console forwarding
 * Part of the modular browser daemon architecture
 */

export interface CommandResult {
  success: boolean;
  data?: any;
  error?: string;
}

export interface QueuedCommand {
  command: string;
  params: any;
  requestId: string;
  sessionId?: string;
}

export interface ConsoleCommand {
  action: string;
  message: string;
  source: string;
  data: any;
  sessionId?: string;
}

export class CommandDaemon {
  private ws: WebSocket | null = null;
  private sessionId: string | null = null;
  private clientId: string | null = null;
  private messageQueue: any[] = [];
  private consoleQueue: ConsoleCommand[] = [];
  private consoleProcessing = false;
  private pendingCommands = new Map<string, { resolve: (value: any) => void; reject: (reason: any) => void }>();

  constructor() {
    console.log('🎯 CommandDaemon: Initializing command execution system');
  }

  /**
   * Initialize the command daemon with WebSocket connection
   */
  initialize(ws: WebSocket, sessionId?: string, clientId?: string): void {
    this.ws = ws;
    this.sessionId = sessionId || null;
    this.clientId = clientId || null;
    
    console.log('🎯 CommandDaemon: Initialized with WebSocket connection');
    if (sessionId) {
      console.log(`🎯 CommandDaemon: Session ID: ${sessionId}`);
    }
  }

  /**
   * Update session information
   */
  setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
    console.log(`🎯 CommandDaemon: Session ID updated: ${sessionId}`);
    
    // Process any queued console commands now that we have session
    this.processConsoleQueue();
  }

  /**
   * Execute a command
   */
  async execute(command: string, params: any = {}): Promise<any> {
    if (!this.isConnected()) {
      throw new Error('Command daemon not connected');
    }

    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const timeout = setTimeout(() => {
        this.pendingCommands.delete(requestId);
        reject(new Error(`Command '${command}' timed out`));
      }, 10000);

      // Store promise resolvers
      this.pendingCommands.set(requestId, { 
        resolve: (data: any) => {
          clearTimeout(timeout);
          resolve(data);
        },
        reject: (error: any) => {
          clearTimeout(timeout);
          reject(error);
        }
      });

      // Send command
      this.sendMessage({
        type: 'execute_command',
        data: {
          command,
          params: typeof params === 'string' ? params : JSON.stringify(params),
          requestId,
          sessionId: this.sessionId
        }
      });
    });
  }

  /**
   * Handle command response messages
   */
  handleCommandResponse(message: any): boolean {
    if (message.type === 'execute_command_response') {
      const _requestId = message.data?.requestId;
      const pendingCommand = this.pendingCommands.get(_requestId);
      
      if (pendingCommand) {
        this.pendingCommands.delete(_requestId);
        
        if (message.data?.success) {
          pendingCommand.resolve(message.data.data || { success: true });
        } else {
          pendingCommand.reject(new Error(message.data?.error || 'Command failed'));
        }
        return true; // Message handled
      }
    }
    return false; // Message not handled
  }

  /**
   * Queue console command for forwarding
   */
  queueConsoleCommand(consoleCommand: ConsoleCommand): void {
    this.consoleQueue.push(consoleCommand);
    
    if (!this.consoleProcessing) {
      this.processConsoleQueue();
    }
  }

  /**
   * Process console command queue with rate limiting
   */
  private async processConsoleQueue(): Promise<void> {
    if (this.consoleProcessing || this.consoleQueue.length === 0) {
      return;
    }

    this.consoleProcessing = true;

    while (this.consoleQueue.length > 0) {
      const consoleCommand = this.consoleQueue.shift();
      
      if (this.isConnected()) {
        // Only send console commands if we have a sessionId
        if (this.sessionId) {
          try {
            // Update the sessionId in the console command to ensure it's current
            consoleCommand!.sessionId = this.sessionId;
            
            // Execute console command with shorter timeout for faster failure
            await this.execute('console', consoleCommand);
          } catch (error) {
            // Log failed console forwards to original console to avoid loops
            setTimeout(() => {
              const originalConsole = (window as any).__originalConsole__ || console;
              originalConsole.warn('⚠️ Console forward failed:', error);
            }, 0);
          }
        } else {
          // No sessionId yet - keep the console command in queue
          this.consoleQueue.unshift(consoleCommand!);
          
          // Log debug message about waiting for sessionId
          const originalConsole = (window as any).__originalConsole__ || console;
          originalConsole.log('🔍 [SESSION_DEBUG] Console command queued - waiting for session_ready message');
          
          // Stop processing until we have sessionId
          break;
        }
        
        // Rate limit: wait 50ms between console commands to prevent overwhelming
        if (this.consoleQueue.length > 0) {
          await new Promise(resolve => setTimeout(resolve, 50));
        }
      } else {
        // If not connected, add to message queue
        this.messageQueue.push({
          type: 'execute_command',
          data: {
            command: 'console',
            params: JSON.stringify(consoleCommand),
            requestId: `console_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`
          }
        });
        break; // Stop processing until connected
      }
    }

    this.consoleProcessing = false;
  }

  /**
   * Discover and log available commands
   */
  async discoverCommands(): Promise<string[]> {
    try {
      console.log('🔍 Discovering available commands for session...');
      
      // Try to get command list from server
      const response = await this.execute('help', {});
      
      if (response && response.commands) {
        console.log(`📋 Available Commands (${response.commands.length} total):`);
        
        // Group commands by category for better readability
        const commandsByCategory: Record<string, string[]> = {};
        
        for (const cmd of response.commands) {
          const category = cmd.category || 'general';
          if (!commandsByCategory[category]) {
            commandsByCategory[category] = [];
          }
          commandsByCategory[category].push(cmd.name);
        }
        
        // Log commands by category
        for (const [category, commands] of Object.entries(commandsByCategory)) {
          console.log(`  ${category}: ${commands.join(', ')}`);
        }
        
        return response.commands.map((cmd: any) => cmd.name);
      } else {
        // Fallback: try common commands to see what's available
        console.log('📋 Testing common commands...');
        const testCommands = ['health', 'help', 'js-execute', 'browserjs', 'screenshot', 'console'];
        
        for (const cmd of testCommands) {
          try {
            // Quick test - don't wait for full execution
            setTimeout(() => this.execute(cmd, {}).catch(() => {}), 100);
          } catch {
            // Ignore errors, just testing availability
          }
        }
        
        return testCommands;
      }
      
    } catch (error) {
      console.log('⚠️ Could not discover commands:', error instanceof Error ? error.message : String(error));
      console.log('💡 Commands will be discovered as they are used');
      return [];
    }
  }

  /**
   * Check if WebSocket is connected
   */
  private isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Send message via WebSocket
   */
  private sendMessage(message: any): void {
    if (this.isConnected() && this.ws) {
      this.ws.send(JSON.stringify({
        ...message,
        timestamp: new Date().toISOString(),
        clientId: this.clientId,
        sessionId: this.sessionId
      }));
    } else {
      // Queue message for when connection is ready
      this.messageQueue.push(message);
    }
  }

  /**
   * Flush queued messages when connection is restored
   */
  flushMessageQueue(): void {
    while (this.messageQueue.length > 0 && this.isConnected()) {
      const message = this.messageQueue.shift();
      this.sendMessage(message);
    }
  }

  /**
   * Get queue status for debugging
   */
  getQueueStatus(): { messageQueue: number; consoleQueue: number; pendingCommands: number } {
    return {
      messageQueue: this.messageQueue.length,
      consoleQueue: this.consoleQueue.length,
      pendingCommands: this.pendingCommands.size
    };
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    // Clear pending commands
    for (const [_requestId, { reject }] of this.pendingCommands) {
      reject(new Error('Command daemon destroyed'));
    }
    this.pendingCommands.clear();
    
    // Clear queues
    this.messageQueue = [];
    this.consoleQueue = [];
    
    // Reset state
    this.ws = null;
    this.sessionId = null;
    this.clientId = null;
    this.consoleProcessing = false;
    
    console.log('🎯 CommandDaemon: Destroyed and cleaned up');
  }
}

// Export singleton instance
export const commandDaemon = new CommandDaemon();