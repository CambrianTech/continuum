/**
 * Generated Collection Constants
 *
 * ⚠️  AUTO-GENERATED - DO NOT EDIT MANUALLY
 * Source of truth: Entity files with `static readonly collection`
 * Generator: generator/generate-collection-constants.ts
 *
 * Run: npx tsx generator/generate-collection-constants.ts
 */

/**
 * Collection name constants - use these instead of hardcoded strings
 * TypeScript will catch any typos at compile time
 */
export const COLLECTIONS = {
  /** From ActivityEntity */
  ACTIVITIES: 'activities' as const,
  /** From AdapterDecisionLogEntity */
  ADAPTER_DECISION_LOGS: 'adapter_decision_logs' as const,
  /** From AdapterReasoningLogEntity */
  ADAPTER_REASONING_LOGS: 'adapter_reasoning_logs' as const,
  /** From AIGenerationEntity */
  AI_GENERATIONS: 'ai_generations' as const,
  /** From CallEntity */
  CALLS: 'calls' as const,
  /** From CanvasStrokeEntity */
  CANVAS_STROKES: 'canvas_strokes' as const,
  /** From ChatMessageEntity */
  CHAT_MESSAGES: 'chat_messages' as const,
  /** From CodeIndexEntity */
  CODE_INDEX: 'code_index' as const,
  /** From CognitionMemoryOperationEntity */
  COGNITION_MEMORY_OPERATIONS: 'cognition_memory_operations' as const,
  /** From CognitionPlanEntity */
  COGNITION_PLAN_RECORDS: 'cognition_plan_records' as const,
  /** From CognitionPlanReplanEntity */
  COGNITION_PLAN_REPLANS: 'cognition_plan_replans' as const,
  /** From CognitionPlanStepExecutionEntity */
  COGNITION_PLAN_STEP_EXECUTIONS: 'cognition_plan_step_executions' as const,
  /** From CognitionSelfStateUpdateEntity */
  COGNITION_SELF_STATE_UPDATES: 'cognition_self_state_updates' as const,
  /** From CognitionStateEntity */
  COGNITION_STATE_SNAPSHOTS: 'cognition_state_snapshots' as const,
  /** From ContentTypeEntity */
  CONTENTTYPE: 'ContentType' as const,
  /** From CoordinationDecisionEntity */
  COORDINATION_DECISIONS: 'coordination_decisions' as const,
  /** From DatasetExecutionEntity */
  DATASET_EXECUTIONS: 'dataset_executions' as const,
  /** From DecisionProposalEntity */
  DECISION_PROPOSALS: 'decision_proposals' as const,
  /** From DecisionEntity */
  DECISIONS: 'decisions' as const,
  /** From FeedbackEntity */
  FEEDBACK_PATTERNS: 'feedback_patterns' as const,
  /** From FileVoteProposalEntity */
  FILE_VOTE_PROPOSALS: 'file_vote_proposals' as const,
  /** From FineTunedModelEntity */
  FINE_TUNED_MODELS: 'fine_tuned_models' as const,
  /** From FineTuningDatasetEntity */
  FINE_TUNING_DATASETS: 'fine_tuning_datasets' as const,
  /** From FineTuningJobEntity */
  FINE_TUNING_JOBS: 'fine_tuning_jobs' as const,
  /** From GenomeLayerEntity */
  GENOME_LAYERS: 'genome_layers' as const,
  /** From GenomeEntity */
  GENOMES: 'genomes' as const,
  /** From HandleEntity */
  HANDLES: 'handles' as const,
  /** From MemoryEntity */
  MEMORIES: 'memories' as const,
  /** From PersonaRAGContextEntity */
  PERSONA_RAG_CONTEXTS: 'persona_rag_contexts' as const,
  /** From PinnedItemEntity */
  PINNED_ITEMS: 'pinned_items' as const,
  /** From RecipeEntity */
  RECIPES: 'recipes' as const,
  /** From ResponseGenerationLogEntity */
  RESPONSE_GENERATION_LOGS: 'response_generation_logs' as const,
  /** From RoomEntity */
  ROOMS: 'rooms' as const,
  /** From SkillEntity */
  SKILLS: 'skills' as const,
  /** From SocialCredentialEntity */
  SOCIAL_CREDENTIALS: 'social_credentials' as const,
  /** From SystemCheckpointEntity */
  SYSTEM_CHECKPOINTS: 'system_checkpoints' as const,
  /** From SystemConfigEntity */
  SYSTEM_CONFIG: 'system_config' as const,
  /** From TaskEntity */
  TASKS: 'tasks' as const,
  /** From TestExecutionEntity */
  TEST_EXECUTIONS: 'test_executions' as const,
  /** From TimelineEventEntity */
  TIMELINE_EVENTS: 'timeline_events' as const,
  /** From ToolExecutionLogEntity */
  TOOL_EXECUTION_LOGS: 'tool_execution_logs' as const,
  /** From TrainingCheckpointEntity */
  TRAINING_CHECKPOINTS: 'training_checkpoints' as const,
  /** From TrainingDatasetEntity */
  TRAINING_DATASETS: 'training_datasets' as const,
  /** From TrainingExampleEntity */
  TRAINING_EXAMPLES: 'training_examples' as const,
  /** From TrainingLogEntity */
  TRAINING_LOGS: 'training_logs' as const,
  /** From TrainingMetricsEntity */
  TRAINING_METRICS: 'training_metrics' as const,
  /** From TrainingSessionEntity */
  TRAINING_SESSIONS: 'training_sessions' as const,
  /** From TrainingSessionEntity */
  TRAININGSESSION: 'TrainingSession' as const,
  /** From UIPreferencesEntity */
  UIPREFERENCES: 'UIPreferences' as const,
  /** From UserStateEntity */
  USER_STATES: 'user_states' as const,
  /** From UserProfileEntity */
  USERPROFILE: 'UserProfile' as const,
  /** From UserEntity */
  USERS: 'users' as const,
  /** From WallDocumentEntity */
  WALL_DOCUMENTS: 'wall_documents' as const,
  /** From WebhookEventEntity */
  WEBHOOK_EVENTS: 'webhook_events' as const,
} as const;

/**
 * Type-safe collection name - use this in ORM method signatures
 * Prevents passing arbitrary strings as collection names
 */
export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];

/**
 * Collection constant keys - for programmatic access
 */
export type CollectionKey = keyof typeof COLLECTIONS;

/**
 * Validate a string is a valid collection name (runtime check)
 */
export function isValidCollection(name: string): name is CollectionName {
  return Object.values(COLLECTIONS).includes(name as CollectionName);
}

/**
 * Get all collection names as array
 */
export function getAllCollections(): CollectionName[] {
  return Object.values(COLLECTIONS);
}
