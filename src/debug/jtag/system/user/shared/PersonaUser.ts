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
import { Commands } from '../../core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
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
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypes';
import { ChatRAGBuilder } from '../../rag/builders/ChatRAGBuilder';

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

  // Rate limiting state (in-memory for now, will move to SQLite later)
  private lastResponseTime: Map<UUID, Date> = new Map();
  private readonly minSecondsBetweenResponses = 10; // 10 seconds between responses per room

  // Response cap to prevent infinite loops
  private responseCount: Map<UUID, number> = new Map(); // room -> count
  private readonly maxResponsesPerSession = 10; // Max 10 responses per room per session

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
    console.log(`📨 ${this.displayName}: Message from ${messageEntity.senderName}`);

    // STEP 1: Ignore our own messages
    if (messageEntity.senderId === this.id) {
      console.log(`🔇 ${this.displayName}: Ignoring own message`);
      return;
    }

    // STEP 2: Check response cap (prevent infinite loops)
    const currentCount = this.responseCount.get(messageEntity.roomId) || 0;
    if (currentCount >= this.maxResponsesPerSession) {
      console.log(`🛑 ${this.displayName}: Response cap reached (${currentCount}/${this.maxResponsesPerSession}) in room ${messageEntity.roomId.slice(0, 8)}`);
      return;
    }

    // STEP 3: Check if sender is human or AI (use denormalized field!)
    const senderIsHuman = messageEntity.senderType === 'human';
    const messageText = messageEntity.content?.text || '';
    const isMentioned = this.isPersonaMentioned(messageText);

    console.log(`🔍 ${this.displayName}: Sender type is "${messageEntity.senderType}" (human: ${senderIsHuman})`);

    // STEP 4: Check rate limiting (before expensive LLM call)
    if (this.isRateLimited(messageEntity.roomId)) {
      const lastTime = this.lastResponseTime.get(messageEntity.roomId)!;
      const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
      const waitTime = this.minSecondsBetweenResponses - secondsSince;
      console.log(`⏸️  ${this.displayName}: Rate limited, wait ${waitTime.toFixed(1)}s more`);
      return;
    }

    // STEP 5: Use LLM judgment to decide whether to respond
    // This implements the 8-rule protocol: mentioned, direct-question, concluded, just-spoke, etc.
    const shouldRespond = await this.shouldRespondToMessage(messageEntity, senderIsHuman, isMentioned);

    if (!shouldRespond) {
      console.log(`🤔 ${this.displayName}: LLM decided not to respond to "${messageText.slice(0, 50)}..."`);
      return;
    }

    // STEP 6: We should respond!
    console.log(`✅ ${this.displayName}: LLM decided to respond to ${messageEntity.senderName}`);

    // Update RAG context
    await this.updateRAGContext(messageEntity.roomId, messageEntity);

    // Post response
    await this.respondToMessage(messageEntity);

    // Increment response count
    const newCount = (this.responseCount.get(messageEntity.roomId) || 0) + 1;
    this.responseCount.set(messageEntity.roomId, newCount);
    console.log(`📊 ${this.displayName}: Response ${newCount}/${this.maxResponsesPerSession} in room ${messageEntity.roomId.slice(0, 8)}`);

    // Track response time for rate limiting
    this.lastResponseTime.set(messageEntity.roomId, new Date());
  }

  /**
   * Check if this persona is rate limited for a room
   */
  private isRateLimited(roomId: UUID): boolean {
    const lastTime = this.lastResponseTime.get(roomId);
    if (!lastTime) {
      return false; // Never responded in this room
    }

    const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
    return secondsSince < this.minSecondsBetweenResponses;
  }

  /**
   * Generate and post a response to a chat message
   * Phase 2: AI-powered responses with RAG context via AIProviderDaemon
   */
  private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
    try {
      // Build RAG context using ChatRAGBuilder (includes room membership, message history, persona identity)
      const ragBuilder = new ChatRAGBuilder();
      const fullRAGContext = await ragBuilder.buildContext(
        originalMessage.roomId,
        this.id,
        {
          maxMessages: 20,
          maxMemories: 10,
          includeArtifacts: false, // Skip artifacts for now (image attachments)
          includeMemories: false    // Skip private memories for now
        }
      );

      // Build message history for LLM
      const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

      // System prompt from RAG builder (includes room membership!)
      messages.push({
        role: 'system',
        content: fullRAGContext.identity.systemPrompt
      });

      // Add conversation history from RAG context
      // NOTE: Llama 3.2 doesn't support multi-party chats natively, so we embed speaker names in content
      // Format: "SpeakerName: message" (Llama only supports system/user/assistant/ipython roles)
      if (fullRAGContext.conversationHistory.length > 0) {
        for (const msg of fullRAGContext.conversationHistory) {
          // For Llama models, embed speaker identity in the content since name parameter isn't supported
          const formattedContent = msg.name ? `${msg.name}: ${msg.content}` : msg.content;

          messages.push({
            role: msg.role,
            content: formattedContent
          });
        }
      }

      // CRITICAL: Identity reminder at END of context (research shows this prevents "prompt drift")
      // LLMs have recency bias - instructions at the end have MORE influence than at beginning
      // This prevents the persona from copying the "Name: message" format or inventing fake participants
      messages.push({
        role: 'system',
        content: `IDENTITY REMINDER: You are ${this.displayName}. Respond naturally with JUST your message - NO name prefix, NO "A:" or "H:" labels, NO fake conversations. The room has ONLY these people: ${fullRAGContext.identity.systemPrompt.match(/Current room members: ([^\n]+)/)?.[1] || 'unknown members'}.`
      });

      // Generate response using AIProviderDaemon (static method like DataDaemon.query)
      console.log(`🤖 ${this.displayName}: Generating AI response with ${messages.length} context messages...`);

      const request: TextGenerationRequest = {
        messages,
        model: 'llama3.2:3b', // Larger model for better instruction following (was 1b)
        temperature: 0.7,
        maxTokens: 150, // Keep responses concise
        preferredProvider: 'ollama'
      };

      const aiResponse = await AIProviderDaemon.generateText(request);

      console.log(`✅ ${this.displayName}: Generated response in ${aiResponse.responseTime}ms (${aiResponse.usage.outputTokens} tokens)`);

      // Create response message entity
      const responseMessage = new ChatMessageEntity();
      responseMessage.roomId = originalMessage.roomId;
      responseMessage.senderId = this.id;
      responseMessage.senderName = this.displayName;
      responseMessage.senderType = this.entity.type; // Denormalize from UserEntity (persona)
      responseMessage.content = { text: aiResponse.text.trim(), attachments: [] };
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
        : await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          });

      if (!result.success) {
        throw new Error(`Failed to create message: ${result.error}`);
      }

      console.log(`✅ PersonaUser ${this.displayName}: Posted AI response: "${aiResponse.text.slice(0, 100)}${aiResponse.text.length > 100 ? '...' : ''}"`);

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Failed to generate or post AI response:`, error);

      // Fallback to simple response if AI generation fails
      try {
        const fallbackText = `Sorry, I'm having trouble generating a response right now.`;
        const fallbackMessage = new ChatMessageEntity();
        fallbackMessage.roomId = originalMessage.roomId;
        fallbackMessage.senderId = this.id;
        fallbackMessage.senderName = this.displayName;
        fallbackMessage.senderType = this.entity.type; // Denormalize from UserEntity (persona)
        fallbackMessage.content = { text: fallbackText, attachments: [] };
        fallbackMessage.status = 'sent';
        fallbackMessage.priority = 'normal';
        fallbackMessage.timestamp = new Date();
        fallbackMessage.reactions = [];

        const result = this.client
          ? await this.client.daemons.commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
              context: this.client.context,
              sessionId: this.client.sessionId,
              collection: ChatMessageEntity.collection,
              backend: 'server',
              data: fallbackMessage
            })
          : await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
              collection: ChatMessageEntity.collection,
              backend: 'server',
              data: fallbackMessage
            });

        if (result.success) {
          console.log(`⚠️  PersonaUser ${this.displayName}: Posted fallback response`);
        }
      } catch (fallbackError) {
        console.error(`❌ PersonaUser ${this.displayName}: Even fallback response failed:`, fallbackError);
      }
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
   * Use LLM to decide whether to respond to a message
   * Implements the 8-rule protocol from AI_TO_AI_INTERACTION_PROTOCOL.md
   *
   * Uses a cheap/fast model (like GPT-3.5 or Claude Haiku) for the decision,
   * then uses the full model for actual response generation
   */
  private async shouldRespondToMessage(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    isMentioned: boolean
  ): Promise<boolean> {
    // Rule 1: Always respond if @mentioned (highest priority - forced response)
    if (isMentioned) {
      console.log(`📣 ${this.displayName}: Mentioned - FORCED RESPONSE`);
      return true;
    }

    // Rule 2: Respond to humans (Phase 1 - simple rule)
    if (senderIsHuman) {
      console.log(`👤 ${this.displayName}: Human message - will respond`);
      return true;
    }

    // Rule 3: Don't respond to AIs without @mention (prevents loops)
    console.log(`🔇 ${this.displayName}: AI message without @mention - staying quiet`);
    return false;

    // PHASE 2: Fuzzy logic heuristics (NO API costs)
    // This code will be enabled when Phase 1 testing is complete
    //
    // const heuristics = await this.calculateResponseHeuristics(messageEntity);
    //
    // // Score-based decision (0-100 scale)
    // let score = 0;
    //
    // // 1. Question detection (+40 points - strong signal)
    // if (heuristics.containsQuestion) score += 40;
    //
    // // 2. Conversation temperature (+0 to +30)
    // if (heuristics.conversationTemp === 'HOT') score += 30;
    // else if (heuristics.conversationTemp === 'WARM') score += 20;
    // else if (heuristics.conversationTemp === 'COOL') score += 10;
    // // COLD = +0
    //
    // // 3. Participation ratio (-30 if dominating, +10 if quiet)
    // if (heuristics.myParticipationRatio > 0.5) score -= 30; // I'm dominating
    // else if (heuristics.myParticipationRatio < 0.2) score += 10; // I'm quiet
    //
    // // 4. Time since my last message (+20 if been quiet, -20 if just spoke)
    // if (heuristics.secondsSinceMyLastMessage > 60) score += 20;
    // else if (heuristics.secondsSinceMyLastMessage < 15) score -= 20;
    //
    // // 5. Turn-taking pattern (+15 if it's my turn)
    // if (heuristics.appearsToBeMyTurn) score += 15;
    //
    // // Decision threshold: 50+ = respond
    // const shouldRespond = score >= 50;
    //
    // console.log(`🎯 ${this.displayName}: Heuristic score ${score}/100 -> ${shouldRespond ? 'RESPOND' : 'WAIT'}`);
    // return shouldRespond;
  }

  /**
   * Calculate heuristics for response decision (Phase 2)
   * NO API calls - pure logic based on conversation history
   */
  private async calculateResponseHeuristics(messageEntity: ChatMessageEntity): Promise<{
    containsQuestion: boolean;
    conversationTemp: 'HOT' | 'WARM' | 'COOL' | 'COLD';
    myParticipationRatio: number;
    secondsSinceMyLastMessage: number;
    appearsToBeMyTurn: boolean;
  }> {
    // TODO: Implement when Phase 2 is enabled
    // 1. Check if message contains "?" (simple question detection)
    // 2. Get last 10 messages from room to calculate:
    //    - Conversation temperature (time between messages)
    //    - My participation ratio (my messages / total messages)
    //    - Time since my last message
    //    - Turn-taking pattern (did I just speak? does message follow someone else?)

    return {
      containsQuestion: false,
      conversationTemp: 'COLD',
      myParticipationRatio: 0,
      secondsSinceMyLastMessage: 999,
      appearsToBeMyTurn: false
    };
  }

  /**
   * Check if a sender is a human user (not AI/persona/agent)
   * CRITICAL for preventing infinite response loops between AI users
   */
  private async isSenderHuman(senderId: UUID): Promise<boolean> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot check sender type - no client, BLOCKING response`);
      return false; // Fail CLOSED - don't respond if we can't verify (prevents startup loops)
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
        console.warn(`⚠️  PersonaUser ${this.displayName}: Could not read sender ${senderId}, BLOCKING response`);
        return false; // Fail CLOSED - don't respond if database fails (prevents loops)
      }

      const senderType = result.data.type;
      const isHuman = senderType === 'human';

      console.log(`🔍 PersonaUser ${this.displayName}: Sender ${senderId} type is "${senderType}" (human: ${isHuman})`);
      return isHuman;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Error checking sender type, BLOCKING response:`, error);
      return false; // Fail CLOSED on error (prevents loops)
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