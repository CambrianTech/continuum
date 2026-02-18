/**
 * SentinelDefinition - Portable JSON Schema for Sentinels
 *
 * Every sentinel, whether created from CLI args, fluent builder, or JSON,
 * can export to this canonical format. Definitions can be:
 * - Saved to database as entities
 * - Loaded and re-run
 * - Shared between systems
 * - Versioned and tracked
 */

import type { ModelCapacity, ModelProvider } from './ModelProvider';

/**
 * Base definition shared by all sentinel types
 */
export interface SentinelDefinitionBase {
  /** Unique identifier (generated on save) */
  id?: string;

  /** Sentinel type discriminator */
  type: 'build' | 'orchestrate' | 'screenshot' | 'task' | 'script' | 'pipeline';

  /** Human-readable name */
  name: string;

  /** Description of what this sentinel does */
  description?: string;

  /** Working directory (defaults to cwd) */
  workingDir?: string;

  /** Timeout in milliseconds */
  timeout?: number;

  /** Tags for organization/search */
  tags?: string[];

  /** Schema version for forward compatibility */
  version: '1.0';

  /** When this definition was created */
  createdAt?: string;

  /** When this definition was last modified */
  updatedAt?: string;

  /** Who created this definition */
  createdBy?: string;
}

/**
 * BuildSentinel definition
 */
export interface BuildSentinelDefinition extends SentinelDefinitionBase {
  type: 'build';

  /** Build command to execute */
  command: string;

  /** Maximum fix attempts before escalation */
  maxAttempts?: number;

  /** Whether to attempt auto-fixes */
  canAutoFix?: boolean;

  /** Enable LLM-assisted error fixing when pattern matching fails */
  useLLM?: boolean;

  /** LLM model capacity for error analysis */
  llmCapacity?: ModelCapacity;

  /** LLM model provider */
  llmProvider?: ModelProvider;
}

/**
 * OrchestratorSentinel definition
 */
export interface OrchestrateSentinelDefinition extends SentinelDefinitionBase {
  type: 'orchestrate';

  /** The goal to accomplish */
  goal: string;

  /** Maximum iterations */
  maxIterations?: number;

  /** Model capacity level */
  capacity?: ModelCapacity;

  /** Model provider */
  provider?: ModelProvider;

  /** Specific model name (overrides capacity) */
  modelName?: string;

  /** Where to save screenshots */
  screenshotDir?: string;

  /** Available tools for the orchestrator */
  tools?: string[];
}

/**
 * ScreenshotSentinel definition
 */
export interface ScreenshotSentinelDefinition extends SentinelDefinitionBase {
  type: 'screenshot';

  /** URL or file path to screenshot */
  target: string;

  /** Output filename */
  filename?: string;

  /** Output directory */
  outputDir?: string;

  /** Viewport dimensions */
  viewport?: {
    width: number;
    height: number;
  };
}

/**
 * Task action within a TaskSentinel
 */
export interface TaskAction {
  /** Task name for progress tracking */
  name: string;

  /** Action type */
  action: 'write' | 'read' | 'run' | 'build' | 'sentinel';

  /** File path (for write/read) */
  file?: string;

  /** Content to write */
  content?: string;

  /** Command to run */
  command?: string;

  /** Nested sentinel definition (for action: 'sentinel') */
  sentinel?: SentinelDefinition;

  /** Condition to check before running (JavaScript expression) */
  condition?: string;

  /** Whether to continue on failure */
  continueOnError?: boolean;
}

/**
 * TaskSentinel definition - executes a sequence of actions
 */
export interface TaskSentinelDefinition extends SentinelDefinitionBase {
  type: 'task';

  /** Ordered list of tasks to execute */
  tasks: TaskAction[];

  /** Stop on first error (default: true) */
  stopOnError?: boolean;

  /** Maximum parallel tasks (default: 1 = sequential) */
  parallelism?: number;
}

/**
 * ScriptSentinel definition - pure script, no AI
 */
export interface ScriptSentinelDefinition extends SentinelDefinitionBase {
  type: 'script';

  /** Shell command or script path */
  script: string;

  /** Environment variables */
  env?: Record<string, string>;

  /** Script interpreter (default: /bin/sh) */
  interpreter?: string;
}

// ============================================================================
// Step-Based Pipeline Definitions (Declarative Runtime)
// ============================================================================

/**
 * Loop control for pipeline sentinels.
 */
export type LoopConfig =
  | { type: 'once' }                           // Run pipeline once
  | { type: 'count'; max: number }             // Run N iterations
  | { type: 'until'; check: string }           // Run until condition is true
  | { type: 'while'; check: string }           // Run while condition is true
  | { type: 'continuous'; intervalMs?: number } // Keep running
  | { type: 'event'; event: string };          // Re-run on each event

