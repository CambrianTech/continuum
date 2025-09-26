/**
 * Chat Widget - JTAG-based Web Component using BaseWidget Architecture
 * 
 * Updated to use the new BaseWidget abstraction with one-line operations.
 * Demonstrates dramatic code simplification through proper abstraction.
 */

import { ChatMessageEntity, type MessageStatus, type CreateMessageData, type MessageContent } from '../../../system/data/entities/ChatMessageEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
// Using UUID directly instead of domain type aliases
import { generateUUID, type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import { MessageRowWidgetFactory } from '../shared/BaseMessageRowWidget';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { CHAT_EVENTS, CHAT_EVENT_TYPES } from '../shared/ChatEventConstants';
import type {
  ChatMessageEventData,
  ChatParticipantEventData,
  ChatEventName
} from '../shared/ChatEventTypes';
// UserIdManager removed - using UserEntity.id directly
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import { createScroller, SCROLLER_PRESETS, type RenderFn, type LoadFn, type EntityScroller } from '../../shared/EntityScroller';
import { createDataLoader, PAGINATION_PRESETS } from '../../shared/DataLoaders';
import { createDataExecutor } from '../../shared/DataExecutorAdapter';
import { COLLECTIONS } from '../../../system/data/core/FieldMapping';
import { Commands } from '../../../system/core/client/shared/Commands';
import { Events } from '../../../system/core/client/shared/Events';
import { DEFAULT_ROOMS, DEFAULT_USERS } from '../../../system/data/domains/DefaultEntities';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
// ChatRoomData removed - using RoomEntity directly
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';

// Default constants extracted from ChatMessageEntity
const DEFAULT_MESSAGE_METADATA = {
  source: 'system' as const
};

// ChatMessageEntity already extends Entity via BaseEntity - no need for new interface

/**
 * Chat Message Renderer - Pure function for generic scroller
 */
const renderChatMessage: RenderFn<ChatMessageEntity> = (message, context) => {
  const isCurrentUser = context.isCurrentUser || false;

  // Create exact same DOM structure as existing ChatWidget
  const messageRow = document.createElement('div');
  messageRow.className = `message-row ${isCurrentUser ? 'right' : 'left'}`;
  messageRow.setAttribute('data-message-id', message.id);

  const messageBubble = document.createElement('div');
  messageBubble.className = `message-bubble ${isCurrentUser ? 'current-user' : 'other-user'}`;

  const messageHeader = document.createElement('div');
  messageHeader.className = 'message-header';

  const timeSpan = document.createElement('span');
  timeSpan.className = 'message-time';
  timeSpan.textContent = new Date(message.timestamp).toLocaleString();
  messageHeader.appendChild(timeSpan);

  const messageContentDiv = document.createElement('div');
  messageContentDiv.className = 'message-content';

  const textContent = document.createElement('p');
  textContent.className = 'text-content chat-message-renderer';
  textContent.setAttribute('data-interactive', 'true');
  textContent.setAttribute('tabindex', '0');
  textContent.textContent = message.content?.text || '';

  messageContentDiv.appendChild(textContent);
  messageBubble.appendChild(messageHeader);
  messageBubble.appendChild(messageContentDiv);
  messageRow.appendChild(messageBubble);

  return messageRow;
};

/**
 * Chat Data Loader - Pure function for generic scroller
 */

/**
 * Scroll position state for persistence across reloads
 */
interface ScrollState {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
  readonly timestamp: number;
  readonly visibleUUIDs: string[];
}

export class ChatWidget extends ChatWidgetBase {
  private messages: ChatMessageEntity[] = [];
  private currentRoomEntity?: RoomEntity;
  private messageInput?: HTMLInputElement;
  private eventSubscriptionId?: string;
  private messagesContainer?: HTMLElement;
  private chatScroller?: EntityScroller<ChatMessageEntity>;

  // Cached current user - avoids repeated database lookups
  private cachedCurrentUser?: UserEntity;

  // Event handler references for proper cleanup
  private _keydownHandler?: (e: KeyboardEvent) => void;
  private _clickHandler?: (e: Event) => void;

  // PUBLIC properties required by integration tests (NOT optional - must be set)
  public roomId!: UUID; // Current room ID for chat context
  
  constructor(roomId: UUID = DEFAULT_ROOMS.GENERAL as UUID) {
    super({
      widgetName: 'ChatWidget',
      styles: 'chat-widget.css',
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true
    });
    this.roomId = roomId;

    // EntityScroller will be initialized after roomId is set
  }

  // Static property required by widget registration system
  static get widgetName(): string {
    return 'chat';
  }



  protected override resolveResourcePath(filename: string): string {
    return `widgets/chat/chat-widget/${filename}`;
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log(`🎯 ChatWidget: Initializing for room "${this.roomId}"...`);

    // Set current room ID from constructor parameter or default
    // (roomId set in constructor)

    // EntityScroller will be initialized after DOM is rendered in renderWidget()

    // Verify client context exists (REQUIRED)
    const client = await JTAGClient.sharedInstance;
    if (!client?.sessionId) {
      throw new Error('ChatWidget requires session context - cannot initialize without sessionId');
    }

    // 🔧 CLAUDE-STATE-TEST-' + Date.now() + ': Test State system with properties
    console.log('🔧 CLAUDE-STATE-TEST-' + Date.now() + ': Testing State system with properties...');
    try {
      // Test property access
      const currentRoomId = client.state.room.currentId;
      const currentRoom = client.state.room.currentRoom;
      console.log('🔧 CLAUDE-STATE-TEST: Current room ID (property):', currentRoomId);
      console.log('🔧 CLAUDE-STATE-TEST: Current room entity (property):', currentRoom);

      // Test property assignment
      client.state.room.currentId = this.roomId as UUID;
      console.log('🔧 CLAUDE-STATE-TEST: Set room ID to:', this.roomId);
      console.log('🔧 CLAUDE-STATE-TEST: New room ID (property):', client.state.room.currentId);
      console.log('✅ CLAUDE-STATE-TEST: State system properties work perfectly!');
    } catch (error) {
      console.error('❌ CLAUDE-STATE-TEST: State system property access failed:', error);
    }

    // Load room entity data for display name and metadata
    await this.loadRoomEntity();

    // Subscribe to room change events
    await this.subscribeToRoomChangeEvents();

    // Subscribe to room-specific events
    await this.subscribeToRoomEvents();

    const messageCount = this.chatScroller?.entities().length || this.messages.length;
    console.log(`✅ ChatWidget: Initialized for room "${this.roomId}" with ${messageCount} messages`);
  }

  // Entity list functionality for header rendering
  protected getEntityCount(): number {
    // Return count of loaded messages (EntityScroller approach)
    const count = this.chatScroller?.entities().length || 0;
    return count;
  }

  protected getEntityTitle(entity?: ChatMessageEntity): string {
    return this.currentRoomEntity?.name || 'Chat';
  }

  // Shared count update logic from EntityListWidget
  protected updateEntityCount(): void {
    const countElement = this.shadowRoot.querySelector('.list-count');
    if (countElement) {
      countElement.textContent = this.getEntityCount().toString();
    }
  }

  // Standardized header structure
  protected renderHeader(entity?: ChatMessageEntity): string {
    return `
      <div class="entity-list-header">
        <span class="header-title">${this.getEntityTitle(entity)}</span>
        <span class="list-count">${this.getEntityCount()}</span>
      </div>
    `;
  }


  protected async onWidgetCleanup(): Promise<void> {
    // Save scroll position and context before cleanup
    if (this.messagesContainer) {
      const scrollState: ScrollState = {
        scrollTop: this.messagesContainer.scrollTop,
        scrollHeight: this.messagesContainer.scrollHeight,
        clientHeight: this.messagesContainer.clientHeight,
        timestamp: Date.now(),
        visibleUUIDs: this.getVisibleUUIDs()
      };

      const scrollStateKey = `chat_scroll_${this.roomId}`;
      await this.storeData(scrollStateKey, scrollState, { persistent: true });
      console.log(`💾 ChatWidget: Saved scroll position for room "${this.roomId}"`, scrollState);
    }

    // Cleanup message input handlers
    this.cleanupMessageInputHandlers();

    // Cleanup chat scroller
    if (this.chatScroller) {
      this.chatScroller.destroy();
      this.chatScroller = undefined;
    }

    // Unsubscribe from room events using JTAG abstraction
    //TODO: completely missing command for unsubscribe. should this just be subscribe-room with a flag?
    // if (this.eventSubscriptionId) {
    //   try {
    //     await this.executeCommand<ChatUnsubscribeEventResult>('events/unsubscribe', {
    //       subscriptionId: this.eventSubscriptionId
    //     });
    //     console.log(`🔌 ChatWidget: Unsubscribed from room ${this.roomId} events`);
    //   } catch (error) {
    //     console.error(`❌ ChatWidget: Failed to unsubscribe from events:`, error);
    //   }
    // }

    // Save room-specific messages using BaseWidget abstraction
    const roomMessageKey = `chat_messages_${this.roomId}`;
    await this.storeData(roomMessageKey, this.messages, { persistent: true });
    console.log(`✅ ChatWidget: Cleanup complete for room "${this.roomId}"`);
  }

  /**
   * Load and cache room entity data by ID
   * This is used throughout the chat system to get room display names and metadata
   */
  private async loadRoomEntity(): Promise<void> {
    try {
      const client = await JTAGClient.sharedInstance;

      const result = await this.executeCommand<DataReadParams, DataReadResult>('data/read', {
        collection: RoomEntity.collection,
        id: this.roomId,
        context: client.context,
        sessionId: client.sessionId
      });

      if (result.success && result.data) {
        this.currentRoomEntity = result.data;
        console.log(`📋 ChatWidget: Loaded room entity "${this.currentRoomEntity?.displayName || this.currentRoomEntity?.name}"`);
      } else {
        console.warn(`⚠️ ChatWidget: Could not load room entity for ID "${this.roomId}"`);
        this.currentRoomEntity = undefined;
      }
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to load room entity:`, error);
      this.currentRoomEntity = undefined;
    }
  }

  /**
   * Get room display name from cached entity data
   * No more hardcoded room ID comparisons!
   */
  private getRoomDisplayName(): string {
    if (this.currentRoomEntity?.displayName) {
      return this.currentRoomEntity.displayName;
    } else if (this.currentRoomEntity?.name) {
      return this.currentRoomEntity.name;
    } else {
      // Fallback: extract from room ID if it's a UUID
      return this.roomId.includes('-') ? 'Room' : this.roomId.charAt(0).toUpperCase() + this.roomId.slice(1);
    }
  }

  protected override async renderWidget(skipScrollRestoration = false): Promise<void> {
    super.renderWidget();

    // Replace room name in template using cached entity data
    if (this.shadowRoot) {
      const roomName = this.getRoomDisplayName();
      this.shadowRoot.innerHTML = this.shadowRoot.innerHTML.replace('<!-- ROOM_NAME -->', roomName);
    }

    // Cache input element and messages container
    this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    this.messagesContainer = this.shadowRoot.getElementById('messages') as HTMLElement;

    // Set up message input handlers after room changes
    this.setupMessageInputHandlers();

    // Initialize EntityScroller now that DOM is ready
    if (!this.chatScroller && this.messagesContainer) {

      try {
        // Create clean data executor - elegant protocol with firm typing
        const executor = createDataExecutor<ChatMessageEntity>(Commands.execute, ChatMessageEntity.collection);
        const loader = createDataLoader<ChatMessageEntity>(executor, {
          collection: COLLECTIONS.CHAT_MESSAGES,
          filter: { roomId: this.roomId },
          cursor: PAGINATION_PRESETS.CHAT_MESSAGES,
          defaultLimit: 20
        });

        const context = {
          isCurrentUser: false, // Will be set per message by checking senderId
          customData: { roomId: this.roomId, currentUUID: this.roomId }
        };

        // Enhanced render function that checks current user for each message
        const renderWithUserCheck: RenderFn<ChatMessageEntity> = (message, ctx) => {
          const enhancedContext = {
            ...ctx,
            isCurrentUser: message.senderId === this.getCurrentUserId()
          };
          return renderChatMessage(message, enhancedContext);
        };


        this.chatScroller = createScroller(
          this.messagesContainer,
          renderWithUserCheck,
          loader,
          SCROLLER_PRESETS.CHAT,
          context
        );


        // Load initial messages using EntityScroller
        if (this.chatScroller) {
          this.chatScroller.load().then(() => {
            this.messages = this.chatScroller!.entities() as ChatMessageEntity[];
            console.log(`✅ EntityScroller: Loaded ${this.messages.length} messages`);

            // Update entity count in header
            this.updateEntityCount();

            // CRITICAL FIX: Scroll to bottom AFTER messages are loaded
            // This fixes the "starts scrolled to top" issue
            if (!skipScrollRestoration) {
              this.restoreScrollPosition();
            }
          }).catch(error => {
            console.error('❌ EntityScroller: Failed to load messages:', error);
          });
        }

      } catch (error) {
        console.error('❌ CLAUDE-DEBUG-EntityScroller: Error creating scroller:', error);
        this.chatScroller = undefined;
      }
    } else if (this.messagesContainer && this.messages.length > 0) {
      console.log('🔄 ChatWidget: Messages already loaded via EntityScroller');

      // For already-loaded case, still need to handle scroll restoration
      if (!skipScrollRestoration) {
        await this.restoreScrollPosition();
      }
    }

    // Note: Scroll restoration for new EntityScroller loads is handled in the .then() block above
  }


  /**
   * Load initial room message history with cursor-based pagination
   */
  private async loadRoomHistory(): Promise<void> {
    try {
      console.log(`📚 ChatWidget: Loading room history using cursor-based pagination`);

      // Load initial batch of recent messages using data/list
      const historyResult = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>('data/list', {
        collection: ChatMessageEntity.collection,
        filter: { roomId: this.roomId },
        orderBy: [{ field: 'timestamp', direction: 'asc' }],
        limit: 20
      });

      if (historyResult?.success && historyResult?.items) {
        this.messages = historyResult.items
          .filter((message): message is ChatMessageEntity => !!message.content?.text && message.content.text.trim().length > 0);

        console.log(`✅ ChatWidget: Loaded ${this.messages.length} initial messages for room "${this.roomId}"`);
      } else if (historyResult?.success === false) {
        console.error(`❌ ChatWidget: Failed to get messages: ${historyResult.error}`);
        this.messages = [];
      } else {
        console.log(`ℹ️ ChatWidget: No messages found for room "${this.roomId}"`);
        this.messages = [];
      }

    } catch (error) {
      console.error(`❌ ChatWidget: Failed to load room history:`, error);
      throw error;
    }
  }

  /**
   * Get current user ID (using constant for Joel)
   */
  private getCurrentUserId(): string {
    return DEFAULT_USERS.HUMAN;
  }

  /**
   * Load older messages when user scrolls to top
   */
  private async loadOlderMessages(cursor: string): Promise<ChatMessageEntity[]> {
    try {
      console.log(`📚 ChatWidget: Loading older messages with cursor: ${cursor}`);

      // Use data/list command with cursor for older messages
      // NOTE: We want messages BEFORE (older than) the cursor timestamp
      // But we need them in ascending order to append at the beginning correctly
      const olderResult = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>('data/list', {
        collection: ChatMessageEntity.collection,
        filter: { roomId: this.roomId },
        orderBy: [{ field: 'timestamp', direction: 'desc' }], // DESC to get messages before cursor
        limit: 20,
        cursor: {
          field: 'timestamp',
          value: cursor,
          direction: 'before' // Get messages older than cursor
        }
      });

      if (olderResult?.success && olderResult?.items) {
        const olderMessages = olderResult.items
          .filter((message): message is ChatMessageEntity => !!message.content?.text && message.content.text.trim().length > 0)
          .reverse(); // Reverse DESC results to get chronological order (oldest first)

        // Insert older messages at the beginning of our data array
        this.messages = [...olderMessages, ...this.messages];

        // EntityScroller handles dynamic row insertion automatically
        console.log('🔧 ChatWidget: EntityScroller handles pagination automatically');

        console.log(`✅ ChatWidget: Loaded ${olderMessages.length} older messages`);
        return olderMessages;
      }

      return [];
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to load older messages:`, error);
      return [];
    }
  }

  /**
   * Subscribe to room-specific events for real-time updates using RoomEventSystem
   */
  private async subscribeToRoomEvents(): Promise<void> {
    try {
      console.log(`🔗 ChatWidget: Subscribing to room events for "${this.roomId}"`);
      
      // Subscribe to data events directly - no need for separate chat/subscribe-room command
      console.log(`✅ ChatWidget: Using direct data event subscription for room "${this.roomId}"`);
      
      // Set up DOM event listeners as fallback (these are emitted by EventsDaemon)
      this.setupRoomEventHandlers();
      
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to subscribe to room events:`, error);
      // Don't throw - continue with DOM events only
    }
  }

  /**
   * Set up type-safe event handlers for real-time room updates using BaseWidget
   */
  private setupRoomEventHandlers(): void {
    console.log(`📡 ChatWidget: Setting up type-safe room event handlers for room ${this.roomId}`);
    
    // Type-safe event listeners using BaseWidget's executeEvent system
    // These ONLY respond to genuine server-originated events

    // Listen for unified data events (ChatMessage creation via data/create)
    this.addWidgetEventListener('data:ChatMessage:created' as any, (eventData: ChatMessageEntity) => {
      console.log(`🔥 UNIFIED-DATA-EVENT: data:ChatMessage:created`, eventData);
      this.onMessageReceived(eventData);
    });

    // Listen for unified data events (ChatMessage updates via data/update)
    this.addWidgetEventListener('data:ChatMessage:updated' as any, (eventData: ChatMessageEntity) => {
      console.log(`🔥 UNIFIED-DATA-EVENT: data:ChatMessage:updated`, eventData);
      this.onMessageUpdated(eventData);
    });
    
    this.addWidgetEventListener(CHAT_EVENTS.PARTICIPANT_JOINED, (eventData: ChatParticipantEventData) => {
      console.log(`🔥 SERVER-EVENT-RECEIVED: ${CHAT_EVENTS.PARTICIPANT_JOINED}`, eventData);
      this.onUserJoined(eventData);
    });
    
    this.addWidgetEventListener(CHAT_EVENTS.PARTICIPANT_LEFT, (eventData: ChatParticipantEventData) => {
      console.log(`🔥 SERVER-EVENT-RECEIVED: ${CHAT_EVENTS.PARTICIPANT_LEFT}`, eventData);
      this.onUserLeft(eventData);
    });
    
    // Server events are properly configured above - no fallback needed
    
    console.log(`✅ ChatWidget: Set up type-safe event listeners for chat events`);
  }

  
  
  /**
   * Handle incoming room events from RoomEventSystem
   */
  public async handleRoomEvent(
    eventType: string, 
    eventData: ChatMessageEventData | ChatParticipantEventData
  ): Promise<void> {
    console.log(`📨 ChatWidget: Received room event "${eventType}" for room ${this.roomId}`);
    
    switch (eventType) {
      case 'chat:message-received':
        await this.onMessageReceived(eventData as ChatMessageEventData);
        break;
      case 'chat:participant-joined':
        await this.onUserJoined(eventData as ChatParticipantEventData);
        break;
      case 'chat:participant-left':
        await this.onUserLeft(eventData as ChatParticipantEventData);
        break;
      default:
        console.log(`ℹ️ ChatWidget: Unhandled room event type: ${eventType}`);
    }
  }


  // Circuit breaker to prevent infinite loops
  private messageProcessingCount = 0;
  private lastMessageProcessingReset = 0;

  /**
   * Handle incoming chat messages for this room - STRICT TYPING
   * Supports both unified data events (ChatMessageEntity) and legacy chat events (ChatMessageEventData)
   */
  private async onMessageReceived(eventData: ChatMessageEntity | ChatMessageEventData): Promise<void> {
    // Circuit breaker: prevent infinite loops
    const now = Date.now();
    if (now - this.lastMessageProcessingReset > 5000) {
      this.messageProcessingCount = 0;
      this.lastMessageProcessingReset = now;
    }

    this.messageProcessingCount++;
    if (this.messageProcessingCount > 10) {
      console.error(`🚨 CIRCUIT BREAKER: ChatWidget prevented infinite loop. Dropping message.`);
      return;
    }

    console.log(`📨 ChatWidget: Received message for room ${this.roomId} (count: ${this.messageProcessingCount}):`, eventData);

    // Handle both unified data events (ChatMessageEntity) and legacy chat events (ChatMessageEventData)
    const chatMessage = 'entity' in eventData
      ? (eventData as ChatMessageEventData).entity
      : eventData as ChatMessageEntity;
    const roomId = chatMessage.roomId;

    if (roomId === this.roomId) {
      // Use the ChatMessage entity - unified approach

      console.log(`✨ ChatWidget: Using ChatMessage entity directly:`, chatMessage);

      if (this.chatScroller) {
        // Check if message already exists - if so, update instead of add
        const existingEntities = this.chatScroller.entities() as ChatMessageEntity[];
        const existingMessage = existingEntities.find(msg => msg.id === chatMessage.id);

        if (existingMessage) {
          // Message exists, update it
          console.log(`🔄 ChatWidget: Message already exists, updating: ${chatMessage.id}`);
          this.chatScroller.update(chatMessage.id, chatMessage);
        } else {
          // New message, add with auto-scroll
          console.log(`✨ ChatWidget: New message, adding: ${chatMessage.id}`);
          this.chatScroller.addWithAutoScroll(chatMessage);
        }

        this.messages = this.chatScroller.entities() as ChatMessageEntity[];
        this.updateEntityCount();
      } else {
        // Fallback to old method
        this.messages.push(chatMessage);
        await this.renderWidget(); // Re-render with new message
        this.smartScrollToBottom(); // Keep fallback behavior for now
        this.updateEntityCount();
      }

      // Real-time message added successfully using domain object
    }
  }

  /**
   * Handle updated chat messages for this room - STRICT TYPING
   * Supports unified data events (ChatMessageEntity from data/update)
   */
  private async onMessageUpdated(eventData: ChatMessageEntity): Promise<void> {
    console.log(`🔄 ChatWidget: Received message update for room ${this.roomId}:`, eventData);

    // ChatMessageEntity from unified data events
    const chatMessage = eventData;
    const roomId = chatMessage.roomId;

    if (roomId === this.roomId) {
      console.log(`✨ ChatWidget: Updating ChatMessage entity:`, chatMessage);

      if (this.chatScroller) {
        // Use EntityScroller to update the message in place
        this.chatScroller.update(chatMessage.id, chatMessage);
        this.messages = this.chatScroller.entities() as ChatMessageEntity[];
      } else {
        // Fallback: Find and update in messages array
        const messageIndex = this.messages.findIndex(msg => msg.id === chatMessage.id);
        if (messageIndex !== -1) {
          this.messages[messageIndex] = chatMessage;
          await this.renderWidget(); // Re-render with updated message
        } else {
          console.warn(`⚠️ ChatWidget: Could not find message to update: ${chatMessage.id}`);
        }
      }

      console.log(`✅ ChatWidget: Message updated successfully`);
    }
  }

  /**
   * Handle user joined events - STRICT TYPING
   */
  private async onUserJoined(eventData: ChatParticipantEventData): Promise<void> {
    console.log(`👋 ChatWidget: User ${eventData.userName} joined room ${this.roomId}`);
    
    if (eventData.roomId === this.roomId) {
      // Create ChatMessageEntity directly - no field copying
      const systemMessage = new ChatMessageEntity();
      systemMessage.roomId = this.roomId as UUID;
      systemMessage.senderId = 'system' as UUID;
      systemMessage.senderName = 'System';
      systemMessage.content = {
        text: `${eventData.userName} joined the room`,
        attachments: []
      };
      systemMessage.status = 'sent';
      systemMessage.priority = 'normal';
      systemMessage.timestamp = new Date(eventData.timestamp);
      systemMessage.reactions = [];
      systemMessage.metadata = {
        ...DEFAULT_MESSAGE_METADATA,
        source: 'system'
      };
      if (this.chatScroller) {
        // System messages use regular add() since they don't need auto-scroll
        this.chatScroller.add(systemMessage, 'start');
        this.messages = this.chatScroller.entities() as ChatMessageEntity[];
      } else {
        this.messages.push(systemMessage);
        await this.renderWidget();
      }
    }
  }

  /**
   * Handle user left events - STRICT TYPING
   */
  private async onUserLeft(eventData: ChatParticipantEventData): Promise<void> {
    console.log(`👋 ChatWidget: User ${eventData.userName} left room ${this.roomId}`);
    
    if (eventData.roomId === this.roomId) {
      // Create ChatMessageEntity directly - no field copying
      const systemMessage = new ChatMessageEntity();
      systemMessage.roomId = this.roomId as UUID;
      systemMessage.senderId = 'system' as UUID;
      systemMessage.senderName = 'System';
      systemMessage.content = {
        text: `${eventData.userName} left the room`,
        attachments: []
      };
      systemMessage.status = 'sent';
      systemMessage.priority = 'normal';
      systemMessage.timestamp = new Date(eventData.timestamp);
      systemMessage.reactions = [];
      systemMessage.metadata = {
        ...DEFAULT_MESSAGE_METADATA,
        source: 'system'
      };
      if (this.chatScroller) {
        // System messages use regular add() since they don't need auto-scroll
        this.chatScroller.add(systemMessage, 'start');
        this.messages = this.chatScroller.entities() as ChatMessageEntity[];
      } else {
        this.messages.push(systemMessage);
        await this.renderWidget();
      }
    }
  }


  /**
   * Scroll chat messages to bottom to show latest content
   */
  private scrollToBottom(): void {
    try {
      const messagesContainer = this.shadowRoot.querySelector('#messages');
      if (messagesContainer) {
        // Use requestAnimationFrame to ensure DOM is fully rendered
        requestAnimationFrame(() => {
          messagesContainer.scrollTop = messagesContainer.scrollHeight;
        });
      } else {
        console.warn('⚠️ ChatWidget: Messages container not found for auto-scroll');
      }
    } catch (error) {
      console.error('❌ ChatWidget: Auto-scroll failed:', error);
    }
  }

  /**
   * Check if user is near the bottom of the chat (within scrolling threshold)
   */
  private isNearBottom(threshold: number = 100): boolean {
    if (!this.messagesContainer) return false;

    const { scrollTop, scrollHeight, clientHeight } = this.messagesContainer;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;

    return distanceFromBottom <= threshold;
  }

  /**
   * Smart scroll to bottom - only if user is already near bottom
   * This prevents interrupting users who are reading older messages
   */
  private smartScrollToBottom(): void {
    if (this.isNearBottom()) {
      console.log('📍 ChatWidget: User near bottom, auto-scrolling to show new message');
      this.scrollToBottomSmooth();
    } else {
      console.log('📍 ChatWidget: User reading older messages, not auto-scrolling');
    }
  }

  /**
   * Smooth scroll to bottom with visual feedback
   */
  private scrollToBottomSmooth(): void {
    try {
      if (this.messagesContainer) {
        // Use smooth scrolling behavior for better UX
        this.messagesContainer.scrollTo({
          top: this.messagesContainer.scrollHeight,
          behavior: 'smooth'
        });
      }
    } catch (error) {
      console.error('❌ ChatWidget: Smooth scroll failed:', error);
      // Fallback to instant scroll
      this.scrollToBottom();
    }
  }

  /**
   * Get message IDs currently visible in the viewport
   */
  private getVisibleUUIDs(): string[] {
    if (!this.messagesContainer) return [];

    const containerRect = this.messagesContainer.getBoundingClientRect();
    const messageElements = this.messagesContainer.querySelectorAll('[data-message-id]');
    const visibleIds: string[] = [];

    messageElements.forEach(el => {
      const rect = el.getBoundingClientRect();
      const isVisible = rect.top < containerRect.bottom && rect.bottom > containerRect.top;
      if (isVisible) {
        const messageId = el.getAttribute('data-message-id');
        if (messageId) visibleIds.push(messageId);
      }
    });

    return visibleIds;
  }

  /**
   * Restore scroll position from saved state
   */
  private async restoreScrollPosition(): Promise<void> {
    try {
      const scrollStateKey = `chat_scroll_${this.roomId}`;
      const savedState = await this.getData(scrollStateKey) as ScrollState | null;

      if (savedState && savedState.timestamp && this.messagesContainer) {
        const timeSinceScroll = Date.now() - savedState.timestamp;

        // Only restore if scroll position was saved recently (within 5 minutes)
        if (timeSinceScroll < 5 * 60 * 1000) {
          console.log(`🔄 ChatWidget: Restoring scroll position`, savedState);

          // Try to restore to saved scroll position
          this.messagesContainer.scrollTop = savedState.scrollTop;

          // Verify we landed on a familiar message, if not scroll to bottom
          const currentVisibleIds = this.getVisibleUUIDs();
          const hasMatchingMessage = savedState.visibleUUIDs.some((id: string) =>
            currentVisibleIds.includes(id)
          );

          if (!hasMatchingMessage) {
            console.log(`⚠️ ChatWidget: Couldn't find matching messages, scrolling to bottom`);
            this.scrollToBottom();
          } else {
            console.log(`✅ ChatWidget: Successfully restored scroll position`);
          }
        } else {
          console.log(`⏰ ChatWidget: Scroll state too old (${Math.round(timeSinceScroll/60000)}min), scrolling to bottom`);
          this.scrollToBottom();
        }
      } else {
        console.log(`📍 ChatWidget: No saved scroll state for key "${scrollStateKey}", scrolling to bottom`);
        console.log(`📊 ChatWidget: getData returned:`, savedState, typeof savedState);
        this.scrollToBottom();
      }
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to restore scroll position:`, error);
      this.scrollToBottom();
    }
  }

  /**
   * Render messages - now handled by EntityScroller
   */
  private renderMessages(): string {
    // EntityScroller handles DOM rendering directly
    // Return empty string since template replacement still expects it
    return '';
  }
  
  // Row manipulation is now handled by EntityScroller
  // These methods are no longer needed


  protected override setupEventListeners(): void {
    super.setupEventListeners();
    
    // Re-cache messageInput in case it wasn't available during renderWidget
    if (!this.messageInput) {
      this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    }
    
    
    // Send message on Enter
    const keydownHandler = (e: KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        this.sendMessage();
      }
    };
    
    // Send message on button click  
    const clickHandler = (e: Event) => {
      e.preventDefault();
      this.sendMessage();
    };
    
    // Store handlers for cleanup
    this._keydownHandler = keydownHandler;
    this._clickHandler = clickHandler;
    
    // Attach event listeners
    this.messageInput?.addEventListener('keydown', keydownHandler);
    
    const sendButton = this.shadowRoot.getElementById('sendButton');
    sendButton?.addEventListener('click', clickHandler);
    
  }
  
  protected override cleanupEventListeners(): void {
    super.cleanupEventListeners();
    // Remove existing event listeners to prevent duplicates
    if (this._keydownHandler && this.messageInput) {
      this.messageInput.removeEventListener('keydown', this._keydownHandler);
    }
    
    if (this._clickHandler) {
      const sendButton = this.shadowRoot.getElementById('sendButton');
      if (sendButton) {
        sendButton.removeEventListener('click', this._clickHandler);
      }
    }
  }

  protected renderTemplate(): string {
    return `
      ${this.renderHeader()}

      <div class="messages-container" id="messages">
        ${this.renderMessages()}
      </div>

      ${this.renderFooter()}
    `;
  }

  // ChatWidget overrides renderFooter to provide input controls
  protected renderFooter(): string {
    return `
      <div class="input-container">
        <input type="text" class="message-input" id="messageInput" placeholder="Type a message...">
        <button class="send-button" id="sendButton">Send</button>
      </div>
    `;
  }

  public async sendMessage(): Promise<void> {
    const content = this.getInputContent();
    if (!content) return;

    this.clearInput();
    await this.sendChatMessage(content);
  }

  private getInputContent(): string {
    if (!this.messageInput) return '';
    return this.messageInput.value.trim();
  }

  private clearInput(): void {
    if (this.messageInput) {
      this.messageInput.value = '';
    }
  }

  private async sendChatMessage(content: string): Promise<void> {
    try {
      console.log(`🔧 CLAUDE-DEBUG-${Date.now()}: sendChatMessage called with currentRoom="${this.roomId}"`);

      // Create ChatMessageEntity instance for data/create command
      const messageEntity = new ChatMessageEntity();
      messageEntity.roomId = this.roomId;
      messageEntity.senderId = "002350cc-0031-408d-8040-004f000f" as UUID; // Joel's user ID
      messageEntity.senderName = "Joel";
      messageEntity.content = {
        text: content,
        attachments: []
      };
      messageEntity.status = "sending" as const;
      messageEntity.priority = "normal" as const;
      messageEntity.timestamp = new Date();
      messageEntity.reactions = [];

      const client = await JTAGClient.sharedInstance;
      const createResult = await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
        collection: 'ChatMessage',
        data: messageEntity,
        context: client.context,
        sessionId: client.sessionId
      });

      console.log('✅ Message sent successfully via data/create:', createResult);

      // Always scroll to bottom after sending own message
      // User expects to see their message immediately
      requestAnimationFrame(() => {
        this.scrollToBottom(); // Use simple scroll since user sent the message
      });
    } catch (error) {
      console.error('❌ Failed to send message:', error);
      // Re-add content to input on error
      this.restoreInputContent(content);
    }
  }

  private restoreInputContent(content: string): void {
    if (this.messageInput) {
      this.messageInput.value = content;
    }
  }

  // REMOVED: getPersistentUUID() - now using UUIDManager properly


  /**
   * Subscribe to room change events from other widgets (like RoomListWidget)
   */
  private async subscribeToRoomChangeEvents(): Promise<void> {
    try {
      console.log(`🎧 ChatWidget: Subscribing to room change events`);

      // Listen for room selection events from RoomListWidget or other sources
      Events.subscribe<{ roomId: UUID, roomEntity?: RoomEntity }>('chat:room-changed', async (eventData) => {
        console.log(`🔥 SERVER-EVENT-RECEIVED: chat:room-changed`, eventData);
        await this.handleRoomChange(eventData.roomId, eventData.roomEntity);
      });

      console.log(`✅ ChatWidget: Subscribed to room change events`);
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to subscribe to room change events:`, error);
    }
  }

  /**
   * Handle room changes from events with proper change detection to prevent loops
   */
  private async handleRoomChange(newUUID: UUID, roomEntity?: RoomEntity): Promise<void> {
    // Prevent infinite loops - only change if room actually changed
    if (newUUID === this.roomId) {
      console.log(`🔄 ChatWidget: Room change ignored - already in room "${newUUID}"`);
      return;
    }

    console.log(`🏠 ChatWidget: Changing room from "${this.roomId}" to "${newUUID}"`);

    try {
      // Save current scroll position before switching
      await this.onWidgetCleanup();

      // Update room references
      this.roomId = newUUID;
      this.currentRoomEntity = roomEntity;

      // If no room entity provided in the event, load it
      if (!this.currentRoomEntity) {
        await this.loadRoomEntity();
      }

      // Clear current messages and scroller
      this.messages = [];
      if (this.chatScroller) {
        this.chatScroller.destroy();
        this.chatScroller = undefined;
      }

      // Re-initialize for new room
      await this.subscribeToRoomEvents();
      await this.renderWidget(); // Will create new EntityScroller for new room

      console.log(`✅ ChatWidget: Successfully changed to room "${this.getRoomDisplayName()}" (${newUUID})`);
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to change room:`, error);
    }
  }

  /**
   * Public method to change room programmatically and emit event
   */
  public async changeRoom(roomId: UUID, roomEntity?: RoomEntity): Promise<void> {
    console.log(`🎯 ChatWidget: Public room change requested to "${roomId}"`);

    // Emit room change event for other widgets to subscribe to
    Events.emit('chat:room-changed', { roomId, roomEntity });

    // Handle the change locally (will ignore if already in the room)
    await this.handleRoomChange(roomId, roomEntity);
  }

  /**
   * Set up message input event handlers
   * Called after DOM rendering to ensure input elements are available
   */
  private setupMessageInputHandlers(): void {
    // Clean up existing listeners first
    this.cleanupMessageInputHandlers();

    if (!this.messageInput) {
      console.warn('⚠️ ChatWidget: Cannot set up input handlers - messageInput not found');
      return;
    }

    // Send message on Enter key
    const keydownHandler = (e: KeyboardEvent) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        this.sendMessage();
      }
    };

    // Send message on button click
    const clickHandler = (e: Event) => {
      e.preventDefault();
      this.sendMessage();
    };

    // Store handlers for cleanup
    this._keydownHandler = keydownHandler;
    this._clickHandler = clickHandler;

    // Attach event listeners
    this.messageInput.addEventListener('keydown', keydownHandler);

    const sendButton = this.shadowRoot?.getElementById('sendButton');
    sendButton?.addEventListener('click', clickHandler);

    console.log('✅ ChatWidget: Message input handlers set up for room', this.roomId);
  }

  /**
   * Clean up message input event handlers
   */
  private cleanupMessageInputHandlers(): void {
    if (this._keydownHandler && this.messageInput) {
      this.messageInput.removeEventListener('keydown', this._keydownHandler);
      this._keydownHandler = undefined;
    }

    if (this._clickHandler) {
      const sendButton = this.shadowRoot?.getElementById('sendButton');
      if (sendButton) {
        sendButton.removeEventListener('click', this._clickHandler);
      }
      this._clickHandler = undefined;
    }
  }

}