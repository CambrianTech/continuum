/**
 * Sentinel System - Pipeline Execution in Rust
 *
 * All sentinel execution happens in Rust SentinelModule.
 * This module only exports types and definition utilities.
 */

// Model selection types
export { ModelCapacity, ModelProvider } from './ModelProvider';
export type { ModelConfig } from './ModelProvider';

// Portable definitions (JSON-serializable)
export {
  SentinelBuilder,
  validateDefinition,
  type SentinelDefinition,
  type SentinelDefinitionBase,
  type SentinelEntity,
  type SentinelExecutionResult,
  // Pipeline types
  type PipelineSentinelDefinition,
  type LoopConfig,
  type SentinelTrigger,
  type SentinelSafety,
  type SentinelStep,
  type CommandStep,
  type LLMStep,
  type ConditionStep,
  type WatchStep,
  type SentinelSpawnStep,
  type EmitStep,
  type ParallelStep,
  type SentinelRule,
} from './SentinelDefinition';
