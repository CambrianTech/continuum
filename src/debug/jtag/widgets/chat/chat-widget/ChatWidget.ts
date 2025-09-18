/**
 * Chat Widget - JTAG-based Web Component using BaseWidget Architecture
 * 
 * Updated to use the new BaseWidget abstraction with one-line operations.
 * Demonstrates dramatic code simplification through proper abstraction.
 */

import { ChatMessageData, type MessageReaction } from '../../../system/data/domains/ChatMessage';
import { DEFAULT_MESSAGE_METADATA, DEFAULT_MESSAGE_FORMATTING } from '../../../system/data/domains/ChatMessage';
import { MessageId, RoomId, UserId, ISOString } from '../../../system/data/domains/CoreTypes';
import type { SessionId } from '../../../system/data/domains/CoreTypes';
import type { ChatSendMessageParams, ChatSendMessageResult } from '../../../commands/chat/send-message/shared/ChatSendMessageTypes';
import { MessageRowWidgetFactory } from '../shared/BaseMessageRowWidget';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { GetMessagesParams, GetMessagesResult } from '../../../commands/chat/get-messages/shared/GetMessagesTypes';
import type { SubscribeRoomParams, SubscribeRoomResult } from '../../../commands/chat/subscribe-room/shared/SubscribeRoomCommand';
import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { CHAT_EVENTS, CHAT_EVENT_TYPES } from '../shared/ChatEventConstants';
import type {
  ChatMessageEventData,
  ChatParticipantEventData,
  ChatEventName
} from '../shared/ChatEventTypes';
import { userIdManager } from '../../../system/shared/UserIdManager';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import { InfiniteScrollHelper, type CursorPaginationState } from '../shared/InfiniteScrollHelper';

export class ChatWidget extends ChatWidgetBase {
  private messages: ChatMessageData[] = [];
  private currentRoom: string;
  private messageInput?: HTMLInputElement;
  private eventSubscriptionId?: string;
  private scrollHelper?: InfiniteScrollHelper;
  private messagesContainer?: HTMLElement;

  // Event handler references for proper cleanup
  private _keydownHandler?: (e: KeyboardEvent) => void;
  private _clickHandler?: (e: Event) => void;

  // PUBLIC properties required by integration tests (NOT optional - must be set)
  public currentUserId!: UserId; // Persistent User ID for "me" attribution
  public currentSessionId!: SessionId; // Current browser session ID
  
  constructor(roomId: string = 'general') {
    super({
      widgetName: 'ChatWidget',
      template: 'chat-widget.html',
      styles: 'chat-widget.css',
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true
    });
    this.currentRoom = roomId;
  }

  // Static property required by widget registration system
  static get widgetName(): string {
    return 'chat';
  }



   protected override resolveResourcePath(filename: string): string {
      // Extract widget directory name from widget name (ChatWidget -> chat)
      //const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
      // Return relative path from current working directory
      return `widgets/chat/chat-widget/${filename}`;
    }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🔧 CLAUDE-FIX-' + Date.now() + ': ChatWidget now uses chat/get-messages instead of data/list');
    console.log(`🎯 ChatWidget: Initializing for room "${this.currentRoom}"...`);

    // CRITICAL: Initialize persistent User ID using UserIdManager (REQUIRED)
    const userIdString = await userIdManager.getCurrentUserId();
    this.currentUserId = userIdString as UserId; // Cast to branded type
    console.log(`🔧 CLAUDE-USER-ID-DEBUG: Initialized persistent User ID: ${this.currentUserId}`);

    // Set current session ID from JTAG system context (REQUIRED)
    const client = await JTAGClient.sharedInstance;
    if (!client?.sessionId) {
      throw new Error('ChatWidget requires session context - cannot initialize without sessionId');
    }
    this.currentSessionId = client.sessionId as SessionId;
    console.log(`🔧 CLAUDE-SESSION-ID-DEBUG: Current session ID: ${this.currentSessionId}`);
    
    // Load room message history using command abstraction
    await this.loadRoomHistory();
    
    // Subscribe to room-specific events
    await this.subscribeToRoomEvents();
    
    console.log(`✅ ChatWidget: Initialized for room "${this.currentRoom}" with ${this.messages.length} messages`);
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Cleanup infinite scroll helper
    if (this.scrollHelper) {
      this.scrollHelper.cleanup();
      this.scrollHelper = undefined;
    }

