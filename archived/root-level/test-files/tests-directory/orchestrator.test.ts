/**
 * Unit tests for the clean TypeScript Orchestrator
 * Prevents monolithic disasters and ensures proper separation of concerns
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import { Orchestrator, OrchestratorConfig } from '../src/orchestrator';
import { 
  IAgent, 
  AgentConfig, 
  TaskExecutionResult, 
  AgentState,
  ToolExecutionResult,
  AgentMetrics
} from '../src/interfaces/agent.interface';

describe('Orchestrator', () => {
  let orchestrator: Orchestrator;
  let mockAgent: MockAgent;
  
  const defaultConfig: OrchestratorConfig = {
    port: 5555,
    maxConcurrentTasks: 5,
    defaultTimeout: 10000,
    enableMetrics: true
  };

  beforeEach(() => {
    orchestrator = new Orchestrator(defaultConfig);
    mockAgent = new MockAgent({
      name: 'TestAI',
      type: 'GeneralAI',
      provider: 'anthropic',
      model: 'claude-3-haiku',
      maxTokens: 1000,
      temperature: 0.7
    });
  });

  afterEach(async () => {
    await orchestrator.shutdown();
  });

  test('should register agent successfully', async () => {
    await orchestrator.registerAgent(mockAgent);
    
    const status = orchestrator.getStatus();
    expect(status.activeAgents).toBe(1);
    expect(status.agentMetrics['TestAI']).toBeDefined();
  });

  test('should route simple task to single agent', async () => {
    await orchestrator.registerAgent(mockAgent);
    
    const result = await orchestrator.routeTask('Simple test task');
    
    expect(result.coordination).toBe(false);
    expect(result.results).toHaveLength(1);
    expect(result.results[0].agent).toBe('TestAI');
    expect(result.summary).toContain('TestAI completed task');
  });

  test('should prevent coordination when no multiple agents', async () => {
    await orchestrator.registerAgent(mockAgent);
    
    // Even coordination-type tasks should go to single agent if only one available
    const result = await orchestrator.routeTask('coordinate with CodeAI to fix CI');
    
    expect(result.coordination).toBe(false);
    expect(result.results).toHaveLength(1);
  });

  test('should handle multiple agents for coordination', async () => {
    const plannerAgent = new MockAgent({
      name: 'PlannerAI',
      type: 'PlannerAI',
      provider: 'openai',
      model: 'gpt-4o',
      maxTokens: 1500,
      temperature: 0.7
    });

    const codeAgent = new MockAgent({
      name: 'CodeAI', 
      type: 'CodeAI',
      provider: 'anthropic',
      model: 'claude-3-haiku',
      maxTokens: 1000,
      temperature: 0.5
    });

    await orchestrator.registerAgent(plannerAgent);
    await orchestrator.registerAgent(codeAgent);
    
    const result = await orchestrator.routeTask('coordinate with CodeAI to fix CI build');
    
    expect(result.coordination).toBe(true);
    expect(result.results).toHaveLength(2);
    expect(result.results.map(r => r.agent)).toEqual(['PlannerAI', 'CodeAI']);
  });

  test('should respect task capacity limits', async () => {
    const limitedOrchestrator = new Orchestrator({
      ...defaultConfig,
      maxConcurrentTasks: 1
    });

    await limitedOrchestrator.registerAgent(mockAgent);
    
    // Start a long-running task
    const longTask = limitedOrchestrator.routeTask('Long running task');
    
    // Try to start another task immediately
    await expect(limitedOrchestrator.routeTask('Second task'))
      .rejects
      .toThrow('Maximum concurrent tasks reached');
    
    // Wait for first task to complete
    await longTask;
    
    // Now second task should work
    const result = await limitedOrchestrator.routeTask('Second task');
    expect(result.results).toHaveLength(1);
    
    await limitedOrchestrator.shutdown();
  });

  test('should handle timeouts properly', async () => {
    const timeoutOrchestrator = new Orchestrator({
      ...defaultConfig,
      defaultTimeout: 100 // Very short timeout
    });

    const slowAgent = new MockAgent({
      name: 'SlowAI',
      type: 'GeneralAI',
      provider: 'anthropic',
      model: 'claude-3-haiku',
      maxTokens: 1000,
      temperature: 0.7
    });
    
    // Make agent slow
    slowAgent.setDelay(200); // Longer than timeout
    
    await timeoutOrchestrator.registerAgent(slowAgent);
    
    await expect(timeoutOrchestrator.routeTask('Slow task'))
      .rejects
      .toThrow('Task execution timeout');
    
    await timeoutOrchestrator.shutdown();
  });

  test('should emit proper events', async () => {
    const events: string[] = [];
    
    orchestrator.on('agentRegistered', () => events.push('registered'));
    orchestrator.on('taskCompleted', () => events.push('completed'));
    orchestrator.on('taskFailed', () => events.push('failed'));
    
    await orchestrator.registerAgent(mockAgent);
    await orchestrator.routeTask('Test task');
    
    expect(events).toEqual(['registered', 'completed']);
  });

  test('should route based on task content', async () => {
    const plannerAgent = new MockAgent({
      name: 'PlannerAI',
      type: 'PlannerAI', 
      provider: 'openai',
      model: 'gpt-4o',
      maxTokens: 1500,
      temperature: 0.7
    });

    await orchestrator.registerAgent(plannerAgent);
    await orchestrator.registerAgent(mockAgent);
    
    // Planning task should go to PlannerAI
    const planResult = await orchestrator.routeTask('Create a strategy for this project');
    expect(planResult.results[0].agent).toBe('PlannerAI');
    
    // General task should go to GeneralAI (fallback)
    const generalResult = await orchestrator.routeTask('What is the weather?');
    expect(generalResult.results[0].agent).toBe('TestAI'); // Our general mock agent
  });

  test('should shutdown gracefully', async () => {
    await orchestrator.registerAgent(mockAgent);
    
    const shutdownPromise = orchestrator.shutdown();
    
    // Verify agent gets shutdown call
    expect(mockAgent.shutdownCalled).toBe(false);
    await shutdownPromise;
    expect(mockAgent.shutdownCalled).toBe(true);
    
    // Verify agents are cleared
    const status = orchestrator.getStatus();
    expect(status.activeAgents).toBe(0);
  });
});

// Mock Agent for testing
class MockAgent implements IAgent {
  public readonly name: string;
  public readonly config: AgentConfig;
  private _state: AgentState = 'initializing';
  private _metrics: AgentMetrics = {
    requests: 0,
    cost: 0,
    errors: 0,
    successRate: 100,
    averageResponseTime: 100
  };
  private delay = 0;
  public shutdownCalled = false;

  constructor(config: AgentConfig) {
    this.name = config.name;
    this.config = Object.freeze(config);
  }

  get state(): AgentState {
    return this._state;
  }

  get metrics(): AgentMetrics {
    return { ...this._metrics };
  }

  setDelay(ms: number): void {
    this.delay = ms;
  }

  async initialize(): Promise<void> {
    this._state = 'ready';
  }

  async execute(task: string): Promise<TaskExecutionResult> {
    this._state = 'processing';
    
    if (this.delay > 0) {
      await new Promise(resolve => setTimeout(resolve, this.delay));
    }
    
    this._metrics.requests++;
    this._state = 'ready';
    
    return {
      agent: this.name,
      task,
      response: `Mock response for: ${task}`,
      toolResults: [],
      metrics: this.getMetrics(),
      timestamp: new Date(),
      duration: this.delay || 100
    };
  }

  async getAIResponse(task: string): Promise<string> {
    return `Mock AI response for: ${task}`;
  }

  async processTools(response: string): Promise<readonly ToolExecutionResult[]> {
    return [];
  }

  setState(newState: AgentState): void {
    this._state = newState;
  }

  getMetrics(): AgentMetrics {
    return { ...this._metrics };
  }

  async shutdown(): Promise<void> {
    this.shutdownCalled = true;
    this._state = 'shutdown';
  }
}