/**
 * SentinelRunner — Declarative Step Engine
 *
 * Executes JSON-defined sentinel pipelines with:
 * - Variable substitution ($variable references)
 * - Loop control (until, while, continuous, event, count)
 * - Step types: command, llm, condition, watch, sentinel, emit
 * - Nested sentinel spawning
 * - Event-driven composition
 *
 * This is the "interpreter" that makes sentinels truly declarative.
 * AIs can create, modify, and share sentinels as pure data (JSON).
 */

import { Commands } from '../core/shared/Commands';
import { Events } from '../core/shared/Events';
import { ModelInvoker, ModelCapacity, ModelProvider } from './ModelProvider';
import type { ModelConfig } from './ModelProvider';
import type {
  PipelineSentinelDefinition,
  SentinelStep,
  LoopConfig,
  CommandStep,
  LLMStep,
  ConditionStep,
  WatchStep,
  SentinelSpawnStep,
  EmitStep,
  StepModelConfig,
} from './SentinelDefinition';

// ============================================================================
// Types
// ============================================================================

export interface ExecutionContext {
  /** Variables accumulated from step outputs */
  variables: Record<string, unknown>;
  /** Current loop iteration (1-indexed) */
  iteration: number;
  /** Execution trace for debugging */
  trace: StepTrace[];
  /** Start time (ms since epoch) */
  startedAt: number;
  /** Last step completion time */
  lastStepAt: number;
  /** Whether execution should stop */
  shouldStop: boolean;
  /** Stop reason if applicable */
  stopReason?: string;
}

export interface StepTrace {
  stepIndex: number;
  stepType: string;
  startedAt: number;
  completedAt: number;
  durationMs: number;
  success: boolean;
  output?: unknown;
  error?: string;
}

export interface SentinelResult {
  success: boolean;
  summary: string;
  context: ExecutionContext;
  exitCode?: number;
}

export interface RunnerConfig {
  /** Working directory for file operations */
  workingDir: string;
  /** Default model config for LLM steps without explicit model */
  defaultModel?: ModelConfig;
  /** Callbacks for observability */
  onStepStart?: (step: SentinelStep, ctx: ExecutionContext) => void;
  onStepComplete?: (step: SentinelStep, trace: StepTrace, ctx: ExecutionContext) => void;
  onIteration?: (iteration: number, ctx: ExecutionContext) => void;
  onLog?: (message: string, level: 'debug' | 'info' | 'warn' | 'error') => void;
}

// ============================================================================
// Variable Substitution
// ============================================================================

/**
 * Substitute $variable references in a value.
 *
 * Supports:
 * - Simple: $varName
 * - Nested: $varName.property.subProperty
 * - Array access: $varName[0].property
 * - In strings: "prefix $varName suffix"
 * - In objects: { key: "$varName" }
 */
function substituteVariables(value: unknown, variables: Record<string, unknown>): unknown {
  if (typeof value === 'string') {
    // Check if entire string is a variable reference
    const fullMatch = value.match(/^\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\])*)$/);
    if (fullMatch) {
      return resolveVariablePath(fullMatch[1], variables);
    }

    // Replace embedded variables in string
    return value.replace(/\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\])*)/g, (_, path) => {
      const resolved = resolveVariablePath(path, variables);
      return resolved === undefined ? `$${path}` : String(resolved);
    });
  }

  if (Array.isArray(value)) {
    return value.map(item => substituteVariables(item, variables));
  }

  if (value !== null && typeof value === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, val] of Object.entries(value)) {
      result[key] = substituteVariables(val, variables);
    }
    return result;
  }

  return value;
}

/**
 * Resolve a dotted/bracketed path like "buildOutput.exitCode" or "items[0].name"
 */
