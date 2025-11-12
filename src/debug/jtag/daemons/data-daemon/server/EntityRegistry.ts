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
import { TrainingSessionEntity } from '../../../system/data/entities/TrainingSessionEntity';
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
  new TrainingSessionEntity();
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

  registerEntity(UserEntity.collection, UserEntity);
  registerEntity(RoomEntity.collection, RoomEntity);
  // Enable ChatMessage relational ORM with individual columns for optimal querying
  registerEntity(ChatMessageEntity.collection, ChatMessageEntity);
  registerEntity(TrainingSessionEntity.collection, TrainingSessionEntity);
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

  console.log('✅ EntityRegistry: All entities registered');
}