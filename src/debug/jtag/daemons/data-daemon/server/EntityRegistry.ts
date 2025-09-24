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

  registerEntity(UserEntity.collection, UserEntity);
  registerEntity(RoomEntity.collection, RoomEntity);
  // Enable ChatMessage relational ORM with individual columns for optimal querying
  registerEntity(ChatMessageEntity.collection, ChatMessageEntity);

  console.log('✅ EntityRegistry: All entities registered');
}