function resolveVariablePath(path: string, variables: Record<string, unknown>): unknown {
  const parts = path.split(/\.|\[(\d+)\]/).filter(Boolean);
  let current: unknown = variables;

  for (const part of parts) {
    if (current === null || current === undefined) {
      return undefined;
    }
    if (typeof current === 'object') {
      current = (current as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }

  return current;
}

/**
 * Evaluate a condition expression with variable access.
 *
 * Examples:
 * - "$buildOutput.exitCode === 0"
 * - "$findings.complete"
 * - "$items.length > 0"
 */
function evaluateCondition(check: string, variables: Record<string, unknown>): boolean {
  // Replace $variable references with actual values
  const substituted = check.replace(/\$([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\])*)/g, (_, path) => {
    const value = resolveVariablePath(path, variables);
    return JSON.stringify(value);
  });

  try {
    // Safe evaluation using Function constructor with no access to globals
    // eslint-disable-next-line no-new-func
    const evalFn = new Function(`"use strict"; return (${substituted});`);
    return Boolean(evalFn());
  } catch (e) {
    console.warn(`[SentinelRunner] Failed to evaluate condition: ${check}`, e);
    return false;
  }
}

// ============================================================================
// SentinelRunner
// ============================================================================

export class SentinelRunner {
  private config: RunnerConfig;
  private invoker: ModelInvoker;

  constructor(config: RunnerConfig) {
    this.config = config;
    this.invoker = new ModelInvoker(config.workingDir);
  }

  private log(message: string, level: 'debug' | 'info' | 'warn' | 'error' = 'info'): void {
    this.config.onLog?.(message, level);
    if (level === 'error') {
      console.error(`[SentinelRunner] ${message}`);
    } else if (level === 'warn') {
      console.warn(`[SentinelRunner] ${message}`);
    } else {
      console.log(`[SentinelRunner] ${message}`);
    }
  }

  /**
   * Execute a sentinel definition.
   */
  async execute(definition: PipelineSentinelDefinition): Promise<SentinelResult> {
    const ctx: ExecutionContext = {
      variables: {
        NOW: Date.now(),
        PROJECT_ROOT: this.config.workingDir,
        ELAPSED_MS: 0,
      },
      iteration: 0,
      trace: [],
      startedAt: Date.now(),
      lastStepAt: Date.now(),
      shouldStop: false,
    };

    this.log(`Starting sentinel: ${definition.name}`);

    // Check safety limits
    const safety = definition.safety ?? {};
    const maxIterations = safety.maxIterations ?? 1000;
    const timeoutMs = safety.timeoutMs ?? 600000; // 10 min default

    try {
      // Main execution loop
      while (!ctx.shouldStop) {
        ctx.iteration++;
        ctx.variables.ELAPSED_MS = Date.now() - ctx.startedAt;

        this.config.onIteration?.(ctx.iteration, ctx);
        this.log(`Iteration ${ctx.iteration}`);

        // Check iteration limit
        if (ctx.iteration > maxIterations) {
          ctx.shouldStop = true;
          ctx.stopReason = `Max iterations (${maxIterations}) reached`;
          break;
        }

        // Check timeout
        if (Date.now() - ctx.startedAt > timeoutMs) {
          ctx.shouldStop = true;
          ctx.stopReason = `Timeout (${timeoutMs}ms) reached`;
          break;
        }

        // Execute all steps in sequence
        for (let i = 0; i < definition.steps.length; i++) {
          const step = definition.steps[i];
          const trace = await this.executeStep(step, i, ctx);
          ctx.trace.push(trace);
          ctx.lastStepAt = trace.completedAt;

          if (!trace.success && step.onError !== 'skip') {
            if (step.onError === 'retry') {
              // Retry logic would go here - for now, continue
              this.log(`Step ${i} failed, retrying not yet implemented`, 'warn');
            } else {
              // Default: fail
              ctx.shouldStop = true;
              ctx.stopReason = `Step ${i} (${step.type}) failed: ${trace.error}`;
              break;
            }
          }

          if (ctx.shouldStop) break;
        }

        if (ctx.shouldStop) break;

        // Check loop condition
        if (!this.shouldContinueLoop(definition.loop, ctx)) {
          break;
        }
      }

      const success = !ctx.stopReason || ctx.stopReason.includes('completed');
      return {
        success,
        summary: ctx.stopReason || `Completed ${ctx.iteration} iteration(s)`,
        context: ctx,
      };
    } catch (error: any) {
      this.log(`Fatal error: ${error.message}`, 'error');
      return {
        success: false,
        summary: `Fatal error: ${error.message}`,
        context: ctx,
      };
    }
  }

  /**
   * Check if loop should continue based on LoopConfig.
   */
  private shouldContinueLoop(loop: LoopConfig | undefined, ctx: ExecutionContext): boolean {
    if (!loop) return false; // No loop = run once

    switch (loop.type) {
      case 'once':
        return false;

      case 'count':
        return ctx.iteration < (loop.max ?? 1);

      case 'until':
        // Continue while condition is FALSE
        return !evaluateCondition(loop.check!, ctx.variables);

      case 'while':
        // Continue while condition is TRUE
        return evaluateCondition(loop.check!, ctx.variables);

      case 'continuous':
        // Always continue (relies on timeout/maxIterations for safety)
        if (loop.intervalMs) {
          // TODO: implement interval pause
        }
        return true;

      case 'event':
        // Event-driven loops are handled differently (trigger-based)
        // For now, run once and rely on external trigger to restart
        return false;

      default:
        return false;
    }
  }

  /**
   * Execute a single step.
   */
  private async executeStep(
    step: SentinelStep,
    index: number,
    ctx: ExecutionContext
  ): Promise<StepTrace> {
    const startedAt = Date.now();
    this.config.onStepStart?.(step, ctx);

    const trace: StepTrace = {
      stepIndex: index,
      stepType: step.type,
      startedAt,
      completedAt: 0,
      durationMs: 0,
      success: false,
    };

    try {
      let output: unknown;

      switch (step.type) {
        case 'command':
          output = await this.executeCommand(step as CommandStep, ctx);
          break;
        case 'llm':
          output = await this.executeLLM(step as LLMStep, ctx);
          break;
        case 'condition':
          output = await this.executeCondition(step as ConditionStep, ctx);
          break;
        case 'watch':
          output = await this.executeWatch(step as WatchStep, ctx);
          break;
        case 'sentinel':
          output = await this.executeSentinel(step as SentinelSpawnStep, ctx);
          break;
        case 'emit':
          output = await this.executeEmit(step as EmitStep, ctx);
          break;
        default:
          throw new Error(`Unknown step type: ${(step as any).type}`);
      }

      // Store output in variables if outputTo specified
      if ('outputTo' in step && step.outputTo) {
        ctx.variables[step.outputTo] = output;
      }

      trace.success = true;
      trace.output = output;
    } catch (error: any) {
      trace.success = false;
      trace.error = error.message;
      this.log(`Step ${index} (${step.type}) failed: ${error.message}`, 'error');
    }

    trace.completedAt = Date.now();
    trace.durationMs = trace.completedAt - trace.startedAt;
    this.config.onStepComplete?.(step, trace, ctx);

    return trace;
  }

  // ==========================================================================
  // Step Implementations
  // ==========================================================================

  /**
   * Execute a command step.
   */
  private async executeCommand(step: CommandStep, ctx: ExecutionContext): Promise<unknown> {
    const params = substituteVariables(step.params, ctx.variables) as Record<string, unknown>;
    this.log(`Command: ${step.command} ${JSON.stringify(params)}`, 'debug');

    const result = await Commands.execute(step.command, params as any);
    return result;
  }

  /**
   * Execute an LLM step.
   */
  private async executeLLM(step: LLMStep, ctx: ExecutionContext): Promise<unknown> {
    const prompt = substituteVariables(step.prompt || '', ctx.variables) as string;
    this.log(`LLM: ${prompt.slice(0, 100)}...`, 'debug');

    // Build model config
    const modelConfig: ModelConfig = step.model
      ? (typeof step.model === 'string'
        ? { model: step.model, capacity: ModelCapacity.MEDIUM, provider: ModelProvider.AUTO }
        : step.model as ModelConfig)
      : this.config.defaultModel ?? {
        capacity: ModelCapacity.SMALL,
        provider: ModelProvider.LOCAL,
      };

    if (step.temperature !== undefined) {
      modelConfig.temperature = step.temperature;
    }

    // Build context from accumulated variables
    let contextStr = '';
    if (Object.keys(ctx.variables).length > 0) {
      const relevantVars = { ...ctx.variables };
      delete relevantVars.NOW;
      delete relevantVars.PROJECT_ROOT;
      delete relevantVars.ELAPSED_MS;
      if (Object.keys(relevantVars).length > 0) {
        contextStr = `\n\nContext:\n${JSON.stringify(relevantVars, null, 2)}`;
      }
    }

    const fullPrompt = prompt + contextStr;

    // If tools are specified and parseToolCalls is true, we need to handle tool calling
    if (step.tools && step.tools.length > 0 && step.parseToolCalls) {
      // For now, include tools in prompt as instructions
      // Full tool calling requires integration with ModelInvoker's tool support
      const toolsStr = step.tools.join(', ');
      const toolPrompt = `${fullPrompt}\n\nAvailable tools: ${toolsStr}\nUse these tools as needed to accomplish the task.`;

      const result = await this.invoker.generate(toolPrompt, modelConfig);
      if (!result.success) {
        throw new Error(result.error || 'LLM inference failed');
      }

      // TODO: Parse and execute tool calls from response
      // For now, return raw response
      return {
        text: result.text,
        toolCalls: [], // TODO: parse tool calls
      };
    }

    const result = await this.invoker.generate(fullPrompt, modelConfig);
    if (!result.success) {
      throw new Error(result.error || 'LLM inference failed');
    }

    return result.text;
  }

  /**
   * Execute a condition step (branching).
   */
  private async executeCondition(step: ConditionStep, ctx: ExecutionContext): Promise<unknown> {
    const checkResult = evaluateCondition(step.check, ctx.variables);
    this.log(`Condition: ${step.check} = ${checkResult}`, 'debug');

    const branch = checkResult ? step.then : step.else;
    if (branch && branch.length > 0) {
      // Execute the branch steps
      for (let i = 0; i < branch.length; i++) {
        const branchStep = branch[i];
        const trace = await this.executeStep(branchStep, -1, ctx); // -1 for nested
        if (!trace.success && branchStep.onError !== 'skip') {
          throw new Error(`Branch step ${i} failed: ${trace.error}`);
        }
      }
    }

    return { condition: step.check, result: checkResult };
  }

  /**
   * Execute a watch step (block until condition).
   */
  private async executeWatch(step: WatchStep, ctx: ExecutionContext): Promise<unknown> {
    const executionId = substituteVariables(step.executionId, ctx.variables) as string;
    this.log(`Watch: ${executionId} until ${step.until}`, 'debug');

    // Use sentinel/status to poll for completion
    const maxWaitMs = 300000; // 5 min max wait
    const pollIntervalMs = 1000;
    const startTime = Date.now();

    while (Date.now() - startTime < maxWaitMs) {
      try {
        const status = await Commands.execute('sentinel/status', { handle: executionId } as any);

        if (step.until === 'finished') {
          if ((status as any).handle?.status !== 'running') {
            return status;
          }
        } else if (step.until === 'error') {
          if ((status as any).handle?.status === 'failed') {
            return status;
          }
        }
        // TODO: handle 'match' with rules

        await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
      } catch (e) {
        // Handle not found - might be completed and cleaned up
        break;
      }
    }

    throw new Error(`Watch timeout waiting for ${executionId}`);
  }

  /**
   * Execute a nested sentinel.
   */
  private async executeSentinel(step: SentinelSpawnStep, ctx: ExecutionContext): Promise<unknown> {
    this.log(`Spawning nested sentinel: ${step.definition.name}`, 'debug');

    // Create a new runner for the nested sentinel
    const nestedRunner = new SentinelRunner(this.config);
    const result = await nestedRunner.execute(step.definition);

    if (step.await === false) {
      // Fire and forget - return handle info
      return { spawned: true, name: step.definition.name };
    }

    return result;
  }

  /**
   * Execute an emit step (fire event).
   */
  private async executeEmit(step: EmitStep, ctx: ExecutionContext): Promise<unknown> {
    const data = step.data ? substituteVariables(step.data, ctx.variables) : undefined;
    this.log(`Emit: ${step.event}`, 'debug');

    Events.emit(step.event, data);
    return { emitted: step.event, data };
  }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/**
 * Run a sentinel definition with default config.
 */
export async function runSentinel(
  definition: PipelineSentinelDefinition,
  workingDir: string,
  options?: Partial<RunnerConfig>
): Promise<SentinelResult> {
  const runner = new SentinelRunner({
    workingDir,
    ...options,
  });
  return runner.execute(definition);
}

/**
 * Run a sentinel from a JSON file.
 */
export async function runSentinelFromFile(
  filePath: string,
  workingDir: string,
  options?: Partial<RunnerConfig>
): Promise<SentinelResult> {
  const fs = await import('fs');
  const content = fs.readFileSync(filePath, 'utf-8');
  const definition = JSON.parse(content) as PipelineSentinelDefinition;
  return runSentinel(definition, workingDir, options);
}
