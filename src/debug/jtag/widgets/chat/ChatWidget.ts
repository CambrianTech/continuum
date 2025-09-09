/**
 * Chat Widget - JTAG-based Web Component using BaseWidget Architecture
 * 
 * Updated to use the new BaseWidget abstraction with one-line operations.
 * Demonstrates dramatic code simplification through proper abstraction.
 */

import { BaseWidget } from '../shared/BaseWidget';
import type { ChatMessage } from './shared/ChatModuleTypes';
import type { ChatSendMessageParams, ChatSendMessageResult } from '../../commands/chat/send-message/shared/ChatSendMessageTypes';

// Strict event data types - no more 'any'!
interface ChatMessageEventData {
  readonly eventType: 'chat:message-sent';
  readonly message: {
    readonly messageId: string;
    readonly content: string;
    readonly roomId: string;
    readonly senderId: string;
    readonly senderName: string;
    readonly timestamp: string;
  };
  readonly roomId: string;
}

interface UserJoinedEventData {
  readonly eventType: 'chat:user-joined';
  readonly userId: string;
  readonly userName: string;
  readonly roomId: string;
  readonly timestamp: string;
}

interface UserLeftEventData {
  readonly eventType: 'chat:user-left';
  readonly userId: string;
  readonly userName: string;
  readonly roomId: string;
  readonly timestamp: string;
}

export class ChatWidget extends BaseWidget {
  private messages: ChatMessage[] = [];
  private currentRoom: string;
  private messageInput?: HTMLInputElement;
  private eventSubscriptionId?: string;
  
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

  protected async onWidgetInitialize(): Promise<void> {
    console.log(`🎯 ChatWidget: Initializing for room "${this.currentRoom}"...`);
    
    // Load room message history using command abstraction
    await this.loadRoomHistory();
    
    // Subscribe to room-specific events
    await this.subscribeToRoomEvents();
    
    console.log(`✅ ChatWidget: Initialized for room "${this.currentRoom}" with ${this.messages.length} messages`);
  }

