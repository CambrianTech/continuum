/**
 * TrainingDaemonServer - Silent observer converting conversations into training data
 *
 * ARCHITECTURE:
 * - Subscribes to data:chat_messages:created events
 * - Filters for messages in training-enabled rooms (dev-updates)
 * - Ignores system test messages (precommit hooks, integration tests)
 * - Accumulates conversation context (window of recent messages)
 * - Scores message quality and creates TrainingExampleEntity records
 * - Auto-triggers fine-tuning when threshold reached (future: Phase 2)
 *
 * QUALITY SCORING (future enhancement):
 * - Corrections (mistakes + fixes) = priority 1.0 (critical learning)
 * - Consensus (multiple AIs agree) = priority 0.7 (high confidence)
 * - Discussion (exploration) = priority 0.5 (medium value)
 *
 * NATURAL INTEGRATION:
 * - GitHub webhooks post to #dev-updates
 * - AIs discuss PRs naturally
 * - TrainingDaemon observes silently
 * - High-quality exchanges → training data automatically
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { TrainingDaemon } from '../shared/TrainingDaemon';
import { Events } from '../../../system/core/shared/Events';
import { DATA_EVENTS } from '../../../system/core/shared/EventConstants';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { ROOM_UNIQUE_IDS } from '../../../system/data/constants/RoomConstants';
import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import { TrainingExampleEntity, type TrainingMessage } from '../../data-daemon/shared/entities/TrainingExampleEntity';

/**
 * Configuration for training data collection
 */
interface TrainingConfig {
  /** Rooms where training observation is enabled */
  enabledRooms: string[];
  /** Number of messages to include in context window (before + current) */
  contextWindow: number;
  /** Minimum messages in window before creating training example */
  minMessages: number;
  /** Auto fine-tune threshold (number of examples accumulated) */
  autoFineTuneThreshold: number;
}

export class TrainingDaemonServer extends TrainingDaemon {
  private unsubscribeFunctions: (() => void)[] = [];

  /** Training configuration */
  private readonly config: TrainingConfig = {
    enabledRooms: [ROOM_UNIQUE_IDS.DEV_UPDATES, 'general'],  // Watch dev-updates and general rooms
    contextWindow: 10,  // Include last 10 messages as context
    minMessages: 3,     // Need at least 3 messages to form training example
    autoFineTuneThreshold: 50  // Auto fine-tune when 50+ examples accumulated
  };

  /** Cache of room IDs for training-enabled rooms */
  private trainingRoomIds: Set<UUID> = new Set();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  async initialize(): Promise<void> {
    console.log('🧠 TrainingDaemonServer: INITIALIZE CALLED - Starting setup');

    // Load training-enabled room IDs
    await this.loadTrainingRooms();

    // Subscribe to chat message creation events
    await this.setupEventSubscriptions();

    console.log(`🧠 TrainingDaemonServer: Initialized, monitoring ${this.trainingRoomIds.size} room(s) for training data`);
  }

  /**
   * Load room IDs for training-enabled rooms
   */
  private async loadTrainingRooms(): Promise<void> {
    console.log('🔄 TrainingDaemon: Loading training-enabled rooms...');

    for (const roomUniqueId of this.config.enabledRooms) {
      try {
        const queryResult = await DataDaemon.query<RoomEntity>({
          collection: COLLECTIONS.ROOMS,
          filter: { uniqueId: roomUniqueId }
        });

        if (queryResult.success && queryResult.data && queryResult.data.length > 0) {
          const roomId = queryResult.data[0].data.id;
          this.trainingRoomIds.add(roomId);
          console.log(`🧠 TrainingDaemon: Monitoring room "${roomUniqueId}" (${roomId}) for training data`);
        } else {
          console.warn(`⚠️  TrainingDaemon: Training room "${roomUniqueId}" not found`);
        }
      } catch (error) {
        console.error(`❌ TrainingDaemon: Failed to load room "${roomUniqueId}":`, error);
      }
    }
  }

  /**
   * Subscribe to chat message creation events
   */
  private async setupEventSubscriptions(): Promise<void> {
    console.log(`🧠 TrainingDaemonServer: Subscribing to ${DATA_EVENTS.CHAT_MESSAGES.CREATED}`);

    const unsubCreated = Events.subscribe<ChatMessageEntity>(
      DATA_EVENTS.CHAT_MESSAGES.CREATED,
      async (messageEntity: ChatMessageEntity) => {
        await this.handleMessageCreated(messageEntity);
      }
    );

    this.unsubscribeFunctions.push(unsubCreated);
    console.log('🧠 TrainingDaemonServer: Subscription complete');
  }

