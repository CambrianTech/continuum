/**
 * TrainingDatasetBuilder - Server-only dataset construction
 *
 * Converts PersonaUser experiences into fine-tuning datasets:
 * - Chat conversations (MVP - already exists!)
 * - User corrections (future - Phase 7.1+)
 * - Training exercises (future - Academy integration)
 *
 * Philosophy: "Start simple, expand systematically"
 * - Phase 7.0 MVP: Chat conversations only
 * - Later phases: Add corrections, exercises, memories
 *
 * SERVER-ONLY: Uses Node.js and database operations
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import type {
  TrainingDataset,
  TrainingExample,
  TrainingMessage
} from '../shared/FineTuningTypes';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { ChatMessageEntity, MessageContent } from '../../../data/entities/ChatMessageEntity';

/**
 * Dataset builder configuration
 */
export interface DatasetBuilderConfig {
  // Conversation window
  maxMessages?: number;          // Max messages to include (default: 100)
  minMessages?: number;          // Min messages required (default: 10)

  // Quality filters
  minMessageLength?: number;     // Min chars per message (default: 10)
  excludeSystemMessages?: boolean; // Exclude system messages (default: true)

  // Persona filtering
  includeOwnMessages?: boolean;  // Include persona's own messages (default: true)
  includeOtherPersonas?: boolean; // Include other AI messages (default: true)
}

/**
 * Dataset builder result
 */
export interface DatasetBuilderResult {
  success: boolean;
  dataset?: TrainingDataset;
  error?: string;
  stats?: {
    messagesProcessed: number;
    examplesCreated: number;
    messagesTooShort: number;
    messagesFiltered: number;
  };
}

/**
 * TrainingDatasetBuilder - Converts experiences into training data
 *
 * MVP: Builds datasets from chat conversations
 */
export class TrainingDatasetBuilder {
  private readonly config: Required<DatasetBuilderConfig>;

  constructor(config: Partial<DatasetBuilderConfig> = {}) {
    this.config = {
      maxMessages: config.maxMessages ?? 100,
      minMessages: config.minMessages ?? 10,
      minMessageLength: config.minMessageLength ?? 10,
      excludeSystemMessages: config.excludeSystemMessages ?? true,
      includeOwnMessages: config.includeOwnMessages ?? true,
      includeOtherPersonas: config.includeOtherPersonas ?? true
    };
  }