/**
 * Trigger configuration for automatic sentinel start.
 */
export type SentinelTrigger =
  | { type: 'immediate' }                      // Start now
  | { type: 'event'; event: string; debounceMs?: number; allowConcurrent?: boolean }  // Start on event
  | { type: 'cron'; schedule: string; debounceMs?: number; allowConcurrent?: boolean }  // Cron-like scheduling
  | { type: 'manual' };                        // Started by command

/**
 * Safety controls for sentinel execution.
 */
export interface SentinelSafety {
  maxIterations?: number;        // Hard limit on loop count
  timeoutMs?: number;            // Hard limit on total runtime
  maxStepTimeoutMs?: number;     // Per-step timeout
  maxMemoryMb?: number;          // Memory budget
  onTimeout?: 'stop' | 'pause';  // What to do when limits hit
}

/**
 * Base step interface.
 */
export interface StepBase {
  type: string;
  outputTo?: string;             // Variable name for result
  onError?: 'fail' | 'skip' | 'retry';
}

/**
 * Execute a command. Output stored in variables[outputTo].
 */
export interface CommandStep extends StepBase {
  type: 'command';
  command: string;                     // e.g., 'code/read', 'code/verify', 'data/list'
  params: Record<string, unknown>;     // Supports $variable references
}

/**
 * Run LLM inference. Accumulated variables injected as context.
 */
export interface LLMStep extends StepBase {
  type: 'llm';
  prompt?: string;                     // Template with $variable references
  model?: string | StepModelConfig;    // Model selection
  temperature?: number;
  tools?: string[];                    // Tool subset for this step
  parseToolCalls?: boolean;            // Extract and execute tool calls
}

/**
 * Model config for LLM steps (inline version).
 */
export interface StepModelConfig {
  capacity?: ModelCapacity;
  provider?: ModelProvider;
  model?: string;
  maxTokens?: number;
  temperature?: number;
}

/**
 * Block until classified output lines arrive.
 */
export interface WatchStep extends StepBase {
  type: 'watch';
  executionId: string;                 // $variable reference to running process
  rules?: SentinelRule[];              // Classification rules
  until?: 'finished' | 'error' | 'match';
}

/**
 * Classification rule for watch steps.
 */
export interface SentinelRule {
  pattern: string;                     // Regex pattern
  classification: string;              // Category name
  action?: 'Emit' | 'Log' | 'Ignore';
}

/**
 * Conditional branching.
 */
export interface ConditionStep extends StepBase {
  type: 'condition';
  check: string;                       // JS expression with $variable access
  then: SentinelStep[];                // Steps if true
  else?: SentinelStep[];               // Steps if false
}

/**
 * Spawn a nested sentinel (recursive composition).
 */
export interface SentinelSpawnStep extends StepBase {
  type: 'sentinel';
  definition: PipelineSentinelDefinition;  // Inline definition
  await?: boolean;                     // Wait for completion or fire-and-forget
}

/**
 * Emit an event (for cross-sentinel composition).
 */
export interface EmitStep extends StepBase {
  type: 'emit';
  event: string;                       // Event name
  data?: string;                       // $variable reference for payload
}

/**
 * Execute multiple steps in parallel (concurrent execution).
 */
export interface ParallelStep extends StepBase {
  type: 'parallel';
  steps: SentinelStep[];               // Steps to run concurrently
  failFast?: boolean;                  // Stop all on first failure (default: false)
}

/**
 * Union of all step types.
 */
export type SentinelStep =
  | CommandStep
  | LLMStep
  | WatchStep
  | ConditionStep
  | SentinelSpawnStep
  | EmitStep
  | ParallelStep;

/**
 * Pipeline-based sentinel definition (declarative, JSON-serializable).
 *
 * This is the target architecture for truly flexible AI-driven sentinels.
 * AIs can create, modify, and share these as pure data.
 */
export interface PipelineSentinelDefinition {
  /** Type discriminator for union compatibility */
  type: 'pipeline';

  /** Unique identifier (generated on save) */
  id?: string;

  /** Human-readable name */
  name: string;

  /** Description of what this sentinel does */
  description?: string;

  /** Working directory (defaults to cwd) */
  workingDir?: string;

  /** Schema version */
  version: '1.0';

  /** Timeout in milliseconds (for compatibility) */
  timeout?: number;

  /** RAG recipe ID for context building */
  recipe?: string;

  /** Explicit RAG sources (alternative to recipe) */
  ragSources?: string[];

  /** The pipeline steps */
  steps: SentinelStep[];

  /** Loop control */
  loop: LoopConfig;

  /** What triggers this sentinel */
  trigger?: SentinelTrigger;