  /**
   * Handle chat message created - determine if it should be training data
   */
  private async handleMessageCreated(messageEntity: ChatMessageEntity): Promise<void> {
    try {
      // Filter 1: Only process messages in training-enabled rooms
      if (!this.trainingRoomIds.has(messageEntity.roomId)) {
        return;
      }

      // Filter 2: Skip system test messages (precommit hooks, integration tests)
      if (messageEntity.metadata?.isSystemTest) {
        console.log(`🧠 TrainingDaemon: Skipping system test message (testType: ${messageEntity.metadata.testType})`);
        return;
      }

      // Filter 3: Skip deleted messages
      if (messageEntity.status === 'deleted') {
        return;
      }

      console.log(`🧠 TrainingDaemon: Processing message in training room (sender: ${messageEntity.senderId})`);

      // Fetch conversation context (recent messages in room)
      const contextMessages = await this.fetchConversationContext(
        messageEntity.roomId,
        messageEntity.id,
        this.config.contextWindow
      );

      // Need minimum messages to form meaningful training example
      if (contextMessages.length < this.config.minMessages) {
        console.log(`🧠 TrainingDaemon: Insufficient context (${contextMessages.length} < ${this.config.minMessages}), skipping`);
        return;
      }

      // Convert to training format
      const trainingMessages = await this.convertToTrainingMessages(contextMessages);

      // Calculate metrics
      const messageCount = trainingMessages.length;
      const totalTokens = this.estimateTokens(trainingMessages);

      // Create training example entity
      const trainingExample = new TrainingExampleEntity();
      trainingExample.messages = trainingMessages;
      trainingExample.messageCount = messageCount;
      trainingExample.totalTokens = totalTokens;
      trainingExample.metadata = {
        roomId: messageEntity.roomId,
        sourceMessageId: messageEntity.id,
        timestamp: messageEntity.timestamp,
        quality: 'medium',  // Future: implement quality scoring
        source: 'chat-conversation'
      };

      // Validate before storing
      const validation = trainingExample.validate();
      if (!validation.success) {
        console.error(`❌ TrainingDaemon: Validation failed: ${validation.error}`);
        return;
      }

      // Store training example
      const storedEntity = await DataDaemon.store<TrainingExampleEntity>(
        TrainingExampleEntity.collection,
        trainingExample
      );

      console.log(`✅ TrainingDaemon: Created training example (${messageCount} messages, ~${totalTokens} tokens)`);

      // Check if we should trigger auto fine-tuning
      await this.checkAutoFineTuneThreshold();
    } catch (error) {
      console.error('❌ TrainingDaemon: Error processing message:', error);
    }
  }

  /**
   * Fetch conversation context (recent messages)
   */
  private async fetchConversationContext(
    roomId: UUID,
    currentMessageId: UUID,
    windowSize: number
  ): Promise<ChatMessageEntity[]> {
    try {
      const queryResult = await DataDaemon.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: windowSize + 1  // Include current message + context
      });

      if (!queryResult.success || !queryResult.data) {
        return [];
      }

      // Extract entities and reverse to chronological order
      const messages = queryResult.data
        .map(record => record.data)
        .filter(msg => msg.status !== 'deleted')  // Exclude deleted messages
        .reverse();  // Oldest first

      return messages;
    } catch (error) {
      console.error('❌ TrainingDaemon: Failed to fetch context:', error);
      return [];
    }
  }

  /**
   * Convert chat messages to training message format
   */
  private async convertToTrainingMessages(messages: ChatMessageEntity[]): Promise<TrainingMessage[]> {
    const trainingMessages: TrainingMessage[] = [];

    for (const msg of messages) {
      // Fetch sender to determine role
      const sender = await this.fetchUser(msg.senderId);
      if (!sender) {
        console.warn(`⚠️  TrainingDaemon: Could not fetch sender ${msg.senderId}, skipping message`);
        continue;
      }

      // Determine role based on user type
      let role: 'system' | 'user' | 'assistant';
      if (msg.metadata?.source === 'system' || msg.metadata?.source === 'webhook') {
        role = 'system';
      } else if (sender.type === 'human') {
        role = 'user';
      } else {
        role = 'assistant';  // AI (persona or agent)
      }

      trainingMessages.push({
        role,
        content: msg.content.text
      });
    }

    return trainingMessages;
  }

  /**
   * Fetch user entity (cached in future)
   */
  private async fetchUser(userId: UUID): Promise<UserEntity | null> {
    try {
      const result = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);
      return result.success && result.data ? result.data.data : null;
    } catch (error) {
      console.error(`❌ TrainingDaemon: Failed to fetch user ${userId}:`, error);
      return null;
    }
  }

  /**
   * Estimate token count (rough approximation: 1 token ≈ 4 characters)
   */
  private estimateTokens(messages: TrainingMessage[]): number {
    const totalChars = messages.reduce((sum, msg) => sum + msg.content.length, 0);
    return Math.ceil(totalChars / 4);
  }

  /**
   * Check if we've reached auto fine-tune threshold
   */
  private async checkAutoFineTuneThreshold(): Promise<void> {
    try {
      const queryResult = await DataDaemon.query<TrainingExampleEntity>({
        collection: TrainingExampleEntity.collection,
        filter: {},
        limit: 1  // Just need count
      });

      if (queryResult.success && queryResult.metadata?.totalCount) {
        const count = queryResult.metadata.totalCount;

        if (count >= this.config.autoFineTuneThreshold && count % this.config.autoFineTuneThreshold === 0) {
          console.log(`🚀 TrainingDaemon: Auto fine-tune threshold reached (${count} examples)`);
          console.log('🚀 TrainingDaemon: TODO: Trigger fine-tuning (Phase 2 implementation)');
          // Future: Trigger genome/batch-micro-tune command
        }
      }
    } catch (error) {
      console.error('❌ TrainingDaemon: Failed to check auto fine-tune threshold:', error);
    }
  }

  /**
   * Cleanup on shutdown
   */
  async cleanup(): Promise<void> {
    console.log('🧠 TrainingDaemon: Cleaning up subscriptions...');
    for (const unsub of this.unsubscribeFunctions) {
      unsub();
    }
    this.unsubscribeFunctions = [];
  }
}
