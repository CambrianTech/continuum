/**
 * Entity Registry - Server-side entity registration system
 *
 * This file imports all entities and registers them with the storage adapter
 * Only runs on server side to avoid cross-environment import issues
 */

import { registerEntity } from './SqliteStorageAdapter';
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

/**
 * Initialize entity registration for the storage adapter
 * Called during server startup to register all known entities
 */
export function initializeEntityRegistry(): void {
  console.log('🏷️ EntityRegistry: Registering all known entities...');

  // Initialize decorators by creating instances (required for Stage 3 decorators)
  console.log('🔧 EntityRegistry: Initializing decorator metadata...');
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

  console.log('✅ EntityRegistry: All entities registered');
}