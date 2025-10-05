/**
 * PersonaUser - Internal AI citizen with RAG + optional LoRA genome
 *
 * THE ACTUAL AI THAT RESPONDS TO CHAT MESSAGES.
 * This is what makes the system useful - without this, we're chatting with the void.
 *
 * Architecture:
 * - Subscribes to data:ChatMessage:created events
 * - Loads chat history as RAG context
 * - Calls AI API (Claude/GPT) to generate responses
 * - Posts responses back to chat as this persona
 * - Dedicated SQLite storage per persona
 */

import { AIUser } from './AIUser';
import { UserEntity } from '../../data/entities/UserEntity';
import { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import { Commands } from '../../core/client/shared/Commands';
import type { JTAGClient } from '../../core/client/shared/JTAGClient';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import type { RoomEntity } from '../../data/entities/RoomEntity';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';

/**
 * RAG Context Types - Storage structure for persona conversation context
 */
interface PersonaRAGMessage {
  senderId: UUID;
  senderName: string;
  text: string;
  timestamp: string;
}

interface PersonaRAGContext {
  roomId: UUID;
  personaId: UUID;
  messages: PersonaRAGMessage[];
  lastUpdated: string;
  tokenCount: number;
}

/**
 * PersonaUser - Our internal AI citizens
 *
 * First-class citizens with their own JTAGClient for universal Commands/Events API
 */
export class PersonaUser extends AIUser {
  private isInitialized: boolean = false;
  private eventsSubscribed: boolean = false;
  // Note: client is now in BaseUser as protected property, accessible via this.client
  // ArtifactsAPI access is through this.client.daemons.artifacts

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage, client); // ✅ Pass client to BaseUser for event subscriptions
    if (client) {
      console.log(`🔌 PersonaUser ${this.displayName}: Client injected and passed to BaseUser`);
      console.log(`📦 PersonaUser ${this.displayName}: ArtifactsAPI available via client.daemons`);
    }
  }

  /**
   * Initialize persona - use BaseUser helpers for room management and event subscriptions
   * PersonaUser acts as an autonomous agent, listening for messages in rooms they're a member of
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      console.log(`ℹ️ PersonaUser ${this.displayName}: Already initialized, reloading rooms...`);
      console.log(`🔍 DEBUG: eventsSubscribed=${this.eventsSubscribed}, hasClient=${!!this.client}`);
      // CRITICAL: Reload rooms even when already initialized
      // PersonaUsers might be created before rooms exist, so we need to refresh membership
      await this.loadMyRooms();
      console.log(`✅ PersonaUser ${this.displayName}: Reloaded, now in ${this.myRoomIds.size} rooms`);

      // CRITICAL: Subscribe to events if not already done
      // PersonaUsers might be created and initialized before having a client
      if (!this.eventsSubscribed && this.client) {
        console.log(`🔧 PersonaUser ${this.displayName}: Setting up event subscriptions (deferred)`);
        this.subscribeToChatEvents(this.handleChatMessage.bind(this));
        this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));
        this.eventsSubscribed = true;
      } else {
        console.log(`⏭️ PersonaUser ${this.displayName}: Skipping subscription (eventsSubscribed=${this.eventsSubscribed}, hasClient=${!!this.client})`);
      }
      return;
    }

    console.log(`🤖 PersonaUser ${this.displayName}: Initializing...`);

    // STEP 1: Base initialization (loads state + rooms)
    await super.initialize();

    // STEP 2: Subscribe to chat events using BaseUser helper (only if client available)
    if (this.client && !this.eventsSubscribed) {
      console.log(`🔧 PersonaUser ${this.displayName}: Setting up event subscriptions (first init)`);
      this.subscribeToChatEvents(this.handleChatMessage.bind(this));
      this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));
      this.eventsSubscribed = true;
    }

    this.isInitialized = true;
    console.log(`✅ PersonaUser ${this.displayName}: Initialized with ${this.myRoomIds.size} rooms, eventsSubscribed=${this.eventsSubscribed}`);
  }

  /**
   * Handle incoming chat message
   * Decides whether to respond based on room membership and other factors
   */
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    console.log(`🔧 CLAUDE-FIX-${Date.now()}: PersonaUser ${this.displayName}: handleChatMessage called`);
    console.log(`🔧 CLAUDE-FIX-${Date.now()}: Message from ${messageEntity.senderName} (${messageEntity.senderId})`);

    // Ignore our own messages
    if (messageEntity.senderId === this.id) {
      console.log(`🔇 CLAUDE-FIX-${Date.now()}: ${this.displayName}: Ignoring own message`);
      return;
    }

    // TEMPORARY: Disable ALL persona responses until we verify the filtering works
    console.log(`🚫 CLAUDE-FIX-${Date.now()}: ${this.displayName}: Persona responses DISABLED for debugging`);

    // Still update RAG context for future use
    await this.updateRAGContext(messageEntity.roomId, messageEntity);
    return;
  }

  /**
   * Generate and post a response to a chat message
   * Phase 1: Simple templated responses
   * Phase 2+: AI API integration with RAG context
   */
  private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
    try {
      // Simple response templates (Phase 1)
      const responses = [
        `Interesting point, ${originalMessage.senderName}!`,
        `I see what you mean about that.`,
        `That's a good question. Let me think about it.`,
        `Thanks for sharing!`,
        `Could you elaborate on that?`
      ];

      const responseText = responses[Math.floor(Math.random() * responses.length)];

      // Create response message entity
      const responseMessage = new ChatMessageEntity();
      responseMessage.roomId = originalMessage.roomId;
      responseMessage.senderId = this.id;
      responseMessage.senderName = this.displayName;
      responseMessage.content = { text: responseText, attachments: [] };
      responseMessage.status = 'sent';
      responseMessage.priority = 'normal';
      responseMessage.timestamp = new Date();
      responseMessage.reactions = [];

      // ✅ Post response via JTAGClient - universal Commands API
      // Prefer this.client if available (set by UserDaemon), fallback to shared instance
      const result = this.client
        ? await this.client.daemons.commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
            context: this.client.context,
            sessionId: this.client.sessionId,
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          })
        : await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          });

      if (!result.success) {
        throw new Error(`Failed to create message: ${result.error}`);
      }

      console.log(`✅ PersonaUser ${this.displayName}: Posted response: "${responseText}"`);

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Failed to respond:`, error);
    }
  }

  /**
   * Handle room update event
   * Updates membership tracking when this persona is added/removed from a room
   */
  private async handleRoomUpdate(roomEntity: RoomEntity): Promise<void> {
    const isMember = roomEntity.members.some((m: { userId: UUID }) => m.userId === this.id);
    const wasInRoom = this.myRoomIds.has(roomEntity.id);

    if (isMember && !wasInRoom) {
      // Added to room
      this.myRoomIds.add(roomEntity.id);
      console.log(`✅ PersonaUser ${this.displayName}: Added to room "${roomEntity.name}"`);
    } else if (!isMember && wasInRoom) {
      // Removed from room
      this.myRoomIds.delete(roomEntity.id);
      console.log(`➖ PersonaUser ${this.displayName}: Removed from room "${roomEntity.name}"`);
    }
  }

  /**
   * Check if this persona is mentioned in a message
   * Supports @username mentions and channel directives
   *
   * TODO Phase 2: Use dedicated mention/directive events instead of text parsing
   */
  private isPersonaMentioned(messageText: string): boolean {
    const displayNameLower = this.displayName.toLowerCase();
    const uniqueIdLower = this.entity.uniqueId?.toLowerCase() || '';

    // Check for @mentions: "@PersonaName" or "@uniqueid"
    const mentionPatterns = [
      `@${displayNameLower}`,
      `@${uniqueIdLower}`,
      // Also support space after @ for natural language: "@ PersonaName"
      `@ ${displayNameLower}`,
      `@ ${uniqueIdLower}`
    ];

    for (const pattern of mentionPatterns) {
      if (messageText.includes(pattern)) {
        return true;
      }
    }

    // Check for channel/directive patterns (if needed)
    // TODO: Add channel directive support when that feature exists

    return false;
  }

  /**
   * Check if a sender is a human user (not AI/persona/agent)
   * CRITICAL for preventing infinite response loops between AI users
   */
  private async isSenderHuman(senderId: UUID): Promise<boolean> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot check sender type - no client`);
      return true; // Fail open - allow response if we can't verify
    }

    try {
      // Query the sender's UserEntity to check their type
      const result = await this.client.daemons.commands.execute<DataReadParams, DataReadResult<UserEntity>>('data/read', {
        collection: COLLECTIONS.USERS,
        id: senderId,
        context: this.client.context,
        sessionId: this.client.sessionId,
        backend: 'server'
      });

      if (!result.success || !result.found || !result.data) {
        console.warn(`⚠️  PersonaUser ${this.displayName}: Could not read sender ${senderId}, assuming human`);
        return true; // Fail open
      }

      const senderType = result.data.type;
      const isHuman = senderType === 'human';

      console.log(`🔍 PersonaUser ${this.displayName}: Sender ${senderId} type is "${senderType}" (human: ${isHuman})`);
      return isHuman;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Error checking sender type:`, error);
      return true; // Fail open on error
    }
  }

  /**
   * Get persona database path
   */
  getPersonaDatabasePath(): string {
    return `.continuum/personas/${this.entity.id}/state.sqlite`;
  }

  /**
   * RAG Context Storage - Store conversation context for a room
   * Enables persona to maintain context across sessions
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, store in memory until artifact commands are implemented
   */
  async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot store RAG context - no client`);
      return;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // await this.client.daemons.artifacts.writeJSON(...)
    console.log(`📝 PersonaUser ${this.displayName}: RAG context stored (in-memory) for room ${roomId}`);
  }

  /**
   * RAG Context Loading - Load conversation context for a room
   * Returns null if no context exists yet
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, return null until artifact commands are implemented
   */
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot load RAG context - no client`);
      return null;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // return await this.client.daemons.artifacts.readJSON<PersonaRAGContext>(...)
    console.log(`📭 PersonaUser ${this.displayName}: No RAG context for room ${roomId} (not yet implemented)`);
    return null;
  }

  /**
   * Update RAG Context - Add new message to context and trim if needed
   */
  async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    // Load existing context or create new
    let context = await this.loadRAGContext(roomId);
    if (!context) {
      context = {
        roomId,
        personaId: this.id,
        messages: [],
        lastUpdated: new Date().toISOString(),
        tokenCount: 0
      };
    }

    // Add new message to context
    context.messages.push({
      senderId: message.senderId,
      senderName: message.senderName,
      text: message.content?.text || '',
      timestamp: typeof message.timestamp === 'string' ? message.timestamp : message.timestamp.toISOString()
    });

    // Keep only last 50 messages (simple context window for now)
    if (context.messages.length > 50) {
      context.messages = context.messages.slice(-50);
    }

    context.lastUpdated = new Date().toISOString();

    // Store updated context
    await this.storeRAGContext(roomId, context);
  }

  /**
   * PersonaUser creation recipe
   *
   * ARCHITECTURE NOTE: Creation still uses DataDaemon for now
   * - DataDaemon is the internal data layer (system-level operations)
   * - Commands.execute() is for user-level operations (PersonaUser responding to chat)
   * - This maintains proper abstraction: creation is system concern, chat is user concern
   *
   * Recipe steps:
   * 1. Create UserEntity in database
   * 2. Create UserStateEntity in database with persona defaults
   * 3. Add to rooms if specified
   * 4. Return PersonaUser instance (UserDaemon will create persistent instance)
   */
  static async create(
    params: UserCreateParams,
    _context: JTAGContext,
    _router: JTAGRouter
  ): Promise<PersonaUser> {
    // STEP 1: Create UserEntity in database
    const userEntity = new UserEntity();
    userEntity.type = 'persona';
    userEntity.uniqueId = params.uniqueId;
    userEntity.displayName = params.displayName;
    userEntity.status = params.status ?? 'online';
    userEntity.lastActiveAt = new Date();
    userEntity.capabilities = params.capabilities ?? getDefaultCapabilitiesForType('persona');
    userEntity.sessionsActive = [];
    // Optional extended fields for personas
    if (params.modelConfig) {
      Object.assign(userEntity, { modelConfig: params.modelConfig });
    }
    // createdAt, updatedAt, version, id handled by constructor

    const storedEntity = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity with persona-specific defaults
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('persona');

    const storedState = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      userState
    );

    // STEP 3: Auto-join "general" room (all users start here)
    try {
      console.log(`🚪 PersonaUser.create: Adding ${params.displayName} to general room...`);
      await this.addToGeneralRoom(storedEntity.id, params.displayName);
      console.log(`✅ PersonaUser.create: Successfully added ${params.displayName} to general room`);
    } catch (error) {
      console.error(`❌ PersonaUser.create: Failed to add to general room:`, error);
    }

    // STEP 4: Add persona to additional rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(storedEntity.id, roomId, params.displayName);
      }
    }

    // STEP 5: Create PersonaUser instance (client injected by UserDaemon)
    const storage = new MemoryStateBackend();
    return new PersonaUser(storedEntity, storedState, storage, undefined);
  }

}