/**
 * Clean TypeScript Orchestrator - No Monolithic Disasters
 * 
 * Each component manages its own concerns:
 * - Agents manage their own tools and state
 * - Tools handle their own input/output
 * - Common piping between components through interfaces
 */

import { IAgent, AgentConfig, TaskExecutionResult, AgentType } from './interfaces/agent.interface';
import { EventEmitter } from 'events';

export interface OrchestrationResult {
  readonly task: string;
  readonly results: readonly TaskExecutionResult[];
  readonly coordination: boolean;
  readonly summary: string;
  readonly timestamp: Date;
  readonly duration: number;
}

export interface OrchestratorConfig {
  readonly port: number;
  readonly maxConcurrentTasks: number;
  readonly defaultTimeout: number;
  readonly enableMetrics: boolean;
}

export class Orchestrator extends EventEmitter {
  private readonly agents: Map<string, IAgent> = new Map();
  private readonly config: OrchestratorConfig;
  private readonly taskQueue: string[] = [];
  private activeTasks = 0;

  constructor(config: OrchestratorConfig) {
    super();
    this.config = Object.freeze(config);
  }

  /**
   * Register an agent with the orchestrator
   * Agent manages its own lifecycle and tools
   */
  public async registerAgent(agent: IAgent): Promise<void> {
    try {
      await agent.initialize();
      this.agents.set(agent.name, agent);
      
      this.emit('agentRegistered', {
        name: agent.name,
        type: agent.config.type,
        timestamp: new Date()
      });
      
      console.log(`✅ Agent registered: ${agent.name}`);
    } catch (error) {
      console.error(`❌ Failed to register agent ${agent.name}:`, error);
      throw error;
    }
  }

  /**
   * Route task to appropriate agent(s)
   * Uses clean routing logic without god object patterns
   */
  public async routeTask(task: string): Promise<OrchestrationResult> {
    const startTime = Date.now();
    
    try {
      this.validateTaskCapacity();
      this.activeTasks++;

      const routing = this.determineRouting(task);
      const results: TaskExecutionResult[] = [];

      if (routing.coordination) {
        // Multi-agent coordination
        for (const agentName of routing.agents) {
          const agent = this.getAgent(agentName);
          const result = await this.executeWithTimeout(
            agent.execute(task),
            this.config.defaultTimeout
          );
          results.push(result);
        }
      } else {
        // Single agent execution
        const agent = this.getAgent(routing.agents[0]);
        const result = await this.executeWithTimeout(
          agent.execute(task),
          this.config.defaultTimeout
        );
        results.push(result);
      }

      const orchestrationResult: OrchestrationResult = {
        task,
        results: Object.freeze(results),
        coordination: routing.coordination,
        summary: this.generateSummary(results, routing.coordination),
        timestamp: new Date(),
        duration: Date.now() - startTime
      };

      this.emit('taskCompleted', orchestrationResult);
      return orchestrationResult;

    } catch (error) {
      this.emit('taskFailed', { task, error, timestamp: new Date() });
      throw error;
    } finally {
      this.activeTasks--;
    }
  }

  /**
   * Clean routing logic - no monolithic conditionals
   */
  private determineRouting(task: string): { agents: string[]; coordination: boolean } {
    const taskLower = task.toLowerCase();
    
    // Coordination patterns
    const coordinationPatterns = [
      /coordinate.*with/i,
      /ci.*fail/i,
      /github.*pr/i,
      /fix.*build/i
    ];

    if (coordinationPatterns.some(pattern => pattern.test(task))) {
      return { agents: ['PlannerAI', 'CodeAI'], coordination: true };
    }

    // Single agent routing
    const routingRules: Array<{ pattern: RegExp; agent: string }> = [
      { pattern: /plan|strategy|architecture|design/i, agent: 'PlannerAI' },
      { pattern: /code|implement|debug|fix/i, agent: 'CodeAI' },
      { pattern: /.*/i, agent: 'GeneralAI' } // Default fallback
    ];

    for (const rule of routingRules) {
      if (rule.pattern.test(task)) {
        return { agents: [rule.agent], coordination: false };
      }
    }

    return { agents: ['GeneralAI'], coordination: false };
  }

  private getAgent(name: string): IAgent {
    const agent = this.agents.get(name);
    if (!agent) {
      throw new Error(`Agent not found: ${name}`);
    }
    return agent;
  }

  private validateTaskCapacity(): void {
    if (this.activeTasks >= this.config.maxConcurrentTasks) {
      throw new Error('Maximum concurrent tasks reached');
    }
  }

  private async executeWithTimeout<T>(
    promise: Promise<T>, 
    timeoutMs: number
  ): Promise<T> {
    return Promise.race([
      promise,
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error('Task execution timeout')), timeoutMs)
      )
    ]);
  }

  private generateSummary(
    results: readonly TaskExecutionResult[], 
    coordination: boolean
  ): string {
    if (coordination) {
      const agents = results.map(r => r.agent).join(' + ');
      return `${agents} coordination completed`;
    } else {
      return `${results[0].agent} completed task`;
    }
  }

  /**
   * Get orchestrator status and metrics
   */
  public getStatus(): {
    activeAgents: number;
    activeTasks: number;
    totalTasks: number;
    agentMetrics: Record<string, any>;
  } {
    const agentMetrics: Record<string, any> = {};
    
    for (const [name, agent] of Array.from(this.agents)) {
      agentMetrics[name] = agent.getMetrics();
    }

    return {
      activeAgents: this.agents.size,
      activeTasks: this.activeTasks,
      totalTasks: this.listenerCount('taskCompleted'),
      agentMetrics
    };
  }

  /**
   * Graceful shutdown
   */
  public async shutdown(): Promise<void> {
    console.log('🔄 Shutting down orchestrator...');
    
    // Wait for active tasks to complete
    while (this.activeTasks > 0) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    // Shutdown all agents
    const shutdownPromises = Array.from(this.agents.values()).map(agent => 
      agent.shutdown().catch(error => 
        console.error(`Error shutting down agent ${agent.name}:`, error)
      )
    );

    await Promise.allSettled(shutdownPromises);
    this.agents.clear();
    
    console.log('✅ Orchestrator shutdown complete');
  }
}