  /**
   * Load room message history using data/list command
   */
  private async loadRoomHistory(): Promise<void> {
    try {
      console.log(`📚 ChatWidget: Loading room history using data/list command`);
      
      // Use data/list command to get all chat messages, then filter by room
      const historyResult = await this.jtagOperation('data/list', {
        collection: 'chat_messages',
        limit: 50 // Recent messages
      }) as any;
      
      // Handle nested JTAG response structure - actual data is in commandResult
      const dataResult = (historyResult as any).commandResult || historyResult;
      
      if (historyResult && historyResult.success && dataResult.items) {
        // Filter messages for current room and convert to internal format  
        const roomMessages = dataResult.items
          .filter((item: any) => item.data && item.data.roomId === this.currentRoom)
          .map((item: any) => ({
            id: item.data.messageId || item.id || 'unknown-id',
            content: item.data.content || '[Message content not available]',
            roomId: item.data.roomId || this.currentRoom,
            senderId: item.data.senderId || 'unknown-sender',
            senderName: item.data.senderName || 'Unknown User',
            type: item.data.senderId === 'current_user' ? 'user' : 'assistant',
            timestamp: item.data.timestamp || new Date().toISOString()
          }))
          .sort((a: any, b: any) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()); // Sort by timestamp
        
        this.messages = roomMessages;
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
      
      // Use JTAG operation to subscribe to room events via the chat daemon
      const subscribeResult = await this.jtagOperation('chat/subscribe-room', {
        roomId: this.currentRoom,
        eventTypes: ['chat:message-received', 'chat:participant-joined', 'chat:participant-left']
      }) as any;
      
      if (subscribeResult && subscribeResult.success) {
        this.eventSubscriptionId = subscribeResult.subscriptionId;
        console.log(`✅ ChatWidget: Subscribed to room "${this.currentRoom}" events`);
        
        // Set up event handlers for real-time updates
        this.setupRoomEventHandlers();
      } else {
        throw new Error(`Room subscription failed: ${subscribeResult?.error || 'Unknown error'}`);
      }
      
    } catch (error) {
      console.error(`❌ ChatWidget: Failed to subscribe to room events:`, error);
      throw error; // No fallbacks - either it works or it fails
    }
  }

  /**
   * Set up event handlers for real-time room updates
   */
  private setupRoomEventHandlers(): void {
    // Use BaseWidget's router event system to handle incoming room events
    console.log(`📡 ChatWidget: Setting up room event handlers for room ${this.currentRoom}`);
    
    // This will be handled by the BaseWidget event routing system
    // When RoomEventSystem sends events, they'll be routed to this widget
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
        await this.onUserJoined(eventData as UserJoinedEventData);
        break;
      case 'chat:participant-left':
        await this.onUserLeft(eventData as UserLeftEventData);
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
        id: eventData.message.messageId,
        content: eventData.message.content,
        roomId: eventData.message.roomId,
        senderId: eventData.message.senderId,
        senderName: eventData.message.senderName,
        type: eventData.message.senderId === 'current_user' ? 'user' : 'assistant',
        timestamp: eventData.message.timestamp
      };
      
      this.messages.push(newMessage);
      await this.renderWidget(); // Re-render with new message
    }
  }

  /**
   * Handle user joined events - STRICT TYPING
   */
  private async onUserJoined(eventData: UserJoinedEventData): Promise<void> {
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
  private async onUserLeft(eventData: UserLeftEventData): Promise<void> {
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

  protected async renderWidget(): Promise<void> {
    // Use external template and styles loaded by BaseWidget
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';
    
    // Ensure template is a string before calling replace
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';
    
    // Replace dynamic content in template
    const dynamicContent = templateString
      .replace('<!-- ROOM_NAME -->', this.getRoomDisplayName())
      .replace('<!-- Dynamic messages rendered here -->', this.renderMessages());
    
    this.shadowRoot.innerHTML = `
      <style>${styles}</style>
      ${dynamicContent}
    `;
    
    // Cache input element
    this.messageInput = this.shadowRoot.getElementById('messageInput') as HTMLInputElement;
    
    // Setup event listeners
    this.setupEventListeners();
  }

  private renderMessages(): string {
    return this.messages.map(msg => `
      <div class="message ${msg.type}">
        ${msg.content}
      </div>
    `).join('');
  }

  private setupEventListeners(): void {
    // Send message on Enter or button click
    this.messageInput?.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        this.sendMessage();
      }
    });
    
    this.shadowRoot.getElementById('sendButton')?.addEventListener('click', () => {
      this.sendMessage();
    });
  }

  private async sendMessage(): Promise<void> {
    if (!this.messageInput) return;
    
    const content = this.messageInput.value.trim();
    if (!content) return;
    
    // Create user message
    const userMessage: ChatMessage = {
      id: `msg_${Date.now()}`,
      content,
      roomId: this.currentRoom,
      senderId: 'current_user',
      senderName: 'You',
      type: 'user',
      timestamp: new Date().toISOString()
    };
    
    // Add to messages and clear input
    this.messages.push(userMessage);
    this.messageInput.value = '';
    
    try {
      // Use existing chat/send-message command with proper types
      const sendResult = await this.jtagOperation<ChatSendMessageResult>('chat/send-message', {
        message: content,
        roomId: this.currentRoom,
        userId: 'current_user'
      });
      
      if (sendResult && sendResult.success) {
        console.log(`✅ ChatWidget: Message sent to room ${this.currentRoom}`);
        // Message will appear via event subscription when that's implemented
      } else {
        const errorMsg = sendResult?.error || 'Unknown error';
        console.error(`❌ ChatWidget: Failed to send message:`, errorMsg);
        this.handleError(errorMsg, 'sendMessage');
      }
      
    } catch (error) {
      console.error('❌ ChatWidget: Failed to send message:', error);
      this.handleError(error, 'sendMessage');
    }
  }

  private getRoomDisplayName(): string {
    // Capitalize the room name for display
    return this.currentRoom.charAt(0).toUpperCase() + this.currentRoom.slice(1);
  }

  private getChatContext(): any {
    return {
      recentMessages: this.messages.slice(-5),
      roomId: this.currentRoom,
      messageCount: this.messages.length
    };
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Unsubscribe from room events using JTAG abstraction
    if (this.eventSubscriptionId) {
      try {
        await this.jtagOperation('events/unsubscribe', {
          subscriptionId: this.eventSubscriptionId
        });
        console.log(`🔌 ChatWidget: Unsubscribed from room ${this.currentRoom} events`);
      } catch (error) {
        console.error(`❌ ChatWidget: Failed to unsubscribe from events:`, error);
      }
    }
    
    // Save room-specific messages using BaseWidget abstraction
    const roomMessageKey = `chat_messages_${this.currentRoom}`;
    await this.storeData(roomMessageKey, this.messages, { persistent: true });
    console.log(`✅ ChatWidget: Cleanup complete for room "${this.currentRoom}"`);
  }

  // Static property required by widget registration system
  static get widgetName(): string {
    return 'chat';
  }
}