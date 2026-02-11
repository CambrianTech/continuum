/**
 * Sentinel Base Architecture
 *
 * Sentinels are autonomous task executors. They come in two flavors:
 *
 * 1. SCRIPT SENTINELS (no AI required)
 *    - BuildSentinel: Parse errors, apply fixes, rebuild
 *    - VisualSentinel: Launch browser, screenshot
 *    - ServeSentinel: Start HTTP server
 *    - TestSentinel: Run test suite, report results
 *
 * 2. AI-POWERED SENTINELS (require LLM)
 *    - OrchestratorSentinel: Plan and execute goals
 *    - CodeSentinel: Write/modify code
 *    - ReviewSentinel: Code review and suggestions
 *    - DebugSentinel: Analyze errors, suggest fixes
 *
 * The key insight: Sentinels are PIPELINES, not just LLM wrappers.
 * Even AI-powered sentinels are mostly deterministic - the LLM
 * is just one step in a larger pipeline.
 */

import { ModelConfig, ModelCapacity, ModelProvider, ModelInvoker, InferenceResult } from './ModelProvider';

/**
 * Sentinel execution result
 */
export interface SentinelResult<T = unknown> {
  success: boolean;
  data?: T;
  error?: string;
  duration?: number;
  steps?: SentinelStep[];
}

/**
 * A step in sentinel execution
 */
export interface SentinelStep {
  name: string;
  success: boolean;
  duration: number;
  output?: string;
}

/**
 * Base config for all sentinels
 */
export interface BaseSentinelConfig {
  workingDir: string;
  timeout?: number;           // Max execution time (ms)
  verbose?: boolean;          // Log steps
  onStep?: (step: SentinelStep) => void;
}

/**
 * Config for AI-powered sentinels
 */
export interface AISentinelConfig extends BaseSentinelConfig {
  model?: ModelConfig;        // Model selection
}

/**
 * Base class for script-only sentinels
 */
export abstract class ScriptSentinel<TConfig extends BaseSentinelConfig, TResult> {
  protected config: TConfig;
  protected steps: SentinelStep[] = [];
  protected startTime: number = 0;

  constructor(config: TConfig) {
    this.config = config;
  }

  /**
   * Execute the sentinel's task
   */
  abstract execute(...args: unknown[]): Promise<SentinelResult<TResult>>;

  /**
   * Record a step
   */
  protected recordStep(name: string, success: boolean, output?: string): void {
    const step: SentinelStep = {
      name,
      success,
      duration: Date.now() - this.startTime,
      output,
    };
    this.steps.push(step);
    if (this.config.verbose) {
      console.log(`[${success ? '✓' : '✗'}] ${name}${output ? `: ${output.slice(0, 100)}` : ''}`);
    }
    this.config.onStep?.(step);
  }

  /**
   * Start timing
   */
  protected start(): void {
    this.startTime = Date.now();
    this.steps = [];
  }

  /**
   * Create result
   */
  protected result<T>(success: boolean, data?: T, error?: string): SentinelResult<T> {
    return {
      success,
      data,
      error,
      duration: Date.now() - this.startTime,
      steps: this.steps,
    };
  }
}

/**
 * Base class for AI-powered sentinels
 */
export abstract class AISentinel<TConfig extends AISentinelConfig, TResult> extends ScriptSentinel<TConfig, TResult> {
  protected invoker: ModelInvoker;

  constructor(config: TConfig) {
    super(config);
    this.invoker = new ModelInvoker(config.workingDir);
  }

  /**
   * Call the AI model
   */
  protected async think(prompt: string): Promise<InferenceResult> {
    this.recordStep('thinking', true, `Prompt: ${prompt.slice(0, 50)}...`);
    const result = await this.invoker.generate(prompt, this.config.model);
    this.recordStep('thought', result.success, result.text?.slice(0, 100) || result.error);
    return result;
  }

  /**
   * Convenience: think with specific capacity
   */
  protected async thinkWith(prompt: string, capacity: ModelCapacity): Promise<InferenceResult> {
    return this.invoker.generate(prompt, { ...this.config.model, capacity });
  }
}

/**
 * Sentinel registry - for discovery and spawning
 */
export class SentinelRegistry {
  private static sentinels: Map<string, new (config: any) => ScriptSentinel<any, any>> = new Map();

  static register(name: string, sentinel: new (config: any) => ScriptSentinel<any, any>): void {
    this.sentinels.set(name, sentinel);
  }

  static get(name: string): (new (config: any) => ScriptSentinel<any, any>) | undefined {
    return this.sentinels.get(name);
  }

  static list(): string[] {
    return Array.from(this.sentinels.keys());
  }

  static spawn<T extends ScriptSentinel<any, any>>(name: string, config: any): T | undefined {
    const SentinelClass = this.sentinels.get(name);
    if (SentinelClass) {
      return new SentinelClass(config) as T;
    }
    return undefined;
  }
}

// Export types
export type { ModelConfig, ModelCapacity, ModelProvider, InferenceResult };
export { ModelCapacity as Capacity, ModelProvider as Provider } from './ModelProvider';
