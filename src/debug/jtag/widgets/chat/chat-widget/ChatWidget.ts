/**
 * Chat Widget - JTAG-based Web Component using BaseWidget Architecture
 * 
 * Updated to use the new BaseWidget abstraction with one-line operations.
 * Demonstrates dramatic code simplification through proper abstraction.
 */

import type { ChatMessage } from '../shared/ChatModuleTypes';
import type { ChatSendMessageResult } from '../../../commands/chat/send-message/shared/ChatSendMessageTypes';
import { MessageRowWidgetFactory } from '../shared/BaseMessageRowWidget';
//import type { User } from '../../../domain/user/User';
import type { DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { SubscribeRoomResult } from '../../../commands/chat/subscribe-room/shared/SubscribeRoomCommand';
import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { CHAT_EVENTS, CHAT_EVENT_TYPES } from '../shared/ChatEventConstants';
import type { 
  ChatMessageEventData, 
  ChatParticipantEventData,
  ChatEventName
} from '../shared/ChatEventTypes';

export class ChatWidget extends ChatWidgetBase {
  private messages: ChatMessage[] = [];
  private currentRoom: string;
  private messageInput?: HTMLInputElement;
  private eventSubscriptionId?: string;
  
  // Event handler references for proper cleanup
  private _keydownHandler?: (e: KeyboardEvent) => void;
  private _clickHandler?: (e: Event) => void;

  private currentUserId?: string; // Persistent User ID for "me" attribution
  
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
    console.log(`🎯 ChatWidget: Initializing for room "${this.currentRoom}"...`);
    
    // CRITICAL: Initialize persistent User ID using simple localStorage approach
    this.currentUserId = this.getPersistentUserId();
    console.log(`🔧 CLAUDE-USER-ID-DEBUG: Initialized persistent User ID: ${this.currentUserId}`);
    
    // Load room message history using command abstraction
    await this.loadRoomHistory();
    
    // Subscribe to room-specific events
    await this.subscribeToRoomEvents();
    
    console.log(`✅ ChatWidget: Initialized for room "${this.currentRoom}" with ${this.messages.length} messages`);
  }

  protected async onWidgetCleanup(): Promise<void> {
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
    
    // Cache input element
    this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
        
    // Auto-scroll to bottom to show latest messages
    this.scrollToBottom();
  }


  /**
   * Load room message history using data/list command
   */
  private async loadRoomHistory(): Promise<void> {
    try {
      console.log(`📚 ChatWidget: Loading room history using data/list command`);
      
      // Use data/list command to get all chat messages, then filter by room
      const historyResult = await this.executeCommand<DataListResult<ChatMessage>>('data/list', {
        collection: 'chat_messages',
        limit: 2000, // Recent messages
        roomId: this.currentRoom, // ← Ideally filter on server side, but our server is dumb right now
        orderBy: [{ field: 'timestamp', direction: 'asc' }] // Order by timestamp ascending
      });
      
      console.log("JOEL History result:", historyResult.items);

      if (historyResult?.items) {
        // Filter messages for current room and convert to internal format
        this.messages = historyResult.items
          .map((item) => {
            // TODO: type isnt used as current user you are ridiculous claude. the style should be applied by the user id match but has nothing to do with type
            const senderId = item.senderId;
            const isCurrentUser = this.currentUserId && senderId === this.currentUserId;
            return { ...item, type: isCurrentUser ?  'user' : item.type };
          })
          .filter((item: ChatMessage) => item.content && item.content.trim().length > 0) 

        console.log(`✅ ChatWidget: Loaded ${this.messages.length} messages for room "${this.currentRoom}" from data/list`);
      } else {
        console.log(`ℹ️ ChatWidget: No messages found for room "${this.currentRoom}"`);
        this.messages = [];
      }
      
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to load room history:`, error);
      throw error; // No fallbacks - crash and burn is better
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
        const subscribeResult = await this.executeCommand<SubscribeRoomResult>('chat/subscribe-room', {
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
    
    // DEPRECATED: DOM event listeners - mark for removal
    console.warn("⚠️ DEPRECATED: Setting up DOM event fallbacks - these will be removed once server events are working");
    this.setupDOMEventFallbacks();
    
    console.log(`✅ ChatWidget: Set up type-safe event listeners for chat events`);
  }

  /**
   * DEPRECATED: DOM event fallbacks - only for backward compatibility
   * TODO: Remove this once server-side event emission is working properly
   */
  private setupDOMEventFallbacks(): void {
    console.warn("⚠️ DEPRECATED: setupDOMEventFallbacks() - Remove after fixing server events");
    
    // Keep existing DOM listeners as fallback until server events work
    document.addEventListener(CHAT_EVENTS.MESSAGE_RECEIVED, (event: Event) => {
      console.warn("⚠️ DEPRECATED: Using DOM event fallback for MESSAGE_RECEIVED");
      const customEvent = event as CustomEvent;
      this.handleDOMChatEvent(customEvent);
    });
    
    document.addEventListener('chat-message-sent', (event: Event) => {
      console.warn("⚠️ DEPRECATED: Using legacy chat-message-sent DOM event");
      const customEvent = event as CustomEvent;
      this.handleDOMChatEvent(customEvent);
    });
  }
  
  /**
   * Handle DOM chat events from EventsDaemon - React-like efficient rendering
   */
  private async handleDOMChatEvent(event: CustomEvent): Promise<void> {
    console.log(`🔧 CLAUDE-DOM-HANDLER: Processing DOM chat event`, event.detail);
    
    // Check if event has message data we can use directly (React-like state update)
    if (event.detail.message) {
      const eventMessage = event.detail.message;
      
      // Only add if it's for our room and not already in our messages
      if (eventMessage.roomId === this.currentRoom) {
        const messageExists = this.messages.some(msg => msg.id === eventMessage.messageId);
        
        if (!messageExists) {
          // Add new message directly (React-like state update)
          const newMessage: ChatMessage = {
            id: eventMessage.messageId,
            content: eventMessage.content,
            roomId: eventMessage.roomId,
            senderId: eventMessage.senderId,
            senderName: eventMessage.senderName ?? 'Unknown User',
            type: (() => {
              const isCurrentUser = this.currentUserId && eventMessage.senderId === this.currentUserId;
              console.log(`🔧 CLAUDE-DOM-ATTRIBUTION-DEBUG: DOM event senderId="${eventMessage.senderId}", currentUserId="${this.currentUserId}", isCurrentUser=${isCurrentUser}`);
              return isCurrentUser ? 'user' : 'assistant';
            })(),
            timestamp: eventMessage.timestamp
          };
          
          this.messages.push(newMessage);
          
          // Super efficient: just render and append the new message row
          this.appendMessageRow(newMessage);
          console.log(`✅ ChatWidget: Added new message row via DOM event (React-like row rendering)`);
          return;
        }
      }
    }
    
    // Fallback: reload from database if we couldn't parse the event
    try {
      console.log(`🔄 ChatWidget: Falling back to database reload`);
      await this.loadRoomHistory();
      await this.renderWidget();
      console.log(`✅ ChatWidget: Reloaded messages after DOM event`);
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to reload after DOM event:`, error);
    }
  }
  
  /**
   * Handle incoming room events from RoomEventSystem
   */
  public async handleRoomEvent(eventType: string, eventData: any): Promise<void> {
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
    
    if (eventData.roomId === this.currentRoom) {
      const newMessage: ChatMessage = {
        id: eventData.messageId,
        content: eventData.content,
        roomId: eventData.roomId,
        senderId: eventData.senderId,
        senderName: eventData.senderName,
        type: (this.currentUserId && eventData.senderId === this.currentUserId) ? 'user' : 'assistant',
        timestamp: eventData.timestamp
      };
      
      this.messages.push(newMessage);
      await this.renderWidget(); // Re-render with new message
    }
  }

  /**
   * Handle user joined events - STRICT TYPING
   */
  private async onUserJoined(eventData: ChatParticipantEventData): Promise<void> {
    console.log(`👋 ChatWidget: User ${eventData.userName} joined room ${this.currentRoom}`);
    
    if (eventData.roomId === this.currentRoom) {
      // Add system message for user join
      const systemMessage: ChatMessage = {
        id: `system_${Date.now()}`,
        content: `${eventData.userName} joined the room`,
        roomId: this.currentRoom,
        senderId: 'system',
        senderName: 'System',
        type: 'assistant', // System messages appear as assistant messages
        timestamp: eventData.timestamp
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
      // Add system message for user leave
      const systemMessage: ChatMessage = {
        id: `system_${Date.now()}`,
        content: `${eventData.userName} left the room`,
        roomId: this.currentRoom,
        senderId: 'system',
        senderName: 'System',
        type: 'assistant', // System messages appear as assistant messages
        timestamp: eventData.timestamp
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
      return renderer.renderMessageContainer(msg);
    }).join('');
  }
  
  /**
   * Efficiently append a single message row without re-rendering entire widget
   */
  private appendMessageRow(message: ChatMessage): void {
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
    
    const messageRowHTML = renderer.renderMessageContainer(message);
    
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
      console.log('🔧 CLAUDE-DEBUG: keydown event triggered, key:', e.key);
      if (e.key === 'Enter') {
        console.log('🔧 CLAUDE-DEBUG: Enter key pressed, calling sendMessage');
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
    console.log('🔧 CLAUDE-DEBUG: sendMessage called');
    if (!this.messageInput) return;
    
    const content = this.messageInput.value.trim();
    if (!content) return;
    
    // Create user message using persistent User ID
    const userMessage: ChatMessage = {
      id: `msg_${Date.now()}`,
      content,
      roomId: this.currentRoom,
      senderId: this.currentUserId ?? 'current_user', // Use persistent User ID
      senderName: 'You',
      type: 'user',
      timestamp: new Date().toISOString()
    };
    
    // Add to messages and render immediately (optimistic UI update)
    this.messages.push(userMessage);
    this.appendMessageRow(userMessage);
    this.messageInput.value = '';
    
    try {
      // Use existing chat/send-message command with proper types
      console.log(`🔧 CLAUDE-DEBUG: About to execute chat/send-message command`);
      const sendResult = await this.executeCommand<ChatSendMessageResult>('chat/send-message', {
        content: content,  // ← Fixed: use 'content' parameter as expected by server
        roomId: this.currentRoom,
        senderType: 'user' // Explicitly mark as user message - server will use UserIdManager
      });
      
      console.log(`🔧 CLAUDE-DEBUG: executeCommand returned:`, sendResult);
      console.log(`🔧 CLAUDE-DEBUG: sendResult type:`, typeof sendResult);
      console.log(`🔧 CLAUDE-DEBUG: sendResult.success:`, sendResult?.success);
      
      // executeCommand might return undefined if result.commandResult is undefined
      // Fall back to checking if the command executed without throwing
      if (sendResult && sendResult.success) {
        console.log(`✅ ChatWidget: Message sent to room ${this.currentRoom}`, sendResult);
        // Message already added optimistically, and events will trigger React-like updates
        // No need to reload - let the DOM events handle it efficiently
      } else {
        const errorMsg = sendResult?.error || `Send failed: ${JSON.stringify(sendResult)}`;
        console.error(`❌ ChatWidget: Failed to send message:`, errorMsg);
        this.handleError(errorMsg, 'sendMessage');
      }
      
    } catch (error) {
      console.error('❌ ChatWidget: Failed to send message:', error);
      this.handleError(error, 'sendMessage');
    }
  }

  /**
   * Get or create persistent User ID that survives browser sessions
   * Foundation for proper User domain objects
   */
  private getPersistentUserId(): string {
    const STORAGE_KEY = 'continuum_user_id';
    const DEFAULT_USER_ID = 'user-joel-12345'; // Matches fake-users.json
    
    // Try to get from localStorage first
    if (typeof localStorage !== 'undefined') {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        console.log(`🔧 CLAUDE-USER-ID-DEBUG: Retrieved persistent User ID from localStorage: ${stored}`);
        return stored;
      }
    }
    
    // Set up persistent User ID and store it
    const persistentUserId = DEFAULT_USER_ID;
    
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, persistentUserId);
      console.log(`🔧 CLAUDE-USER-ID-DEBUG: Created and stored persistent User ID: ${persistentUserId}`);
    }
    
    return persistentUserId;
  }

  private getRoomDisplayName(): string {
    // Capitalize the room name for display
    return this.currentRoom.charAt(0).toUpperCase() + this.currentRoom.slice(1);
  }
}