  /**
   * Build dataset from chat conversations in a specific room
   *
   * MVP implementation: Simple conversation history from single room
   */
  async buildFromConversation(
    personaId: UUID,
    personaName: string,
    roomId: UUID,
    traitType: TraitType
  ): Promise<DatasetBuilderResult> {
    const stats = {
      messagesProcessed: 0,
      examplesCreated: 0,
      messagesTooShort: 0,
      messagesFiltered: 0
    };

    try {
      // Load conversation history from room
      const messages = await this.loadMessages(roomId);

      if (messages.length < this.config.minMessages) {
        return {
          success: false,
          error: `Insufficient messages: ${messages.length} < ${this.config.minMessages}`,
          stats
        };
      }

      stats.messagesProcessed = messages.length;

      // Convert messages to training examples
      const examples: TrainingExample[] = [];

      // Strategy: Sliding window of 3-5 messages creates one training example
      // Example: [user1, assistant, user2, assistant] → training example
      const windowSize = 5;
      for (let i = 0; i < messages.length - windowSize + 1; i++) {
        const window = messages.slice(i, i + windowSize);

        // Filter window
        const filtered = this.filterMessages(window, personaId);

        if (filtered.length >= 2) {
          // Need at least 2 messages for context + response
          const example = this.createTrainingExample(filtered, personaId);

          if (example) {
            examples.push(example);
            stats.examplesCreated++;
          } else {
            stats.messagesFiltered++;
          }
        } else {
          stats.messagesFiltered++;
        }
      }

      if (examples.length === 0) {
        return {
          success: false,
          error: 'No valid training examples created after filtering',
          stats
        };
      }

      // Build dataset
      const dataset: TrainingDataset = {
        examples,
        metadata: {
          personaId,
          personaName,
          traitType,
          createdAt: Date.now(),
          source: 'conversations',
          totalExamples: examples.length
        }
      };

      return { success: true, dataset, stats };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        stats
      };
    }
  }

  /**
   * Load messages from room (most recent first, then reverse)
   */
  private async loadMessages(roomId: UUID): Promise<ChatMessageEntity[]> {
    // Note: DataListParams doesn't support orderBy, messages returned in insertion order
    // We'll reverse them after loading to get chronological order
    const result = await ORM.query<ChatMessageEntity>({
      collection: 'chat_messages',
      filter: { roomId },
      limit: this.config.maxMessages
    });

    if (!result.success || !result.data) {
      throw new Error('Failed to load messages from room');
    }

    // Unwrap DataRecord<ChatMessageEntity>[] to ChatMessageEntity[]
    const messages = result.data.map(record => record.data);

    // Reverse to chronological order (oldest first)
    return messages.reverse();
  }

  /**
   * Filter messages based on config
   */
  /**
   * Convert MessageContent to string
   */
  private contentToString(content: MessageContent | string | null | undefined): string {
    if (!content) return '';
    if (typeof content === 'string') return content;
    return content.text || '';
  }

  private filterMessages(
    messages: ChatMessageEntity[],
    personaId: UUID
  ): ChatMessageEntity[] {
    return messages.filter(msg => {
      // Filter by length
      const contentText = this.contentToString(msg.content);
      if (!contentText || contentText.length < this.config.minMessageLength) {
        return false;
      }

      // Filter system messages
      if (this.config.excludeSystemMessages && msg.metadata?.source === 'system') {
        return false;
      }

      // Filter persona's own messages
      if (!this.config.includeOwnMessages && msg.senderId === personaId) {
        return false;
      }

      // Filter other AI messages (no category field exists, skip this check)
      // if (!this.config.includeOtherPersonas && msg.metadata?.category === 'ai' && msg.senderId !== personaId) {
      //   return false;
      // }

      return true;
    });
  }

  /**
   * Create training example from message window
   *
   * Format: Standard chat completions (OpenAI/Anthropic compatible)
   * - Last message = assistant response (what we're training)
   * - Previous messages = conversation context
   */
  private createTrainingExample(
    messages: ChatMessageEntity[],
    personaId: UUID
  ): TrainingExample | null {
    if (messages.length < 2) {
      return null;
    }

    // Last message should ideally be from the persona (assistant response)
    const lastMessage = messages[messages.length - 1];
    const isPersonaResponse = lastMessage.senderId === personaId;

    if (!isPersonaResponse) {
      // Skip this window if persona didn't respond
      return null;
    }

    // Convert messages to training format
    const trainingMessages: TrainingMessage[] = messages.map((msg) => {
      const isAssistant = msg.senderId === personaId;
      const role: 'system' | 'user' | 'assistant' = isAssistant ? 'assistant' : 'user';

      return {
        role,
        content: this.contentToString(msg.content)
      };
    });

    // Add system prompt at the beginning (optional, helps with identity)
    // trainingMessages.unshift({
    //   role: 'system',
    //   content: `You are ${personaName}, a helpful AI assistant.`
    // });

    return {
      messages: trainingMessages,
      metadata: {
        timestamp: lastMessage.createdAt?.getTime() ?? Date.now(),
        roomId: lastMessage.roomId,
        confidence: 1.0 // High confidence for actual conversations
      }
    };
  }

  /**
   * Build dataset from multiple rooms
   *
   * Aggregates conversations from multiple rooms into single dataset
   */
  async buildFromMultipleRooms(
    personaId: UUID,
    personaName: string,
    roomIds: UUID[],
    traitType: TraitType
  ): Promise<DatasetBuilderResult> {
    const allExamples: TrainingExample[] = [];
    const stats = {
      messagesProcessed: 0,
      examplesCreated: 0,
      messagesTooShort: 0,
      messagesFiltered: 0
    };

    for (const roomId of roomIds) {
      const result = await this.buildFromConversation(
        personaId,
        personaName,
        roomId,
        traitType
      );

      if (result.success && result.dataset) {
        allExamples.push(...result.dataset.examples);
      }

      if (result.stats) {
        stats.messagesProcessed += result.stats.messagesProcessed;
        stats.examplesCreated += result.stats.examplesCreated;
        stats.messagesTooShort += result.stats.messagesTooShort;
        stats.messagesFiltered += result.stats.messagesFiltered;
      }
    }

    if (allExamples.length === 0) {
      return {
        success: false,
        error: 'No training examples created from any room',
        stats
      };
    }

    const dataset: TrainingDataset = {
      examples: allExamples,
      metadata: {
        personaId,
        personaName,
        traitType,
        createdAt: Date.now(),
        source: 'conversations',
        totalExamples: allExamples.length
      }
    };

    return { success: true, dataset, stats };
  }

  /**
   * Export dataset to JSONL format (for llama.cpp, OpenAI, etc.)
   *
   * Each line is a JSON object with "messages" array
   */
  static exportToJSONL(dataset: TrainingDataset): string {
    return dataset.examples
      .map(example => JSON.stringify({ messages: example.messages }))
      .join('\n');
  }

  /**
   * Validate dataset quality
   *
   * Checks for common issues:
   * - Too few examples
   * - Examples too short
   * - Missing assistant responses
   */
  static validateDataset(dataset: TrainingDataset): {
    valid: boolean;
    warnings: string[];
    errors: string[];
  } {
    const warnings: string[] = [];
    const errors: string[] = [];

    // Check minimum examples
    if (dataset.examples.length < 10) {
      warnings.push(`Low example count: ${dataset.examples.length} (recommended: 50+)`);
    }

    // Check each example
    for (let i = 0; i < dataset.examples.length; i++) {
      const example = dataset.examples[i];

      // Check message count
      if (example.messages.length < 2) {
        errors.push(`Example ${i}: Too few messages (${example.messages.length})`);
      }

      // Check for assistant response
      const hasAssistant = example.messages.some(msg => msg.role === 'assistant');
      if (!hasAssistant) {
        errors.push(`Example ${i}: No assistant response`);
      }

      // Check message lengths
      const shortMessages = example.messages.filter(msg => msg.content.length < 10);
      if (shortMessages.length > 0) {
        warnings.push(`Example ${i}: ${shortMessages.length} very short messages`);
      }
    }

    return {
      valid: errors.length === 0,
      warnings,
      errors
    };
  }
}
