/**
 * Entity Registry - Server-side entity registration system
 *
 * This file imports all entities and registers them with the storage adapter
 * Only runs on server side to avoid cross-environment import issues
 */

/**
 * Entity Class Registry - Maps collection names to entity classes
 * This is dynamically populated as entities are registered
 */
type EntityConstructor = new (...args: any[]) => any;
export const ENTITY_REGISTRY = new Map<string, EntityConstructor>();

import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('EntityRegistry', 'sql');

/**
 * Register an entity class with its collection name
 * Called automatically when entity classes are imported/loaded
 */
export function registerEntity(collectionName: string, entityClass: EntityConstructor): void {
  log.debug(`Registering entity ${collectionName} -> ${entityClass.name}`);
  ENTITY_REGISTRY.set(collectionName, entityClass);
}

/**
 * Get registered entity class for a collection
 * Used by data/schema command for schema introspection
 */
export function getRegisteredEntity(collectionName: string): EntityConstructor | undefined {
  return ENTITY_REGISTRY.get(collectionName);
}

/**
 * Export EntityConstructor type for use in other files
 */
export type { EntityConstructor };
import { UserEntity } from '../../../system/data/entities/UserEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { TrainingSessionEntity as AcademyTrainingSessionEntity } from '../../../system/data/entities/TrainingSessionEntity';
import { TrainingSessionEntity as FineTuningTrainingSessionEntity } from '../shared/entities/TrainingSessionEntity';
import { UserStateEntity } from '../../../system/data/entities/UserStateEntity';
import { ContentTypeEntity } from '../../../system/data/entities/ContentTypeEntity';
import { RecipeEntity } from '../../../system/data/entities/RecipeEntity';
import { GenomeEntity } from '../../../system/genome/entities/GenomeEntity';
import { GenomeLayerEntity } from '../../../system/genome/entities/GenomeLayerEntity';
import { AIGenerationEntity } from '../../../system/data/entities/AIGenerationEntity';
import { TaskEntity } from '../../../system/data/entities/TaskEntity';
import { TestExecutionEntity } from '../shared/entities/TestExecutionEntity';
import { DatasetExecutionEntity } from '../shared/entities/DatasetExecutionEntity';
import { TrainingDatasetEntity } from '../shared/entities/TrainingDatasetEntity';
import { TrainingExampleEntity } from '../shared/entities/TrainingExampleEntity';
import { CoordinationDecisionEntity } from '../../../system/data/entities/CoordinationDecisionEntity';
import { CodeIndexEntity } from '../../../system/data/entities/CodeIndexEntity';
import { FineTuningDatasetEntity } from '../shared/entities/FineTuningDatasetEntity';
import { FineTuningJobEntity } from '../shared/entities/FineTuningJobEntity';
import { FineTunedModelEntity } from '../shared/entities/FineTunedModelEntity';
import { CognitionStateEntity } from '../../../system/data/entities/CognitionStateEntity';
import { CognitionPlanEntity } from '../../../system/data/entities/CognitionPlanEntity';
import { ToolExecutionLogEntity } from '../../../system/data/entities/ToolExecutionLogEntity';
import { AdapterDecisionLogEntity } from '../../../system/data/entities/AdapterDecisionLogEntity';
import { ResponseGenerationLogEntity } from '../../../system/data/entities/ResponseGenerationLogEntity';
import { CognitionPlanStepExecutionEntity } from '../../../system/data/entities/CognitionPlanStepExecutionEntity';
import { CognitionSelfStateUpdateEntity } from '../../../system/data/entities/CognitionSelfStateUpdateEntity';
import { CognitionMemoryOperationEntity } from '../../../system/data/entities/CognitionMemoryOperationEntity';
import { AdapterReasoningLogEntity } from '../../../system/data/entities/AdapterReasoningLogEntity';
import { CognitionPlanReplanEntity } from '../../../system/data/entities/CognitionPlanReplanEntity';
import { FileVoteProposalEntity } from '../../../system/data/entities/FileVoteProposalEntity';
import { DecisionProposalEntity } from '../../../system/data/entities/DecisionProposalEntity';
import { MemoryEntity } from '../../../system/data/entities/MemoryEntity';
import { WallDocumentEntity } from '../../../system/data/entities/WallDocumentEntity';
import { DecisionEntity } from '../../../system/data/entities/DecisionEntity';
import { SystemConfigEntity } from '../../../system/data/entities/SystemConfigEntity';
import { ActivityEntity } from '../../../system/data/entities/ActivityEntity';
import { CanvasStrokeEntity } from '../../../system/data/entities/CanvasStrokeEntity';
import { PersonaRAGContextEntity } from '../../../system/data/entities/PersonaRAGContextEntity';
import { TimelineEventEntity } from '../../../system/data/entities/TimelineEventEntity';
import { FeedbackEntity } from '../../../system/data/entities/FeedbackEntity';
import { CallEntity } from '../../../system/data/entities/CallEntity';
import { SocialCredentialEntity } from '../../../system/social/shared/SocialCredentialEntity';
import { HandleEntity } from '../../../system/data/entities/HandleEntity';
import { CodingPlanEntity } from '../../../system/data/entities/CodingPlanEntity';
import { SkillEntity } from '../../../system/data/entities/SkillEntity';

/**
 * Initialize entity registration for the storage adapter
 * Called during server startup to register all known entities
 */
