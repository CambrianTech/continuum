/**
 * SemanticCompressionAdapter - LLM-based memory synthesis
 *
 * This adapter synthesizes multiple related thoughts into compressed insights
 * using an LLM. It groups thoughts by context/topic, then generates semantic
 * summaries that capture patterns, learnings, and decisions.
 *
 * Example transformation:
 * INPUT:  5 thoughts about "user prefers examples", "abstract failed", "code worked"
 * OUTPUT: 1 insight: "Teaching pattern: concrete code examples > abstract explanations"
 *
 * This is the "Phase 2" approach: semantic compression for RAG-friendly memories.
 */

import { MemoryConsolidationAdapter, type ConsolidationContext, type ConsolidationResult } from './MemoryConsolidationAdapter';
import type { WorkingMemoryEntry } from '../../../cognition/memory/InMemoryCognitionStorage';
import type { MemoryEntity } from '../../../MemoryTypes';
import { MemoryType } from '../../../MemoryTypes';
import { generateUUID } from '../../../../../../core/types/CrossPlatformUUID';
import { ISOString } from '../../../../../../data/domains/CoreTypes';
import type { PersonaUser } from '../../../../PersonaUser';
import { RustEmbeddingClient } from '../../../../../../core/services/RustEmbeddingClient';
import { BackpressureService } from '../../../../../../core/services/BackpressureService';

/**
 * Group of related thoughts for synthesis
 */
interface ThoughtGroup {
  contextId: string;
  domain: string;
  thoughts: WorkingMemoryEntry[];
  avgImportance: number;
}

export class SemanticCompressionAdapter extends MemoryConsolidationAdapter {
  private persona: PersonaUser;
  private maxThoughtsPerGroup: number;
  private log: (message: string, ...args: any[]) => void;

  constructor(persona: PersonaUser, config?: { maxThoughtsPerGroup?: number; logger?: (message: string, ...args: any[]) => void }) {
    super();
    this.persona = persona;
    this.maxThoughtsPerGroup = config?.maxThoughtsPerGroup || 10;
    this.log = config?.logger || console.log.bind(console);
  }

  async consolidate(
    thoughts: WorkingMemoryEntry[],
    context: ConsolidationContext
  ): Promise<ConsolidationResult> {
    if (thoughts.length === 0) {
      return { memories: [], metadata: { synthesisCount: 0, groupsCreated: 0 } };
    }

    // Group related thoughts by context and domain
    const groups = this.groupRelatedThoughts(thoughts);

    this.log(`🧠 [${context.personaName}] SemanticCompression: ${thoughts.length} thoughts → ${groups.length} groups`);

    // Synthesize each group via LLM (with backpressure awareness)
    const memories: MemoryEntity[] = [];
    let synthesisCount = 0;
    let skippedDueToLoad = 0;
    const errors: Array<{ domain: string; error: string }> = [];

    for (const group of groups) {
      // BACKPRESSURE: Check system load before expensive LLM synthesis
      // Memory synthesis is low priority - defer when system is loaded
      if (!BackpressureService.shouldProceed('low')) {
        skippedDueToLoad++;
        // Use fallback (no LLM call) when under load
        const fallback = this.createFallbackMemory(group, context);
        memories.push(fallback);
        continue;
      }

      try {
        const synthesis = await this.synthesizeGroup(group, context);
        memories.push(synthesis);
        synthesisCount++;
      } catch (error) {
        // Track error details in metadata (Hippocampus will log these)
        errors.push({
          domain: group.domain,
          error: error instanceof Error ? error.message : String(error)
        });

        // Fallback: store most important thought from group
        const fallback = this.createFallbackMemory(group, context);
        memories.push(fallback);
      }
    }

    if (skippedDueToLoad > 0) {
      this.log(`🚦 [${context.personaName}] Backpressure: Skipped ${skippedDueToLoad}/${groups.length} LLM syntheses (system load=${BackpressureService.getLoad().toFixed(2)})`);
    }

    // Phase 2: Generate embeddings for all memories (semantic cognition)
    // BACKPRESSURE: Embeddings are background priority - only generate when system is idle
    let embeddingsGenerated = 0;
    let embeddingsSkipped = 0;

    for (const memory of memories) {
      // Check backpressure before each embedding (background priority)
      if (!BackpressureService.shouldProceed('background')) {
        embeddingsSkipped++;
        continue; // Skip embedding, memory is still stored without it
      }

      try {
        // Generate embedding directly via Rust worker (fast, ~5ms)
        const client = RustEmbeddingClient.instance;
        if (!await client.isAvailable()) {
          this.log(`[Embedding] Rust worker not available, skipping memory ${memory.id}`);
          embeddingsSkipped++;
          continue;
        }

        const content = memory.content;
        if (!content || !content.trim()) {
          continue;
        }

        const embedding = await client.embed(content);
        if (embedding) {
          memory.embedding = embedding;
          memory.embeddedAt = new Date().toISOString() as ISOString;
          memory.embeddingModel = 'fastembed-onnx';
          embeddingsGenerated++;
        }
      } catch (error) {
        // Don't fail consolidation if embedding fails - memory is still valuable without it
        this.log(`[Embedding] Failed for memory ${memory.id}: ${error}`);
      }
    }

    if (embeddingsSkipped > 0) {
      this.log(`🚦 [${context.personaName}] Backpressure: Skipped ${embeddingsSkipped}/${memories.length} embeddings (will retry when idle)`);
    }
    this.log(`🧠 [${context.personaName}] Embeddings: ${embeddingsGenerated}/${memories.length} generated`);

    return {
      memories,
      metadata: {
        synthesisCount,
        groupsCreated: groups.length,
        embeddingsGenerated,
        skippedDueToLoad,      // LLM syntheses skipped due to backpressure
        embeddingsSkipped,     // Embeddings skipped due to backpressure
        errors: errors.length > 0 ? errors : undefined
      }
    };
  }

