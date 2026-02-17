/**
 * Sentinel System - Pipeline Execution in Rust
 *
 * All sentinel execution happens in Rust SentinelModule.
 * This module exports types, definition utilities, and the entity class.
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
  type SentinelEntity,        // Data interface (used by commands for plain objects)
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

// Entity class (proper ORM entity for EntityRegistry + database schema)
// Commands use the SentinelEntity interface above for plain objects.
// EntityRegistry uses this class for decorator metadata / schema.
export { SentinelEntity as SentinelEntityClass } from './entities/SentinelEntity';
export {
  DEFAULT_ESCALATION_RULES,
  VALID_SENTINEL_STATUSES,
  type EscalationRule,
  type EscalationCondition,
  type EscalationAction,
  type EscalationPriority,
  type SentinelStatus,
} from './entities/SentinelEntity';

// Escalation service (sentinel lifecycle → persona inbox)
export {
  initializeSentinelEscalation,
  registerSentinelHandle,
  unregisterSentinelHandle,
} from './SentinelEscalationService';

// Trigger service (automatic sentinel execution: event, cron, immediate)
export {
  initializeSentinelTriggers,
  shutdownSentinelTriggers,
  getActiveTriggerCount,
  listActiveTriggers,
  parseCronSchedule,
} from './SentinelTriggerService';
