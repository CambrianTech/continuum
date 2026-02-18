/**
 * Persona Test Helpers
 *
 * Type-safe helper functions for persona integration tests
 * Following patterns from crud-db-widget.test.ts
 */

import { randomUUID } from 'crypto';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataDeleteParams, DataDeleteResult } from '../../../commands/data/delete/shared/DataDeleteTypes';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { GenomeLayerEntity } from '../../../system/genome/entities/GenomeLayerEntity';
import type { GenomeEntity } from '../../../system/genome/entities/GenomeEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import { ChatSend } from '../../../commands/collaboration/chat/send/shared/ChatSendTypes';
import { DataDelete } from '../../../commands/data/delete/shared/DataDeleteTypes';
// Track test data for cleanup
const testRoomIds: UUID[] = [];
const testPersonaIds: UUID[] = [];
const testMessageIds: UUID[] = [];
const testGenomeLayerIds: UUID[] = [];
const testGenomeIds: UUID[] = [];

/**
 * Create test room with members
 */
export async function createTestRoom(
  name: string,
  memberNames: string[] = []
): Promise<RoomEntity> {
  const roomId = randomUUID();

  const result = await DataCreate.execute({
    collection: 'rooms',
    data: {
      id: roomId,
      name,
      description: `Test room: ${name}`,
      members: memberNames.map(name => ({ userId: name, role: 'member' as const })),
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test room: ${result.error ?? 'unknown error'}`);
  }

  testRoomIds.push(roomId);
  return result.data as RoomEntity;
}

/**
 * Create test persona (AI user)
 */
export async function createTestPersona(
  username: string,
  options: {
    displayName?: string;
    bio?: string;
    autoResponds?: boolean;
  } = {}
): Promise<UserEntity> {
  const personaId = randomUUID();

  const result = await DataCreate.execute({
    collection: 'users',
    data: {
      id: personaId,
      username,
      displayName: options.displayName ?? username,
      type: 'persona',
      profile: {
        bio: options.bio ?? 'A test AI persona'
      },
      capabilities: {
        autoResponds: options.autoResponds ?? true
      },
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test persona: ${result.error ?? 'unknown error'}`);
  }

  testPersonaIds.push(personaId);
  return result.data as UserEntity;
}

/**
 * Seed conversation history
 */
export async function seedConversationHistory(
  roomId: UUID,
  messages: Array<{
    sender: string;
    text: string;
    timestamp?: number;
  }>
): Promise<ChatMessageEntity[]> {
  const results: ChatMessageEntity[] = [];

  for (const [index, msg] of messages.entries()) {
    const messageId = randomUUID();
    const timestamp = msg.timestamp ?? Date.now() + index * 1000;

    const result = await DataCreate.execute({
      collection: 'chat_messages',
      data: {
        id: messageId,
        roomId,
        senderId: msg.sender,
        senderName: msg.sender,
        content: { text: msg.text },
        timestamp: new Date(timestamp)
      }
    });

    if (!result.success) {
      throw new Error(`Failed to seed message: ${result.error ?? 'unknown error'}`);
    }

    testMessageIds.push(messageId);
    results.push(result.data as ChatMessageEntity);
  }

  return results;
}

/**
 * Send message to room (via chat/send command)
 */
export async function sendMessage(
  roomId: UUID,
  sender: string,
  text: string
): Promise<ChatMessageEntity> {
  const result = await ChatSend.execute({
    roomId,
    senderId: sender,
    content: { text }
  });

  if (!result.success) {
    throw new Error(`Failed to send message: ${result.error ?? 'unknown error'}`);
  }

  const messageId = (result.data as ChatMessageEntity).id;
  testMessageIds.push(messageId);

  return result.data as ChatMessageEntity;
}

/**
 * Execute persona response (simulates Postmaster decision + PersonaUser.respond())
 * TODO: Implement persona/respond command
 */
export async function executePersonaResponse(
  personaId: UUID,
  roomId: UUID,
  triggeringMessageId: UUID
): Promise<{
  success: boolean;
  message?: ChatMessageEntity;
  error?: string;
}> {
  // This will eventually call persona/respond command
  // For now, return stub until command is implemented
  const result = await Commands.execute('persona/respond', {
    personaId,
    contextId: roomId,
    triggeringMessageId
  });

  return result;
}

/**
 * Create test genome layer
 */
export async function createTestGenomeLayer(
  name: string,
  traitType: string,
  embedding?: number[]
): Promise<GenomeLayerEntity> {
  const layerId = randomUUID();

  // Generate random 768-dim embedding if not provided
  const layerEmbedding = embedding ?? Array(768).fill(0).map(() => Math.random());

  const result = await DataCreate.execute({
    collection: 'genome_layers',
    data: {
      id: layerId,
      name,
      traitType,
      modelPath: `/test/layers/${name}.safetensors`,
      sizeMB: 25,
      rank: 16,
      embedding: layerEmbedding,
      source: 'trained',
      fitness: {
        accuracy: 0.85,
        usageCount: 0,
        successRate: 0.0,
        avgLatency: 0.0,
        cacheHitRate: 0.0
      },
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test genome layer: ${result.error ?? 'unknown error'}`);
  }

  testGenomeLayerIds.push(layerId);
  return result.data as GenomeLayerEntity;
}

/**
 * Create test genome
 */
export async function createTestGenome(
  personaId: UUID,
  options: {
    baseModel: string;
    layers: Array<{
      layerId: UUID;
      weight: number;
      enabled?: boolean;
    }>;
  }
): Promise<GenomeEntity> {
  const genomeId = randomUUID();

  // Calculate composite embedding (weighted average of layer embeddings)
  const compositeEmbedding = Array(768).fill(0);
  // For testing, just use zeros (would normally compute from layer embeddings)

  const result = await DataCreate.execute({
    collection: 'genomes',
    data: {
      id: genomeId,
      personaId,
      baseModel: options.baseModel,
      layers: options.layers.map((layer, index) => ({
        layerId: layer.layerId,
        traitType: 'test_trait', // Would normally look up from layer
        orderIndex: index,
        weight: layer.weight,
        enabled: layer.enabled ?? true
      })),
      compositeEmbedding,
      metadata: {
        generation: 0,
        source: 'test'
      },
      fitness: {
        overallScore: 0.0,
        usageCount: 0,
        successRate: 0.0,
        avgResponseTime: 0.0
      },
      createdAt: new Date()
    }
  });

  if (!result.success) {
    throw new Error(`Failed to create test genome: ${result.error ?? 'unknown error'}`);
  }

  testGenomeIds.push(genomeId);
  return result.data as GenomeEntity;
}

/**
 * Mock TypeScript expert embedding (for testing)
 * In reality, this would be generated from training on TypeScript docs
 */
export function mockTypeScriptExpertEmbedding(): number[] {
  // Return deterministic embedding for testing
  const seed = 42;
  const embedding: number[] = [];

  for (let i = 0; i < 768; i++) {
    // Simple seeded random
    const x = Math.sin(seed + i) * 10000;
    embedding.push(x - Math.floor(x));
  }

  return embedding;
}

/**
 * Clean up all test rooms
 */
export async function cleanupTestRooms(): Promise<void> {
  for (const roomId of testRoomIds) {
    await DataDelete.execute({
      collection: 'rooms',
      id: roomId
    });
  }
  testRoomIds.length = 0;
}

/**
 * Clean up all test personas
 */
export async function cleanupTestPersonas(): Promise<void> {
  for (const personaId of testPersonaIds) {
    await DataDelete.execute({
      collection: 'users',
      id: personaId
    });
  }
  testPersonaIds.length = 0;
}

/**
 * Clean up all test messages
 */
export async function cleanupTestMessages(): Promise<void> {
  for (const messageId of testMessageIds) {
    await DataDelete.execute({
      collection: 'chat_messages',
      id: messageId
    });
  }
  testMessageIds.length = 0;
}

/**
 * Clean up all test genome layers
 */
export async function cleanupTestGenomeLayers(): Promise<void> {
  for (const layerId of testGenomeLayerIds) {
    await DataDelete.execute({
      collection: 'genome_layers',
      id: layerId
    });
  }
  testGenomeLayerIds.length = 0;
}

/**
 * Clean up all test genomes
 */
export async function cleanupTestGenomes(): Promise<void> {
  for (const genomeId of testGenomeIds) {
    await DataDelete.execute({
      collection: 'genomes',
      id: genomeId
    });
  }
  testGenomeIds.length = 0;
}

/**
 * Clean up all test data (call in afterEach)
 */
export async function cleanupAllTestData(): Promise<void> {
  await cleanupTestMessages();
  await cleanupTestRooms();
  await cleanupTestPersonas();
  await cleanupTestGenomes();
  await cleanupTestGenomeLayers();
}