export function initializeEntityRegistry(): void {
  log.info('Registering all known entities...');

  // Initialize decorators by creating instances (required for Stage 3 decorators)
  log.debug('Initializing decorator metadata...');
  new UserEntity();
  new RoomEntity();
  new ChatMessageEntity();
  new AcademyTrainingSessionEntity();
  new FineTuningTrainingSessionEntity();
  new UserStateEntity();
  new ContentTypeEntity();
  new RecipeEntity();
  new GenomeEntity();
  new GenomeLayerEntity();
  new AIGenerationEntity();
  new TaskEntity();
  new TestExecutionEntity();
  new DatasetExecutionEntity();
  new TrainingDatasetEntity();
  new TrainingExampleEntity();
  new CoordinationDecisionEntity();
  new CodeIndexEntity();
  new FineTuningDatasetEntity();
  new FineTuningJobEntity();
  new FineTunedModelEntity();
  new CognitionStateEntity();
  new CognitionPlanEntity();
  new ToolExecutionLogEntity();
  new AdapterDecisionLogEntity();
  new ResponseGenerationLogEntity();
  new CognitionPlanStepExecutionEntity();
  new CognitionSelfStateUpdateEntity();
  new CognitionMemoryOperationEntity();
  new AdapterReasoningLogEntity();
  new CognitionPlanReplanEntity();
  new FileVoteProposalEntity();
  new DecisionProposalEntity();
  new MemoryEntity();
  new WallDocumentEntity();
  new DecisionEntity();
  new SystemConfigEntity();
  new ActivityEntity();
  new CanvasStrokeEntity();
  new PersonaRAGContextEntity();
  new TimelineEventEntity();
  new FeedbackEntity();
  new CallEntity();
  new SocialCredentialEntity();
  new HandleEntity();
  new CodingPlanEntity();
  new SkillEntity();

  registerEntity(UserEntity.collection, UserEntity);
  registerEntity(RoomEntity.collection, RoomEntity);
  // Enable ChatMessage relational ORM with individual columns for optimal querying
  registerEntity(ChatMessageEntity.collection, ChatMessageEntity);
  registerEntity(AcademyTrainingSessionEntity.collection, AcademyTrainingSessionEntity);
  registerEntity(FineTuningTrainingSessionEntity.collection, FineTuningTrainingSessionEntity);
  registerEntity(UserStateEntity.collection, UserStateEntity);
  registerEntity(ContentTypeEntity.collection, ContentTypeEntity);
  registerEntity(RecipeEntity.collection, RecipeEntity);
  registerEntity(GenomeEntity.collection, GenomeEntity);
  registerEntity(GenomeLayerEntity.collection, GenomeLayerEntity);
  registerEntity(AIGenerationEntity.collection, AIGenerationEntity);
  registerEntity(TaskEntity.collection, TaskEntity);
  registerEntity(TestExecutionEntity.collection, TestExecutionEntity);
  registerEntity(DatasetExecutionEntity.collection, DatasetExecutionEntity);
  registerEntity(TrainingDatasetEntity.collection, TrainingDatasetEntity);
  registerEntity(TrainingExampleEntity.collection, TrainingExampleEntity);
  registerEntity(CoordinationDecisionEntity.collection, CoordinationDecisionEntity);
  registerEntity(CodeIndexEntity.collection, CodeIndexEntity);
  registerEntity(FineTuningDatasetEntity.collection, FineTuningDatasetEntity);
  registerEntity(FineTuningJobEntity.collection, FineTuningJobEntity);
  registerEntity(FineTunedModelEntity.collection, FineTunedModelEntity);
  registerEntity(CognitionStateEntity.collection, CognitionStateEntity);
  registerEntity(CognitionPlanEntity.collection, CognitionPlanEntity);
  registerEntity(ToolExecutionLogEntity.collection, ToolExecutionLogEntity);
  registerEntity(AdapterDecisionLogEntity.collection, AdapterDecisionLogEntity);
  registerEntity(ResponseGenerationLogEntity.collection, ResponseGenerationLogEntity);
  registerEntity(CognitionPlanStepExecutionEntity.collection, CognitionPlanStepExecutionEntity);
  registerEntity(CognitionSelfStateUpdateEntity.collection, CognitionSelfStateUpdateEntity);
  registerEntity(CognitionMemoryOperationEntity.collection, CognitionMemoryOperationEntity);
  registerEntity(AdapterReasoningLogEntity.collection, AdapterReasoningLogEntity);
  registerEntity(CognitionPlanReplanEntity.collection, CognitionPlanReplanEntity);
  registerEntity(FileVoteProposalEntity.collection, FileVoteProposalEntity);
  registerEntity(DecisionProposalEntity.collection, DecisionProposalEntity);
  registerEntity(MemoryEntity.collection, MemoryEntity);
  registerEntity(WallDocumentEntity.collection, WallDocumentEntity);
  registerEntity(DecisionEntity.collection, DecisionEntity);
  registerEntity(SystemConfigEntity.collection, SystemConfigEntity);
  registerEntity(ActivityEntity.collection, ActivityEntity);
  registerEntity(CanvasStrokeEntity.collection, CanvasStrokeEntity);
  registerEntity(PersonaRAGContextEntity.collection, PersonaRAGContextEntity);
  registerEntity(TimelineEventEntity.collection, TimelineEventEntity);
  registerEntity(FeedbackEntity.collection, FeedbackEntity);
  registerEntity(CallEntity.collection, CallEntity);
  registerEntity(SocialCredentialEntity.collection, SocialCredentialEntity);
  registerEntity(HandleEntity.collection, HandleEntity);
  registerEntity(CodingPlanEntity.collection, CodingPlanEntity);
  registerEntity(SkillEntity.collection, SkillEntity);

  log.info('All entities registered');
}