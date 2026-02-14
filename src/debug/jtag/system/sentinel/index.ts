/**
 * Sentinel System - Autonomous Task Executors
 *
 * ARCHITECTURE:
 *
 * 1. SCRIPT SENTINELS (no AI required)
 *    - BuildSentinel: Agentic compilation with auto-fix
 *    - VisualSentinel: Screenshot feedback via Puppeteer
 *    - TaskSentinel: Serial task execution with limits
 *
 * 2. AI-POWERED SENTINELS (require LLM)
 *    - OrchestratorSentinel: LLM-powered planning & execution
 *
 * MODEL SELECTION EXAMPLES:
 *
 * ```typescript
 * // By capacity (power level)
 * new OrchestratorSentinel({
 *   workingDir: '...',
 *   capacity: ModelCapacity.SMALL,   // TINY | SMALL | MEDIUM | LARGE | SOTA
 * });
 *
 * // By provider
 * new OrchestratorSentinel({
 *   workingDir: '...',
 *   provider: ModelProvider.LOCAL,   // LOCAL | OLLAMA | ANTHROPIC | OPENAI | AUTO
 * });
 *
 * // By specific model name
 * new OrchestratorSentinel({
 *   workingDir: '...',
 *   modelName: 'claude-3-opus-20240229',
 * });
 *
 * // Full config
 * new OrchestratorSentinel({
 *   workingDir: '...',
 *   model: {
 *     capacity: ModelCapacity.LARGE,
 *     provider: ModelProvider.ANTHROPIC,
 *     model: 'claude-3-5-sonnet-20241022',
 *     maxTokens: 4000,
 *   }
 * });
 * ```
 */

// Base classes and types
export { ScriptSentinel, AISentinel, SentinelRegistry } from './Sentinel';
export type { SentinelResult, SentinelStep, BaseSentinelConfig, AISentinelConfig } from './Sentinel';

// Model selection
export { ModelCapacity, ModelProvider, ModelSelector, ModelInvoker, createInvoker, resolveModel } from './ModelProvider';
export type { ModelConfig, InferenceResult } from './ModelProvider';

// Script sentinels (no AI required)
export { BuildSentinel, type BuildSentinelConfig, type BuildResult, type BuildError, type SentinelProgress } from './BuildSentinel';
export { VisualSentinel, type VisualSentinelConfig, type ScreenshotResult } from './VisualSentinel';
export { TaskSentinel, createSnakeGamePlan, type TaskSentinelConfig, type Task, type TaskResult } from './TaskSentinel';

// AI-powered sentinels
export { OrchestratorSentinel, type OrchestratorConfig, type ExecutionContext, type HistoryEntry } from './OrchestratorSentinel';

// Portable definitions (JSON-serializable)
export {
  SentinelBuilder,
  validateDefinition,
  createDefinitionFromParams,
  type SentinelDefinition,
  type SentinelDefinitionBase,
  type BuildSentinelDefinition,
  type OrchestrateSentinelDefinition,
  type ScreenshotSentinelDefinition,
  type TaskSentinelDefinition,
  type ScriptSentinelDefinition,
  type TaskAction,
  type SentinelEntity,
  type SentinelExecutionResult,
  // Step-based pipeline types
  type PipelineSentinelDefinition,
  type LoopConfig,
  type SentinelTrigger,
  type SentinelSafety,
  type SentinelStep as PipelineStep,
  type CommandStep,
  type LLMStep,
  type ConditionStep,
  type WatchStep,
  type SentinelSpawnStep,
  type EmitStep,
  type ParallelStep,
  type SentinelRule,
} from './SentinelDefinition';

// Event trigger management
export {
  SentinelTriggerManager,
  getTriggerManager,
  registerTrigger,
  unregisterTrigger,
} from './SentinelTrigger';

// Declarative step engine (the future!)
export {
  SentinelRunner,
  runSentinel,
  runSentinelFromFile,
  type ExecutionContext as RunnerExecutionContext,
  type StepTrace,
  type SentinelResult as RunnerResult,
  type RunnerConfig,
} from './SentinelRunner';

// Tool result → memory capture
export {
  initToolResultMemoryCapture,
  queryToolMemories,
  getToolMemoryStats,
} from './ToolResultMemoryCapture';