    // Unsubscribe from room events using JTAG abstraction
    //TODO: completely missing command for unsubscribe. should this just be subscribe-room with a flag?
    // if (this.eventSubscriptionId) {
    //   try {
    //     await this.executeCommand<ChatUnsubscribeEventResult>('events/unsubscribe', {
    //       subscriptionId: this.eventSubscriptionId
    //     });
    //     console.log(`🔌 ChatWidget: Unsubscribed from room ${this.currentRoom} events`);
    //   } catch (error) {
    //     console.error(`❌ ChatWidget: Failed to unsubscribe from events:`, error);
    //   }
    // }

    // Save room-specific messages using BaseWidget abstraction
    const roomMessageKey = `chat_messages_${this.currentRoom}`;
    await this.storeData(roomMessageKey, this.messages, { persistent: true });
    console.log(`✅ ChatWidget: Cleanup complete for room "${this.currentRoom}"`);
  }

  protected override async renderWidget(): Promise<void> {
    super.renderWidget();

    // Cache input element and messages container
    this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    this.messagesContainer = this.shadowRoot.getElementById('messagesContainer') as HTMLElement;

    // Initialize infinite scroll helper
    if (this.messagesContainer && !this.scrollHelper) {
      this.scrollHelper = new InfiniteScrollHelper({
        pageSize: 20, // Load 20 messages at a time
        threshold: 0.1 // Trigger when 10% from top
      });

      // Set up intersection observer for loading older messages
      this.scrollHelper.setupIntersectionObserver(
        this.messagesContainer,
        (cursor: string) => this.loadOlderMessages(cursor)
      );
    }

    // Auto-scroll to bottom to show latest messages
    this.scrollToBottom();
  }


  /**
   * Load initial room message history with cursor-based pagination
   */
  private async loadRoomHistory(): Promise<void> {
    try {
      console.log(`📚 ChatWidget: Loading room history using cursor-based pagination`);

      const client = await JTAGClient.sharedInstance;

      // Load initial batch of recent messages (no cursor = most recent)
      const historyResult = await this.executeCommand<GetMessagesParams, GetMessagesResult>('chat/get-messages', {
        context: client.context,
        sessionId: client.sessionId,
        roomId: this.currentRoom,
        limit: 20 // Initial page size
      });

      console.log(`🔧 CLAUDE-WIDGET-DEBUG-${Date.now()}: Initial load result:`, historyResult);

      if (historyResult?.success && historyResult?.messages) {
        this.messages = historyResult.messages
          .filter((message): message is ChatMessageData => !!message.content?.text && message.content.text.trim().length > 0);

        // Initialize scroll helper state with loaded messages
        if (this.scrollHelper) {
          this.scrollHelper.initializeWithMessages(this.messages);
        }

        console.log(`✅ ChatWidget: Loaded ${this.messages.length} initial messages for room "${this.currentRoom}"`);
      } else if (historyResult?.success === false) {
        console.error(`❌ ChatWidget: Failed to get messages: ${historyResult.error}`);
        this.messages = [];
      } else {
        console.log(`ℹ️ ChatWidget: No messages found for room "${this.currentRoom}"`);
        this.messages = [];
      }

    } catch (error) {
      console.error(`❌ ChatWidget: Failed to load room history:`, error);
      throw error;
    }
  }

  /**
   * Load older messages when user scrolls to top
   */
  private async loadOlderMessages(cursor: string): Promise<ChatMessageData[]> {
    try {
      console.log(`📚 ChatWidget: Loading older messages with cursor: ${cursor}`);

      const client = await JTAGClient.sharedInstance;

      // Use data/list command with cursor for older messages
      const olderResult = await this.executeCommand<DataListParams, DataListResult<ChatMessageData>>('data/list', {
        context: client.context,
        sessionId: client.sessionId,
        collection: 'chat_messages',
        filter: { roomId: this.currentRoom },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 20,
        cursor: {
          field: 'timestamp',
          value: cursor,
          direction: 'before'
        }
      });

      if (olderResult?.success && olderResult?.items) {
        const olderMessages = olderResult.items
          .filter((message): message is ChatMessageData => !!message.content?.text && message.content.text.trim().length > 0);

        // Insert older messages at the beginning
        this.messages = [...olderMessages, ...this.messages];

        // Re-render messages to include older ones
        this.renderWidget();

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
      console.log(`🔗 ChatWidget: Subscribing to room events for "${this.currentRoom}"`);
      
      // Try JTAG operation to subscribe to room events via the chat daemon
      try {
        const client = await JTAGClient.sharedInstance;
        const subscribeResult = await this.executeCommand<SubscribeRoomParams, SubscribeRoomResult>('chat/subscribe-room', {
          context: client.context,
          sessionId: client.sessionId,
          roomId: this.currentRoom,
          eventTypes: [CHAT_EVENTS.MESSAGE_RECEIVED, CHAT_EVENTS.PARTICIPANT_JOINED, CHAT_EVENTS.PARTICIPANT_LEFT]
        });
        
        if (subscribeResult && subscribeResult.success && subscribeResult.subscriptionId) {
          this.eventSubscriptionId = subscribeResult.subscriptionId;
          console.log(`✅ ChatWidget: Subscribed to room "${this.currentRoom}" events via JTAG`);
        }
      } catch (jtagError) {
        console.error(`ℹ️ ChatWidget: JTAG room subscription not available, using DOM events fallback`, jtagError);
      }
      
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
    console.log(`📡 ChatWidget: Setting up type-safe room event handlers for room ${this.currentRoom}`);
    
    // Type-safe event listeners using BaseWidget's executeEvent system
    // These ONLY respond to genuine server-originated events
    this.addWidgetEventListener(CHAT_EVENTS.MESSAGE_RECEIVED, (eventData: ChatMessageEventData) => {
      console.log(`🔥 SERVER-EVENT-RECEIVED: ${CHAT_EVENTS.MESSAGE_RECEIVED}`, eventData);
      this.onMessageReceived(eventData);
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
    console.log(`📨 ChatWidget: Received room event "${eventType}" for room ${this.currentRoom}`);
    
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


  /**
   * Handle incoming chat messages for this room - STRICT TYPING
   */
  private async onMessageReceived(eventData: ChatMessageEventData): Promise<void> {
    console.log(`📨 ChatWidget: Received message for room ${this.currentRoom}:`, eventData);
    console.log(`🔧 CLAUDE-EVENT-DATA-${Date.now()}: Raw event data structure:`, JSON.stringify(eventData, null, 2));

    if (eventData.roomId === this.currentRoom) {
      // Use the ChatMessage domain object directly - no reconstruction needed!
      const chatMessage = eventData.message;

      console.log(`✨ ChatWidget: Using ChatMessage domain object directly:`, chatMessage);

      this.messages.push(chatMessage);
      await this.renderWidget(); // Re-render with new message

      // Real-time message added successfully using domain object
    }
  }

  /**
   * Handle user joined events - STRICT TYPING
   */
  private async onUserJoined(eventData: ChatParticipantEventData): Promise<void> {
    console.log(`👋 ChatWidget: User ${eventData.userName} joined room ${this.currentRoom}`);
    
    if (eventData.roomId === this.currentRoom) {
      // Add system message for user join using ChatMessageData interface
      const systemMessage: ChatMessageData = {
        messageId: MessageId(`system_${Date.now()}`),
        roomId: RoomId(this.currentRoom),
        senderId: UserId('system'),
        senderName: 'System',
        content: {
          text: `${eventData.userName} joined the room`,
          attachments: [],
          formatting: DEFAULT_MESSAGE_FORMATTING
        },
        status: 'sent',
        priority: 'normal',
        timestamp: ISOString(eventData.timestamp),
        reactions: [],
        metadata: {
          ...DEFAULT_MESSAGE_METADATA,
          source: 'system'
        },
        id: `system_${Date.now()}`,
        createdAt: ISOString(eventData.timestamp),
        updatedAt: ISOString(eventData.timestamp),
        version: 1
      };
      
      this.messages.push(systemMessage);
      await this.renderWidget();
    }
  }

  /**
   * Handle user left events - STRICT TYPING
   */
  private async onUserLeft(eventData: ChatParticipantEventData): Promise<void> {
    console.log(`👋 ChatWidget: User ${eventData.userName} left room ${this.currentRoom}`);
    
    if (eventData.roomId === this.currentRoom) {
      // Add system message for user leave using ChatMessageData interface
      const systemMessage: ChatMessageData = {
        messageId: MessageId(`system_${Date.now()}`),
        roomId: RoomId(this.currentRoom),
        senderId: UserId('system'),
        senderName: 'System',
        content: {
          text: `${eventData.userName} left the room`,
          attachments: [],
          formatting: DEFAULT_MESSAGE_FORMATTING
        },
        status: 'sent',
        priority: 'normal',
        timestamp: ISOString(eventData.timestamp),
        reactions: [],
        metadata: {
          ...DEFAULT_MESSAGE_METADATA,
          source: 'system'  // Proper system message classification
        },
        id: `system_${Date.now()}`,  // For BaseEntity compatibility
        createdAt: ISOString(eventData.timestamp),
        updatedAt: ISOString(eventData.timestamp),
        version: 1
      };
      
      this.messages.push(systemMessage);
      await this.renderWidget();
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
          console.log('🔧 CLAUDE-SCROLL-DEBUG: Scrolled to bottom');
        });
      } else {
        console.warn('⚠️ ChatWidget: Messages container not found for auto-scroll');
      }
    } catch (error) {
      console.error('❌ ChatWidget: Auto-scroll failed:', error);
    }
  }

  private renderMessages(): string {
    return this.messages.map(msg => {
      // Use enhanced modular renderer with options
      const renderer = MessageRowWidgetFactory.createRenderer(msg, {
        enableIntersectionObserver: true,
        lazyLoadImages: true,
        enableInteractions: true,
        customClassNames: ['chat-message-renderer']
      });
      // FIXED: Pass currentUserId for proper senderId-based positioning
      return renderer.renderMessageContainer(msg, this.currentUserId);
    }).join('');
  }
  
  /**
   * Efficiently append a single message row without re-rendering entire widget
   */
  private appendMessageRow(message: ChatMessageData): void {
    const messagesContainer = this.shadowRoot.querySelector('#messages');
    if (!messagesContainer) {
      console.warn('⚠️ ChatWidget: No messages container found for row append');
      return;
    }
    
    // Render just this one message row
    const renderer = MessageRowWidgetFactory.createRenderer(message, {
      enableIntersectionObserver: true,
      lazyLoadImages: true,
      enableInteractions: true,
      customClassNames: ['chat-message-renderer']
    });
    
    const messageRowHTML = renderer.renderMessageContainer(message, this.currentUserId);
    
    // Create temporary element and append to container
    const tempDiv = document.createElement('div');
    tempDiv.innerHTML = messageRowHTML;
    
    // Append all children from temp div to messages container
    while (tempDiv.firstChild) {
      messagesContainer.appendChild(tempDiv.firstChild);
    }
    
    // Auto-scroll to show new message
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
  }

  protected override setupEventListeners(): void {
    super.setupEventListeners();
    
    // Re-cache messageInput in case it wasn't available during renderWidget
    if (!this.messageInput) {
      this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    }
    
    console.log('🔧 CLAUDE-DEBUG: setupEventListeners called');
    console.log('🔧 CLAUDE-DEBUG: messageInput exists:', !!this.messageInput);
    console.log('🔧 CLAUDE-DEBUG: messageInput element:', this.messageInput);
    console.log('🔧 CLAUDE-DEBUG: sendButton exists:', !!this.shadowRoot.getElementById('sendButton'));
    
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
      console.log('🔧 CLAUDE-DEBUG: send button clicked');
      this.sendMessage();
    };
    
    // Store handlers for cleanup
    this._keydownHandler = keydownHandler;
    this._clickHandler = clickHandler;
    
    // Attach event listeners
    this.messageInput?.addEventListener('keydown', keydownHandler);
    
    const sendButton = this.shadowRoot.getElementById('sendButton');
    sendButton?.addEventListener('click', clickHandler);
    
    console.log('🔧 CLAUDE-DEBUG: event listeners attached with proper cleanup');
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

  protected override getReplacements(): Record<string, string> {
      return {
          '<!-- ROOM_NAME -->': this.getRoomDisplayName(),
          '<!-- Dynamic messages rendered here -->': this.renderMessages()
      };
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
      const client = await JTAGClient.sharedInstance;
      const sendResult = await this.executeCommand<ChatSendMessageParams, ChatSendMessageResult>('chat/send-message', {
        context: client.context,
        sessionId: client.sessionId,
        content: content,
        roomId: this.currentRoom,
        senderType: 'user'
      });

      console.log('✅ Message sent successfully:', sendResult);
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

  // REMOVED: getPersistentUserId() - now using UserIdManager properly

  private getRoomDisplayName(): string {
    // Capitalize the room name for display
    return this.currentRoom.charAt(0).toUpperCase() + this.currentRoom.slice(1);
  }
}