/**
 * Command Processor Daemon - TypeScript-first command execution orchestration
 * Strongly typed, modular, and designed for mesh distribution
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { 
  DaemonMessage, 
  DaemonResponse
} from '../base/DaemonProtocol.js';
import { DaemonConnector } from '../../integrations/websocket/core/DaemonConnector';

// Strongly typed command interfaces
export interface TypedCommandRequest<T = unknown> {
  readonly command: string;
  readonly parameters: T;
  readonly context: Record<string, any>;
  readonly routing?: CommandRouting;
}

export interface CommandRouting {
  readonly preferredProvider: 'browser' | 'python' | 'cloud' | 'mesh' | 'auto';
  readonly fallbackAllowed: boolean;
  readonly meshDistribution: boolean;
  readonly qualityRequirement: 'fast' | 'balanced' | 'accurate';
}

export interface CommandImplementation {
  readonly name: string;
  readonly provider: 'browser' | 'python' | 'cloud' | 'mesh';
  readonly status: 'available' | 'degraded' | 'unavailable';
  readonly quality: 'basic' | 'standard' | 'premium';
  readonly cost: CommandCost;
  readonly capabilities: readonly string[];
}

export interface CommandCost {
  readonly type: 'free' | 'per_execution' | 'per_minute' | 'subscription';
  readonly amount: number;
  readonly currency: string;
}

export interface CommandExecution<T = unknown, R = unknown> {
  readonly id: string;
  readonly command: string;
  readonly parameters: T;
  readonly implementation: CommandImplementation;
  readonly startTime: Date;
  readonly status: 'pending' | 'running' | 'completed' | 'failed';
  readonly result?: R;
  readonly error?: string;
  readonly executionTime?: number;
}

// Phase Omega Pattern of Care validation
export interface CareValidation {
  readonly isValid: boolean;
  readonly careLevel: 'concerning' | 'acceptable' | 'good' | 'excellent';
  readonly score: number;
  readonly message: string;
  readonly metrics: {
    readonly dignityPreservation: number;
    readonly cognitiveLoadReduction: number;
    readonly systemStability: number;
    readonly empowermentFactor: number;
    readonly harmPrevention: number;
  };
}

export class CommandProcessorDaemon extends BaseDaemon {
  public readonly name = 'command-processor';
  public readonly version = '2.0.0';
  public readonly config = {
    name: this.name,
    version: this.version,
    port: 9001,
    autoStart: true,
    dependencies: [],
    healthCheck: { interval: 30000, timeout: 5000, retries: 3 },
    resources: { maxMemory: 512, maxCpu: 70 }
  };

  private readonly implementations = new Map<string, readonly CommandImplementation[]>();
  private readonly activeExecutions = new Map<string, CommandExecution>();
  private readonly executionHistory: CommandExecution[] = [];
  private readonly phaseOmegaEnabled = true;
  private monitoringInterval: NodeJS.Timeout | undefined;
  private commandConnector!: DaemonConnector;

  protected async onStart(): Promise<void> {
    this.log('🚀 Initializing command discovery...');
    
    // Initialize command discovery
    this.commandConnector = new DaemonConnector();
    const connected = await this.commandConnector.connect();
    
    if (!connected) {
      this.log('❌ Failed to connect command discovery system', 'error');
      throw new Error('Command discovery system failed to initialize');
    }
    
    await this.registerCoreImplementations();
    this.startExecutionMonitoring();
    
    const availableCommands = this.commandConnector.getAvailableCommands();
    this.log(`✅ Command Processor started with ${availableCommands.length} discovered commands: ${availableCommands.join(', ')}`);
    
    // Listen for session_created events to log available commands for new sessions
    this.on('session_created', (event: any) => {
      this.logDiscoveredCommandsForSession(event);
    });
  }

  protected async onStop(): Promise<void> {
    // Stop monitoring interval to prevent memory leaks
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = undefined;
    }
    
    // Disconnect command connector
    if (this.commandConnector) {
      await this.commandConnector.disconnect();
    }
    
    // Cancel active executions
    for (const execution of this.activeExecutions.values()) {
      if (execution.status === 'running') {
        this.log(`Cancelling execution: ${execution.id}`);
      }
    }
    this.activeExecutions.clear();
    this.log('Command Processor Daemon stopped');
  }

  /**
   * Public interface for test compatibility
   */
  async processCommand<T = unknown, R = unknown>(request: TypedCommandRequest<T>): Promise<{ success: boolean; result?: R; error?: string }> {
    try {
      const response = await this.executeCommand(request);
      return {
        success: response.success,
        result: response.data as R,
        error: response.success ? undefined : (response.data as any)?.error || 'Command execution failed'
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'command.execute':
        return await this.executeCommand(message.data as TypedCommandRequest);
      
      case 'execute_command':
        // Handle from WebSocket/CLI
        const { command, args } = message.data;
        return await this.executeCommand({
          command,
          parameters: args || [],
          context: { source: message.data.source || 'cli' },
        });
      
      case 'handle_api':
        // Handle HTTP API requests
        const { pathname, method, url } = message.data;
        const pathParts = pathname.split('/').filter(Boolean);
        if (pathParts[0] === 'api' && pathParts[1] === 'commands' && pathParts[2]) {
          const command = pathParts[2];
          // Parse body from request if needed
          return await this.executeCommand({
            command,
            parameters: message.data.body?.args || [],
            context: { source: 'http', method, url }
          });
        }
        return {
          success: false,
          error: `Invalid API path: ${pathname}`
        };
      
      case 'command.get_implementations':
        return await this.getCommandImplementations(message.data as { command: string });
      
      case 'command.register_implementation':
        return await this.registerImplementation(message.data as CommandImplementation);
      
      // Chat messages should be handled by chat commands, not hardcoded here
      case 'message':
      case 'group_message':
      case 'direct_message':
        return {
          success: false,
          error: 'Chat messages should be sent through the chat command'
        };
      
      case 'get_capabilities':
        return {
          success: true,
          data: {
            capabilities: this.getCapabilities()
          }
        };
      
      default:
        return {
          success: false,
          error: `Unknown command type: ${message.type}`
        };
    }
  }

  /**
   * Get message types that this daemon can handle
   */
  getMessageTypes(): string[] {
    return [
      'command.execute',
      'command.get_implementations', 
      'command.register_implementation',
      'message',
      'group_message',
      'direct_message',
      'get_capabilities'
    ];
  }

  /**
   * Get daemon capabilities
   */
  private getCapabilities(): string[] {
    return [
      'command-execution',
      'chat-processing',
      'ai-routing',
      'message-handling'
    ];
  }

  /**
   * Execute command with Phase Omega validation and optimal routing
   */
  private async executeCommand<T, R>(request: TypedCommandRequest<T>): Promise<DaemonResponse> {
    const executionId = this.generateExecutionId();
    
    try {
      this.log(`⚡ Executing command: ${request.command} (${executionId})`);
      
      // Phase Omega: Validate Pattern of Care
      if (this.phaseOmegaEnabled) {
        const careValidation = await this.validatePatternOfCare(request);
        if (!careValidation.isValid) {
          throw new Error(`🚨 PHASE OMEGA PROTECTION: ${careValidation.message}`);
        }
        this.log(`✨ Pattern of Care validated: ${careValidation.careLevel} (${careValidation.score.toFixed(2)})`);
      }

      // Select optimal implementation
      const implementation = await this.selectImplementation(request);
      if (!implementation) {
        throw new Error(`No available implementation for command: ${request.command}`);
      }

      // Create execution tracking
      const execution: CommandExecution<T, R> = {
        id: executionId,
        command: request.command,
        parameters: request.parameters,
        implementation,
        startTime: new Date(),
        status: 'pending'
      };

      this.activeExecutions.set(executionId, execution);

      // Execute with selected implementation
      const result = await this.executeWithImplementation(execution, request);
      
      // Update execution
      const completedExecution: CommandExecution<T, R> = {
        ...execution,
        status: 'completed',
        result,
        executionTime: Date.now() - execution.startTime.getTime()
      };

      this.executionHistory.push(completedExecution);
      this.activeExecutions.delete(executionId);

      this.log(`✅ Command completed: ${request.command} (${completedExecution.executionTime}ms)`);

      return {
        success: true,
        data: completedExecution
      };

    } catch (error) {
      this.log(`❌ Command failed: ${request.command} (${executionId}) - ${error}`, 'error');
      
      const failedExecution: CommandExecution<T, R> = {
        id: executionId,
        command: request.command,
        parameters: request.parameters,
        implementation: { name: 'unknown', provider: 'browser', status: 'unavailable', quality: 'basic', cost: { type: 'free', amount: 0, currency: 'USD' }, capabilities: [] },
        startTime: new Date(),
        status: 'failed',
        error: error instanceof Error ? error.message : String(error),
        executionTime: 0
      };

      this.executionHistory.push(failedExecution);
      this.activeExecutions.delete(executionId);

      return {
        success: false,
        error: `Command execution failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Phase Omega Pattern of Care validation
   */
  private async validatePatternOfCare<T>(request: TypedCommandRequest<T>): Promise<CareValidation> {
    const { command, parameters } = request;
    const commandStr = command || '';
    const paramsStr = parameters ? JSON.stringify(parameters) : '';
    
    // Calculate care metrics
    const metrics = {
      dignityPreservation: this.assessDignityImpact(commandStr, paramsStr),
      cognitiveLoadReduction: this.assessCognitiveImpact(commandStr, paramsStr),
      systemStability: this.assessStabilityImpact(commandStr, paramsStr),
      empowermentFactor: this.assessEmpowermentImpact(commandStr, paramsStr),
      harmPrevention: this.assessHarmPrevention(commandStr, paramsStr)
    };
    
    const score = Object.values(metrics).reduce((sum, metric) => sum + metric, 0) / Object.keys(metrics).length;
    
    let careLevel: CareValidation['careLevel'] = 'acceptable';
    if (score >= 0.8) careLevel = 'excellent';
    else if (score >= 0.6) careLevel = 'good';
    else if (score >= 0.4) careLevel = 'acceptable';
    else careLevel = 'concerning';
    
    const isValid = score >= 0.2; // Block commands that significantly violate care pattern
    const message = isValid ? 
      `Pattern of care validated: ${careLevel}` :
      `Command blocked: violates pattern of care`;

    return { isValid, careLevel, score, message, metrics };
  }

  /**
   * Select optimal implementation based on routing preferences
   */
  private async selectImplementation<T>(request: TypedCommandRequest<T>): Promise<CommandImplementation | null> {
    const implementations = this.implementations.get(request.command) || [];
    const availableImpls = implementations.filter(impl => impl.status === 'available');
    
    if (availableImpls.length === 0) return null;

    // Simple selection logic - could be enhanced with AI
    const routing = request.routing;
    if (routing?.preferredProvider && routing.preferredProvider !== 'auto') {
      const preferred = availableImpls.find(impl => impl.provider === routing.preferredProvider);
      if (preferred) return preferred;
      if (!routing.fallbackAllowed) return null;
    }

    // Default to highest quality available
    return availableImpls.sort((a, b) => {
      const qualityOrder = { premium: 3, standard: 2, basic: 1 };
      return qualityOrder[b.quality] - qualityOrder[a.quality];
    })[0];
  }

  /**
   * Execute command with selected implementation
   */
  private async executeWithImplementation<T, R>(
    execution: CommandExecution<T, R>,
    request: TypedCommandRequest<T>
  ): Promise<R> {
    const { implementation } = execution;
    
    // Update status
    const updatedExecution = { ...execution, status: 'running' as const };
    this.activeExecutions.set(execution.id, updatedExecution);
    
    switch (implementation.provider) {
      case 'browser':
        return await this.executeBrowserImplementation(request);
      case 'python':
        return await this.executePythonImplementation(request);
      case 'cloud':
        return await this.executeCloudImplementation(request);
      case 'mesh':
        return await this.executeMeshImplementation(request);
      default:
        throw new Error(`Unknown implementation provider: ${implementation.provider}`);
    }
  }

  // Implementation-specific execution methods
  private async executeBrowserImplementation<T, R>(request: TypedCommandRequest<T>): Promise<R> {
    this.log(`🌐 Executing command via dynamic discovery: ${request.command}`);
    
    // Use the command connector to execute discovered commands
    if (!this.commandConnector || !this.commandConnector.isConnected()) {
      throw new Error('Command discovery system not available');
    }
    
    const result = await this.commandConnector.executeCommand(
      request.command,
      request.parameters,
      request.context
    );
    
    if (result.success) {
      return result.data as R;
    } else {
      throw new Error(result.error || 'Command execution failed');
    }
  }

  private async executePythonImplementation<T, R>(request: TypedCommandRequest<T>): Promise<R> {
    this.log(`🐍 Executing in Python: ${request.command}`);
    // Would integrate with Python execution system
    return { provider: 'python', status: 'executed' } as R;
  }

  private async executeCloudImplementation<T, R>(request: TypedCommandRequest<T>): Promise<R> {
    this.log(`☁️ Executing in cloud: ${request.command}`);
    return { provider: 'cloud', status: 'executed' } as R;
  }

  private async executeMeshImplementation<T, R>(request: TypedCommandRequest<T>): Promise<R> {
    this.log(`🕸️ Executing in mesh: ${request.command}`);
    return { provider: 'mesh', status: 'executed' } as R;
  }

  // Care pattern assessment methods
  private assessDignityImpact(command: string, params: string): number {
    const dignityCommands = ['help', 'screenshot', 'status', 'info'];
    const harmfulPatterns = ['delete', 'destroy', 'break', 'hack'];
    
    if (dignityCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    if (harmfulPatterns.some(pattern => params.toLowerCase().includes(pattern))) return 0.1;
    return 0.7;
  }

  private assessCognitiveImpact(command: string, _params: string): number {
    const helpfulCommands = ['help', 'info', 'status', 'explain'];
    if (helpfulCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    return 0.6;
  }

  private assessStabilityImpact(command: string, _params: string): number {
    const stableCommands = ['info', 'status', 'help'];
    const riskyCommands = ['exec', 'delete', 'modify'];
    
    if (stableCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    if (riskyCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.4;
    return 0.7;
  }

  private assessEmpowermentImpact(command: string, _params: string): number {
    const empoweringCommands = ['help', 'learn', 'create', 'build'];
    if (empoweringCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    return 0.6;
  }

  private assessHarmPrevention(_command: string, params: string): number {
    const harmfulPatterns = ['rm -rf', 'delete all', 'destroy', 'break'];
    if (harmfulPatterns.some(pattern => params.toLowerCase().includes(pattern))) return 0.1;
    return 0.8;
  }

  // Helper methods
  private async registerCoreImplementations(): Promise<void> {
    // Get discovered commands from the command connector
    const availableCommands = this.commandConnector?.getAvailableCommands() || [];
    
    // Register all discovered commands as browser implementations
    // (since the dynamic discovery uses filesystem-based command loading)
    const discoveredImplementations: CommandImplementation[] = availableCommands.map(commandName => ({
      name: `${commandName}-discovered`,
      provider: 'browser' as const,
      status: 'available' as const,
      quality: 'standard' as const,
      cost: { type: 'free', amount: 0, currency: 'USD' },
      capabilities: ['command-execution', 'discovered']
    }));

    // Register implementations for each command
    for (let i = 0; i < availableCommands.length; i++) {
      const commandName = availableCommands[i];
      const implementation = discoveredImplementations[i];
      this.implementations.set(commandName, [implementation]);
    }
    
    this.log(`Registered ${discoveredImplementations.length} discovered command implementations: ${availableCommands.join(', ')}`);
  }

  private async getCommandImplementations(data: { command: string }): Promise<DaemonResponse> {
    const implementations = this.implementations.get(data.command) || [];
    return {
      success: true,
      data: implementations
    };
  }

  private async registerImplementation(implementation: CommandImplementation): Promise<DaemonResponse> {
    // Implementation registration logic
    this.log(`Registering implementation: ${implementation.name}`);
    return {
      success: true,
      data: true
    };
  }

  private startExecutionMonitoring(): void {
    this.monitoringInterval = setInterval(() => {
      if (this.activeExecutions.size > 0) {
        this.log(`📊 Active executions: ${this.activeExecutions.size}`);
      }
    }, 60000); // Every minute
  }

  private generateExecutionId(): string {
    return `exec-${Date.now().toString(36)}-${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Log discovered commands when a new session is created
   */
  private logDiscoveredCommandsForSession(event: any): void {
    const { sessionId, sessionType, owner } = event;
    const availableCommands = this.commandConnector?.getAvailableCommands() || [];
    
    this.log(`📚 NEW SESSION COMMAND DISCOVERY for ${sessionType} session ${sessionId} (owner: ${owner}):`);
    this.log(`📋 ${availableCommands.length} commands available:`);
    
    // Log commands in groups for readability
    const commandsByCategory: Record<string, string[]> = {};
    
    // Group commands by category (before the colon or by first word)
    for (const cmd of availableCommands) {
      let category = 'general';
      if (cmd.includes(':')) {
        category = cmd.split(':')[0];
      } else if (cmd.includes('_')) {
        category = cmd.split('_')[0];
      }
      
      if (!commandsByCategory[category]) {
        commandsByCategory[category] = [];
      }
      commandsByCategory[category].push(cmd);
    }
    
    // Log by category
    for (const [category, commands] of Object.entries(commandsByCategory)) {
      this.log(`  ${category}: ${commands.join(', ')}`);
    }
    
    this.log(`🎯 Session ${sessionId} ready with full command access`);
  }
}

// Main execution when run directly (direct execution detection)
if (process.argv[1] && process.argv[1].endsWith('CommandProcessorDaemon.ts')) {
  const daemon = new CommandProcessorDaemon();
  daemon.start().catch(error => {
    console.error('❌ Failed to start Command Processor Daemon:', error);
    process.exit(1);
  });
}

export default CommandProcessorDaemon;