  /** Safety controls */
  safety?: SentinelSafety;

  /** Available tools (highlights for LLM steps) */
  tools?: string[];

  /** Tags for organization */
  tags?: string[];

  /** Metadata */
  createdAt?: string;
  updatedAt?: string;
  createdBy?: string;
}

// ============================================================================
// Legacy Class-Based Definitions (for existing sentinels)
// ============================================================================

/**
 * Union of all sentinel definitions (legacy + pipeline).
 */
export type SentinelDefinition =
  | BuildSentinelDefinition
  | OrchestrateSentinelDefinition
  | ScreenshotSentinelDefinition
  | TaskSentinelDefinition
  | ScriptSentinelDefinition
  | PipelineSentinelDefinition;

/**
 * Sentinel execution result (saved alongside definition)
 */
export interface SentinelExecutionResult {
  /** Handle ID from execution */
  handle: string;

  /** Whether execution succeeded */
  success: boolean;

  /** When execution started */
  startedAt: string;

  /** When execution completed */
  completedAt?: string;

  /** Duration in milliseconds */
  durationMs?: number;

  /** Execution-specific data */
  data?: {
    summary?: string;
    filesCreated?: string[];
    filesModified?: string[];
    errors?: string[];
    screenshot?: string;
    attempts?: number;
    iterations?: number;
    output?: string;
  };

  /** Error message if failed */
  error?: string;
}

/**
 * SentinelEntity - Database-persisted sentinel with execution history
 *
 * This interface defines the data shape for sentinel persistence.
 * The EntityClass (SentinelEntityClass) provides decorator metadata for the ORM.
 * Commands construct plain objects matching this interface.
 */
export interface SentinelEntity {
  /** Entity ID (UUID) */
  id: string;

  /** The sentinel definition */
  definition: SentinelDefinition;

  /** Execution history (most recent first) */
  executions: SentinelExecutionResult[];

  /** Current lifecycle status */
  status?: 'saved' | 'running' | 'completed' | 'failed' | 'paused' | 'cancelled';

  /** Owning persona — every sentinel belongs to someone */
  parentPersonaId?: string;

  /** Current Rust-side handle ID (ephemeral, only valid while running) */
  activeHandle?: string;

  /** Entity metadata */
  createdAt: string;
  updatedAt: string;
  createdBy?: string;

  /** Whether this sentinel is a template (can be cloned) */
  isTemplate?: boolean;

  /** Parent template ID if cloned */
  parentId?: string;

  /** Tags for organization and search */
  tags?: string[];

  /** Escalation rules — when to notify the owning persona */
  escalationRules?: Array<{
    condition: 'error' | 'timeout' | 'unfamiliar' | 'approval_needed' | 'complete';
    action: 'pause' | 'notify' | 'abort';
    priority: 'low' | 'normal' | 'high' | 'urgent';
  }>;

  /** Aggregate: how many times executed */
  executionCount?: number;

  /** Last execution success/failure */
  lastSuccess?: boolean;

  /** Last execution timestamp */
  lastRunAt?: string;
}

/**
 * Validate a sentinel definition
 */
