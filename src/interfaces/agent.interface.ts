/**
 * Agent Interface - TypeScript protocol for all AI agents
 * 
 * Defines strict contracts that all agents must implement
 * Prevents runtime errors through compile-time type checking
 */

export interface ToolExecutionResult {
  readonly tool: string;
  readonly command: string;
  readonly result: string;
  readonly success: boolean;
  readonly timestamp: Date;
  readonly duration?: number;
}

export interface AgentMetrics {
  requests: number;
  cost: number;
  errors: number;
  successRate: number;
  averageResponseTime: number;
}

export interface AgentConfig {
  readonly name: string;
  readonly type: AgentType;
  readonly provider: AIProvider;
  readonly model: string;
  readonly maxTokens: number;
  readonly temperature: number;
  readonly systemPrompt?: string;
}

export type AgentType = 'PlannerAI' | 'CodeAI' | 'GeneralAI' | 'SpecializedAI';
export type AIProvider = 'anthropic' | 'openai';
export type AgentState = 'initializing' | 'ready' | 'processing' | 'error' | 'shutdown';

export interface TaskExecutionResult {
  readonly agent: string;
  readonly task: string;
  readonly response: string;
  readonly toolResults: readonly ToolExecutionResult[];
  readonly metrics: AgentMetrics;
  readonly timestamp: Date;
  readonly duration: number;
}

export interface IAgent {
  readonly name: string;
  readonly config: AgentConfig;
  readonly state: AgentState;
  readonly metrics: AgentMetrics;

  /**
   * Initialize the agent and load required tools
   */
  initialize(): Promise<void>;

  /**
   * Execute a task and return results
   * @param task - The task to execute
   * @returns Promise with execution results
   */
  execute(task: string): Promise<TaskExecutionResult>;

  /**
   * Get AI response for the given task
   * @param task - The task description
   * @returns Promise with AI response
   */
  getAIResponse(task: string): Promise<string>;

  /**
   * Process tool commands from AI response
   * @param response - AI response text
   * @returns Promise with tool execution results
   */
  processTools(response: string): Promise<readonly ToolExecutionResult[]>;

  /**
   * Update agent state
   * @param newState - New state to transition to
   */
  setState(newState: AgentState): void;

  /**
   * Get current agent metrics
   * @returns Current performance metrics
   */
  getMetrics(): AgentMetrics;

  /**
   * Shutdown the agent gracefully
   */
  shutdown(): Promise<void>;
}

export abstract class BaseAgent implements IAgent {
  public readonly name: string;
  public readonly config: AgentConfig;
  private _state: AgentState = 'initializing';
  private _metrics: AgentMetrics;
  protected readonly tools: Map<string, ITool> = new Map();

  constructor(config: AgentConfig) {
    this.name = config.name;
    this.config = Object.freeze(config);
    this._metrics = {
      requests: 0,
      cost: 0,
      errors: 0,
      successRate: 0,
      averageResponseTime: 0
    };
  }

  public get state(): AgentState {
    return this._state;
  }

  public get metrics(): AgentMetrics {
    return { ...this._metrics };
  }

  public setState(newState: AgentState): void {
    const validTransitions: Record<AgentState, AgentState[]> = {
      'initializing': ['ready', 'error'],
      'ready': ['processing', 'shutdown'],
      'processing': ['ready', 'error'],
      'error': ['ready', 'shutdown'],
      'shutdown': []
    };

    if (validTransitions[this._state].includes(newState)) {
      this._state = newState;
    } else {
      throw new Error(`Invalid state transition from ${this._state} to ${newState}`);
    }
  }

  public getMetrics(): AgentMetrics {
    return { ...this._metrics };
  }

  protected updateMetrics(updates: Partial<AgentMetrics>): void {
    Object.assign(this._metrics, updates);
    this._metrics.successRate = this._metrics.requests > 0 
      ? (this._metrics.requests - this._metrics.errors) / this._metrics.requests 
      : 0;
  }

  public abstract initialize(): Promise<void>;
  public abstract getAIResponse(task: string): Promise<string>;
  public abstract execute(task: string): Promise<TaskExecutionResult>;
  public abstract processTools(response: string): Promise<readonly ToolExecutionResult[]>;
  public abstract shutdown(): Promise<void>;
}

// Tool interface
export interface ITool {
  readonly name: string;
  
  /**
   * Process AI response and execute any commands for this tool
   * @param response - AI response text
   * @returns Promise with execution results
   */
  processResponse(response: string): Promise<readonly ToolExecutionResult[]>;

  /**
   * Execute a specific command
   * @param command - Command to execute
   * @returns Promise with execution result
   */
  execute(command: string): Promise<ToolExecutionResult>;

  /**
   * Get tool metrics
   * @returns Performance metrics
   */
  getMetrics(): ToolMetrics;
}

export interface ToolMetrics {
  executions: number;
  errors: number;
  totalTime: number;
  averageTime: number;
  successRate: number;
}