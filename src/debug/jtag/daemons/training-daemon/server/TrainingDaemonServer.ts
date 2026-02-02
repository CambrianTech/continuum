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
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { TrainingDaemon } from '../shared/TrainingDaemon';
import { Events } from '../../../system/core/shared/Events';
import { DATA_EVENTS } from '../../../system/core/shared/EventConstants';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { ROOM_UNIQUE_IDS } from '../../../system/data/constants/RoomConstants';
import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import { TrainingExampleEntity, type TrainingMessage } from '../../data-daemon/shared/entities/TrainingExampleEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';

import { DataList } from '../../../commands/data/list/shared/DataListTypes';
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
  protected log: ComponentLogger;

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

    // Initialize logger with standard pattern
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  async initialize(): Promise<void> {
    this.log.info('TrainingDaemonServer: INITIALIZE CALLED - Starting setup');

    // Subscribe to chat message creation events first
    await this.setupEventSubscriptions();

    // Defer room loading to allow DataDaemon to initialize
    // TrainingDaemon can start without rooms and load them asynchronously
    setTimeout(() => {
      this.loadTrainingRooms().catch(error => {
        this.log.error('❌ TrainingDaemon: Failed to load rooms after delay:', error);
      });
    }, 2000); // Wait 2 seconds for DataDaemon to initialize

    this.log.info('🧠 TrainingDaemonServer: Initialized, will load training rooms asynchronously');
  }

  /**
   * Load room IDs for training-enabled rooms
   */
  private async loadTrainingRooms(): Promise<void> {
    this.log.info('🔄 TrainingDaemon: Loading training-enabled rooms...');

    const { Commands } = await import('../../../system/core/shared/Commands');

    for (const roomUniqueId of this.config.enabledRooms) {
      try {
        // Use Commands.execute instead of DataDaemon.query for reliability
        const result = await DataList.execute<RoomEntity>({
          collection: COLLECTIONS.ROOMS,
          filter: { uniqueId: roomUniqueId },
          limit: 1
        });

        if (result.success && result.items && result.items.length > 0) {
          const roomId = result.items[0].id;
          this.trainingRoomIds.add(roomId);
          this.log.info(`🧠 TrainingDaemon: Monitoring room "${roomUniqueId}" (${roomId}) for training data`);
        } else {
          this.log.warn(`⚠️  TrainingDaemon: Training room "${roomUniqueId}" not found`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log.error(`❌ TrainingDaemon: Failed to load room "${roomUniqueId}": ${errorMessage}`);
      }
    }
  }

  /**
   * Subscribe to chat message creation events
   */
  private async setupEventSubscriptions(): Promise<void> {
    //console.log(`🧠 TrainingDaemonServer: Subscribing to ${DATA_EVENTS.CHAT_MESSAGES.CREATED}`);

    const unsubCreated = Events.subscribe<ChatMessageEntity>(
      DATA_EVENTS.CHAT_MESSAGES.CREATED,
      async (messageEntity: ChatMessageEntity) => {
        await this.handleMessageCreated(messageEntity);
      }
    );

    this.registerSubscription(unsubCreated);
    this.log.info('🧠 TrainingDaemonServer: Subscription complete');
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
        this.log.info(`🧠 TrainingDaemon: Skipping system test message (testType: ${messageEntity.metadata.testType})`);
        return;
      }

      // Filter 3: Skip deleted messages
      if (messageEntity.status === 'deleted') {
        return;
      }

      this.log.info(`🧠 TrainingDaemon: Processing message in training room (sender: ${messageEntity.senderId})`);

      // Fetch conversation context (recent messages in room)
      const contextMessages = await this.fetchConversationContext(
        messageEntity.roomId,
        messageEntity.id,
        this.config.contextWindow
      );

      // Need minimum messages to form meaningful training example
      if (contextMessages.length < this.config.minMessages) {
        this.log.info(`🧠 TrainingDaemon: Insufficient context (${contextMessages.length} < ${this.config.minMessages}), skipping`);
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
        this.log.error(`❌ TrainingDaemon: Validation failed: ${validation.error}`);
        return;
      }

      // Store training example
      const storedEntity = await DataDaemon.store<TrainingExampleEntity>(
        TrainingExampleEntity.collection,
        trainingExample
      );

      this.log.info(`✅ TrainingDaemon: Created training example (${messageCount} messages, ~${totalTokens} tokens)`);

      // Check if we should trigger auto fine-tuning
      await this.checkAutoFineTuneThreshold();
    } catch (error) {
      this.log.error('❌ TrainingDaemon: Error processing message:', error);
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
      this.log.error('❌ TrainingDaemon: Failed to fetch context:', error);
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
        this.log.warn(`⚠️  TrainingDaemon: Could not fetch sender ${msg.senderId}, skipping message`);
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
      return await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);
    } catch (error) {
      this.log.error(`❌ TrainingDaemon: Failed to fetch user ${userId}:`, error);
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
          this.log.info(`🚀 TrainingDaemon: Auto fine-tune threshold reached (${count} examples)`);
          this.log.info('🚀 TrainingDaemon: TODO: Trigger fine-tuning (Phase 2 implementation)');
          // Future: Trigger genome/batch-micro-tune command
        }
      }
    } catch (error) {
      this.log.error('❌ TrainingDaemon: Failed to check auto fine-tune threshold:', error);
    }
  }

  /**
   * Cleanup on shutdown
   * Base class handles subscription cleanup automatically
   */
  protected async cleanup(): Promise<void> {
    this.log.info('🧠 TrainingDaemon: Cleanup complete');
  }
}