  getName(): string {
    return 'SemanticCompressionAdapter';
  }

  doesSynthesis(): boolean {
    return true;
  }

  /**
   * Group related thoughts by context and domain
   */
  private groupRelatedThoughts(thoughts: WorkingMemoryEntry[]): ThoughtGroup[] {
    const groupMap = new Map<string, ThoughtGroup>();

    for (const thought of thoughts) {
      // Group key: contextId + domain
      const key = `${thought.contextId || 'global'}-${thought.domain || 'general'}`;

      if (!groupMap.has(key)) {
        groupMap.set(key, {
          contextId: thought.contextId || 'global',
          domain: thought.domain || 'general',
          thoughts: [],
          avgImportance: 0
        });
      }

      const group = groupMap.get(key)!;
      group.thoughts.push(thought);
    }

    // Calculate average importance for each group
    for (const group of groupMap.values()) {
      group.avgImportance = group.thoughts.reduce((sum, t) => sum + t.importance, 0) / group.thoughts.length;
    }

    return Array.from(groupMap.values());
  }

  /**
   * Synthesize a group of thoughts into one compressed insight using LLM
   */
  private async synthesizeGroup(group: ThoughtGroup, context: ConsolidationContext): Promise<MemoryEntity> {
    // Build synthesis prompt
    const thoughtsList = group.thoughts
      .map((t, idx) => `${idx + 1}. [${t.thoughtType}] ${t.thoughtContent}`)
      .join('\n');

    const synthesisPrompt = `You are ${context.personaName}, an AI assistant reflecting on your recent experiences.

Your task: Synthesize these ${group.thoughts.length} related thoughts into ONE compressed insight.

Raw thoughts:
${thoughtsList}

Extract the KEY PATTERN, LEARNING, or DECISION from these thoughts. Focus on:
- What did you discover or learn?
- What pattern or relationship did you identify?
- What decision did you make and why?
- What strategy worked or failed?

Respond with ONLY the compressed insight (1-3 sentences). Be specific and actionable.`;

    // Use persona's unified inference interface (same code path as chat)
    const synthesizedText = await this.persona.generateText({
      prompt: synthesisPrompt,
      temperature: 0.3,   // Low temp for consistent synthesis
      maxTokens: 200,     // Concise insights
      context: 'memory-synthesis'
    });

    // Create synthesized memory entity
    const memory: MemoryEntity = {
      id: generateUUID(),
      createdAt: ISOString(new Date().toISOString()),
      updatedAt: ISOString(new Date().toISOString()),
      version: 0,
      personaId: context.personaId,
      sessionId: context.sessionId,
      type: this.inferMemoryType(group),
      content: synthesizedText.trim(),  // ← Synthesized insight
      context: {
        domain: group.domain,
        contextId: group.contextId,
        thoughtCount: group.thoughts.length,
        synthesizedFrom: group.thoughts.map(t => t.id)
      },
      timestamp: ISOString(new Date().toISOString()),
      consolidatedAt: ISOString(context.timestamp.toISOString()),
      importance: group.avgImportance,
      accessCount: 0,
      relatedTo: [],
      tags: [group.domain, 'synthesized'],
      source: 'semantic-compression'
    };

    return memory;
  }

  /**
   * Create fallback memory from most important thought (if synthesis fails)
   */
  private createFallbackMemory(group: ThoughtGroup, context: ConsolidationContext): MemoryEntity {
    // Use most important thought as fallback
    const mostImportant = group.thoughts.reduce((max, t) =>
      t.importance > max.importance ? t : max
    );

    return {
      id: generateUUID(),
      createdAt: ISOString(new Date().toISOString()),
      updatedAt: ISOString(new Date().toISOString()),
      version: 0,
      personaId: context.personaId,
      sessionId: context.sessionId,
      type: this.mapThoughtTypeToMemoryType(mostImportant.thoughtType),
      content: mostImportant.thoughtContent,
      context: {
        domain: group.domain,
        contextId: group.contextId,
        thoughtType: mostImportant.thoughtType
      },
      timestamp: ISOString(new Date(mostImportant.createdAt).toISOString()),
      consolidatedAt: ISOString(context.timestamp.toISOString()),
      importance: mostImportant.importance,
      accessCount: 0,
      relatedTo: [],
      tags: [group.domain, 'fallback'],
      source: 'working-memory-fallback'
    };
  }

  /**
   * Infer memory type from thought group characteristics
   */
  private inferMemoryType(group: ThoughtGroup): MemoryType {
    // Count thought types in group
    const typeCounts: Record<string, number> = {};
    for (const thought of group.thoughts) {
      typeCounts[thought.thoughtType] = (typeCounts[thought.thoughtType] || 0) + 1;
    }

    // Most common type
    const dominantType = Object.entries(typeCounts)
      .sort(([, a], [, b]) => b - a)[0][0];

    return this.mapThoughtTypeToMemoryType(dominantType);
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
        return MemoryType.OBSERVATION;
    }
  }
}