export function validateDefinition(def: SentinelDefinition): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (!def.type) {
    errors.push('Missing required field: type');
  }

  if (!def.name) {
    errors.push('Missing required field: name');
  }

  if (def.version !== '1.0') {
    errors.push(`Unsupported version: ${def.version}`);
  }

  switch (def.type) {
    case 'build':
      if (!(def as BuildSentinelDefinition).command) {
        errors.push('BuildSentinel requires command');
      }
      break;

    case 'orchestrate':
      if (!(def as OrchestrateSentinelDefinition).goal) {
        errors.push('OrchestrateSentinel requires goal');
      }
      break;

    case 'screenshot':
      if (!(def as ScreenshotSentinelDefinition).target) {
        errors.push('ScreenshotSentinel requires target');
      }
      break;

    case 'task':
      const taskDef = def as TaskSentinelDefinition;
      if (!taskDef.tasks || taskDef.tasks.length === 0) {
        errors.push('TaskSentinel requires at least one task');
      }
      break;

    case 'script':
      if (!(def as ScriptSentinelDefinition).script) {
        errors.push('ScriptSentinel requires script');
      }
      break;

    case 'pipeline':
      const pipelineDef = def as PipelineSentinelDefinition;
      if (!pipelineDef.steps || pipelineDef.steps.length === 0) {
        errors.push('PipelineSentinel requires at least one step');
      }
      if (!pipelineDef.loop) {
        errors.push('PipelineSentinel requires loop config');
      }
      break;

    default:
      errors.push(`Unknown sentinel type: ${(def as any).type}`);
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Create a definition from CLI-style params
 */
export function createDefinitionFromParams(params: Record<string, any>): SentinelDefinition {
  const base: Partial<SentinelDefinitionBase> = {
    name: params.name || `${params.type}-sentinel-${Date.now()}`,
    description: params.description,
    workingDir: params.workingDir,
    timeout: params.timeout,
    tags: params.tags,
    version: '1.0',
    createdAt: new Date().toISOString(),
  };

  switch (params.type) {
    case 'build':
      return {
        ...base,
        type: 'build',
        command: params.command,
        maxAttempts: params.maxAttempts,
        canAutoFix: params.canAutoFix,
        useLLM: params.useLLM,
        llmCapacity: params.llmCapacity || params.capacity,
        llmProvider: params.llmProvider || params.provider,
      } as BuildSentinelDefinition;

    case 'orchestrate':
      return {
        ...base,
        type: 'orchestrate',
        goal: params.goal,
        maxIterations: params.maxIterations,
        capacity: params.capacity,
        provider: params.provider,
        modelName: params.modelName,
        screenshotDir: params.screenshotDir,
      } as OrchestrateSentinelDefinition;

    case 'screenshot':
      return {
        ...base,
        type: 'screenshot',
        target: params.target,
        filename: params.filename,
        outputDir: params.outputDir,
        viewport: params.viewport,
      } as ScreenshotSentinelDefinition;

    case 'task':
      return {
        ...base,
        type: 'task',
        tasks: params.tasks,
        stopOnError: params.stopOnError,
        parallelism: params.parallelism,
      } as TaskSentinelDefinition;

    case 'script':
      return {
        ...base,
        type: 'script',
        script: params.script,
        env: params.env,
        interpreter: params.interpreter,
      } as ScriptSentinelDefinition;

    case 'pipeline':
      return {
        ...base,
        type: 'pipeline',
        steps: params.steps || [],
        loop: params.loop || { type: 'once' },
        trigger: params.trigger,
        safety: params.safety,
        recipe: params.recipe,
        ragSources: params.ragSources,
        tools: params.tools,
      } as PipelineSentinelDefinition;

    default:
      throw new Error(`Unknown sentinel type: ${params.type}`);
  }
}

/**
 * Fluent builder for creating sentinel definitions
 */
export class SentinelBuilder {
  private def: Partial<SentinelDefinition> = { version: '1.0' };

  static build(command: string): SentinelBuilder {
    return new SentinelBuilder().type('build').set('command', command);
  }

  static orchestrate(goal: string): SentinelBuilder {
    return new SentinelBuilder().type('orchestrate').set('goal', goal);
  }

  static screenshot(target: string): SentinelBuilder {
    return new SentinelBuilder().type('screenshot').set('target', target);
  }

  static task(): SentinelBuilder {
    return new SentinelBuilder().type('task').set('tasks', []);
  }

  static script(script: string): SentinelBuilder {
    return new SentinelBuilder().type('script').set('script', script);
  }

  static pipeline(): SentinelBuilder {
    return new SentinelBuilder().type('pipeline').set('steps', []).set('loop', { type: 'once' });
  }

  type(t: SentinelDefinition['type']): this {
    this.def.type = t;
    return this;
  }

  name(n: string): this {
    this.def.name = n;
    return this;
  }

  description(d: string): this {
    this.def.description = d;
    return this;
  }

  workingDir(dir: string): this {
    this.def.workingDir = dir;
    return this;
  }

  timeout(ms: number): this {
    this.def.timeout = ms;
    return this;
  }

  tags(...t: string[]): this {
    this.def.tags = t;
    return this;
  }

  set(key: string, value: any): this {
    (this.def as any)[key] = value;
    return this;
  }

  // Task-specific methods
  addTask(task: TaskAction): this {
    if (this.def.type !== 'task') {
      throw new Error('addTask only valid for task sentinels');
    }
    (this.def as TaskSentinelDefinition).tasks.push(task);
    return this;
  }

  write(name: string, file: string, content: string): this {
    return this.addTask({ name, action: 'write', file, content });
  }

  read(name: string, file: string): this {
    return this.addTask({ name, action: 'read', file });
  }

  run(name: string, command: string): this {
    return this.addTask({ name, action: 'run', command });
  }

  // Build to final definition
  toDefinition(): SentinelDefinition {
    if (!this.def.name) {
      this.def.name = `${this.def.type}-${Date.now()}`;
    }
    this.def.createdAt = new Date().toISOString();

    const validation = validateDefinition(this.def as SentinelDefinition);
    if (!validation.valid) {
      throw new Error(`Invalid sentinel definition: ${validation.errors.join(', ')}`);
    }

    return this.def as SentinelDefinition;
  }

  toJSON(): string {
    return JSON.stringify(this.toDefinition(), null, 2);
  }
}
