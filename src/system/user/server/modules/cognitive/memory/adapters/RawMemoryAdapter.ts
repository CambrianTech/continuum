/**
 * RawMemoryAdapter - Simple pass-through consolidation
 *
 * This adapter performs no synthesis - it just converts WorkingMemoryEntry
 * directly to MemoryEntity for storage. This is the "Phase 1" approach:
 * store raw thoughts without compression.
 *
 * Use cases:
 * - Debugging/analysis (preserve raw thought stream)
 * - Low-resource personas (no LLM synthesis overhead)
 * - Testing baseline memory system
 */

import { MemoryConsolidationAdapter, type ConsolidationContext, type ConsolidationResult } from './MemoryConsolidationAdapter';
import type { WorkingMemoryEntry } from '../../../cognition/memory/InMemoryCognitionStorage';
import type { MemoryEntity } from '../../../MemoryTypes';
import { MemoryType } from '../../../MemoryTypes';
import { generateUUID } from '../../../../../../core/types/CrossPlatformUUID';
import { ISOString } from '../../../../../../data/domains/CoreTypes';

export class RawMemoryAdapter extends MemoryConsolidationAdapter {
  async consolidate(
    thoughts: WorkingMemoryEntry[],
    context: ConsolidationContext
  ): Promise<ConsolidationResult> {
    // Direct conversion: WorkingMemoryEntry → MemoryEntity
    const memories: MemoryEntity[] = thoughts.map(thought => ({
      id: generateUUID(),
      createdAt: ISOString(new Date().toISOString()),
      updatedAt: ISOString(new Date().toISOString()),
      version: 0,
      personaId: context.personaId,
      sessionId: context.sessionId,
      type: this.mapThoughtTypeToMemoryType(thought.thoughtType),
      content: thought.thoughtContent,
      context: {
        domain: thought.domain,
        contextId: thought.contextId,
        thoughtType: thought.thoughtType,
        shareable: thought.shareable
      },
      timestamp: ISOString(new Date(thought.createdAt).toISOString()),
      consolidatedAt: ISOString(context.timestamp.toISOString()),
      importance: thought.importance,
      accessCount: 0,
      relatedTo: [],
      tags: thought.domain ? [thought.domain] : [],
      source: 'working-memory'
    }));

    return {
      memories,
      metadata: {
        synthesisCount: 0,  // No synthesis in raw adapter
        groupsCreated: thoughts.length  // One memory per thought
      }
    };
  }

  getName(): string {
    return 'RawMemoryAdapter';
  }

  doesSynthesis(): boolean {
    return false;
  }

  /**
   * Map thought type to memory type
   */
  private mapThoughtTypeToMemoryType(thoughtType: string): MemoryType {
    switch (thoughtType) {
      case 'reflection':
        return MemoryType.OBSERVATION;
      case 'decision':
        return MemoryType.DECISION;
      case 'pattern':
        return MemoryType.INSIGHT;
      case 'observation':
        return MemoryType.OBSERVATION;
      default:
        return MemoryType.OBSERVATION;  // Default to observation
    }
  }
}
