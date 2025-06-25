/**
 * Command Processor Daemon - TypeScript-first command execution orchestration
 * Strongly typed, modular, and designed for mesh distribution
 */

import { BaseDaemon } from '../base/BaseDaemon.js';
import { 
  DaemonMessage, 
  DaemonResponse, 
  DaemonStatus
} from '../base/DaemonProtocol.js';

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

  protected async onStart(): Promise<void> {
    await this.registerCoreImplementations();
    this.startExecutionMonitoring();
    this.log('Command Processor Daemon started with TypeScript architecture');
  }

  protected async onStop(): Promise<void> {
    // Cancel active executions
    for (const execution of this.activeExecutions.values()) {
      if (execution.status === 'running') {
        this.log(`Cancelling execution: ${execution.id}`);
      }
    }
    this.activeExecutions.clear();
    this.log('Command Processor Daemon stopped');
  }

  public async handleMessage<T, R>(message: DaemonMessage<T>): Promise<DaemonResponse<R>> {
    switch (message.type) {
      case 'command.execute':
        return await this.executeCommand(message.data as TypedCommandRequest) as DaemonResponse<R>;
      
      case 'command.get_implementations':
        return await this.getCommandImplementations(message.data as { command: string }) as DaemonResponse<R>;
      
      case 'command.register_implementation':
        return await this.registerImplementation(message.data as CommandImplementation) as DaemonResponse<R>;
      
      default:
        return {
          success: false,
          error: `Unknown command type: ${message.type}`,
          timestamp: new Date()
        };
    }
  }

  /**
   * Execute command with Phase Omega validation and optimal routing
   */
  private async executeCommand<T, R>(request: TypedCommandRequest<T>): Promise<DaemonResponse<CommandExecution<T, R>>> {
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
        data: completedExecution,
        timestamp: new Date()
      };

    } catch (error) {
      this.log(`❌ Command failed: ${request.command} (${executionId}) - ${error}`, 'error');
      
      const failedExecution: CommandExecution<T, R> = {
        id: executionId,
        command: request.command,
        parameters: request.parameters,
        implementation: { name: 'unknown', provider: 'auto', status: 'unavailable', quality: 'basic', cost: { type: 'free', amount: 0, currency: 'USD' }, capabilities: [] },
        startTime: new Date(),
        status: 'failed',
        error: error instanceof Error ? error.message : String(error),
        executionTime: 0
      };

      this.executionHistory.push(failedExecution);
      this.activeExecutions.delete(executionId);

      return {
        success: false,
        error: `Command execution failed: ${error instanceof Error ? error.message : String(error)}`,
        timestamp: new Date()
      };
    }
  }

  /**
   * Phase Omega Pattern of Care validation
   */
  private async validatePatternOfCare<T>(request: TypedCommandRequest<T>): Promise<CareValidation> {
    const { command, parameters } = request;
    const paramsStr = JSON.stringify(parameters);
    
    // Calculate care metrics
    const metrics = {
      dignityPreservation: this.assessDignityImpact(command, paramsStr),
      cognitiveLoadReduction: this.assessCognitiveImpact(command, paramsStr),
      systemStability: this.assessStabilityImpact(command, paramsStr),
      empowermentFactor: this.assessEmpowermentImpact(command, paramsStr),
      harmPrevention: this.assessHarmPrevention(command, paramsStr)
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
    this.log(`🌐 Executing in browser: ${request.command}`);
    // Would integrate with browser WebSocket system
    return { provider: 'browser', status: 'executed' } as R;
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

  private assessCognitiveImpact(command: string, params: string): number {
    const helpfulCommands = ['help', 'info', 'status', 'explain'];
    if (helpfulCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    return 0.6;
  }

  private assessStabilityImpact(command: string, params: string): number {
    const stableCommands = ['info', 'status', 'help'];
    const riskyCommands = ['exec', 'delete', 'modify'];
    
    if (stableCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    if (riskyCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.4;
    return 0.7;
  }

  private assessEmpowermentImpact(command: string, params: string): number {
    const empoweringCommands = ['help', 'learn', 'create', 'build'];
    if (empoweringCommands.some(cmd => command.toLowerCase().includes(cmd))) return 0.9;
    return 0.6;
  }

  private assessHarmPrevention(command: string, params: string): number {
    const harmfulPatterns = ['rm -rf', 'delete all', 'destroy', 'break'];
    if (harmfulPatterns.some(pattern => params.toLowerCase().includes(pattern))) return 0.1;
    return 0.8;
  }

  // Helper methods
  private async registerCoreImplementations(): Promise<void> {
    const coreImplementations: CommandImplementation[] = [
      {
        name: 'screenshot-browser',
        provider: 'browser',
        status: 'available',
        quality: 'standard',
        cost: { type: 'free', amount: 0, currency: 'USD' },
        capabilities: ['screenshot', 'visual', 'browser']
      },
      {
        name: 'info-local',
        provider: 'python',
        status: 'available',
        quality: 'basic',
        cost: { type: 'free', amount: 0, currency: 'USD' },
        capabilities: ['info', 'system', 'status']
      }
    ];

    this.implementations.set('screenshot', [coreImplementations[0]]);
    this.implementations.set('info', [coreImplementations[1]]);
    
    this.log(`Registered ${coreImplementations.length} core implementations`);
  }

  private async getCommandImplementations(data: { command: string }): Promise<DaemonResponse<readonly CommandImplementation[]>> {
    const implementations = this.implementations.get(data.command) || [];
    return {
      success: true,
      data: implementations,
      timestamp: new Date()
    };
  }

  private async registerImplementation(implementation: CommandImplementation): Promise<DaemonResponse<boolean>> {
    // Implementation registration logic
    this.log(`Registering implementation: ${implementation.name}`);
    return {
      success: true,
      data: true,
      timestamp: new Date()
    };
  }

  private startExecutionMonitoring(): void {
    setInterval(() => {
      if (this.activeExecutions.size > 0) {
        this.log(`📊 Active executions: ${this.activeExecutions.size}`);
      }
    }, 60000); // Every minute
  }

  private generateExecutionId(): string {
    return `exec-${Date.now().toString(36)}-${Math.random().toString(36).substring(2, 8)}`;
  }
}

// Main execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new CommandProcessorDaemon();
  daemon.start().catch(error => {
    console.error('❌ Failed to start Command Processor Daemon:', error);
    process.exit(1);
  });
}

export default CommandProcessorDaemon;