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
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';

/**
 * PersonaUser - Our internal AI citizens
 *
 * First-class citizens with their own JTAGClient for universal Commands/Events API
 */
export class PersonaUser extends AIUser {
  private isInitialized: boolean = false;
  private client?: JTAGClient; // Injected via constructor

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage);
    this.client = client;
    if (client) {
      console.log(`🔌 PersonaUser ${this.displayName}: Client injected`);
    }
  }

  /**
   * Initialize persona - use BaseUser helpers for room management and event subscriptions
   * PersonaUser acts as an autonomous agent, listening for messages in rooms they're a member of
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      console.log(`ℹ️ PersonaUser ${this.displayName}: Already initialized`);
      return;
    }

    console.log(`🤖 PersonaUser ${this.displayName}: Initializing...`);

    // STEP 1: Base initialization (loads state + rooms)
    await super.initialize();

    // STEP 2: Subscribe to chat events using BaseUser helper
    this.subscribeToChatEvents(this.handleChatMessage.bind(this));

    // STEP 3: Subscribe to room updates using BaseUser helper
    this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));

    this.isInitialized = true;
    console.log(`✅ PersonaUser ${this.displayName}: Initialized with ${this.myRoomIds.size} rooms`);
  }

  /**
   * Handle incoming chat message
   * Decides whether to respond based on room membership and other factors
   */
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // Ignore our own messages
    if (messageEntity.senderId === this.id) {
      return;
    }

    console.log(`💬 PersonaUser ${this.displayName}: Received message from ${messageEntity.senderName}`);

    // Simple response logic (Phase 1 - no AI API yet)
    // Respond with probability 0.3 to avoid spam
    if (Math.random() > 0.3) {
      console.log(`🤫 PersonaUser ${this.displayName}: Choosing not to respond`);
      return;
    }

    // Generate simple templated response
    await this.respondToMessage(messageEntity);
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
   * Get persona database path
   */
  getPersonaDatabasePath(): string {
    return `.continuum/personas/${this.entity.id}/state.sqlite`;
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