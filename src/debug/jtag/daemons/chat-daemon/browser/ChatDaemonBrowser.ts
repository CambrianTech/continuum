/**
 * Chat Daemon Browser - UI and Real-time Updates
 * 
 * Handles browser-specific chat functionality:
 * - Real-time UI updates for chat messages
 * - User input and message sending
 * - Chat room visualization
 * - WebSocket-based event subscriptions
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { ChatDaemon, CHAT_EVENTS, type ChatMessage, type ChatCitizen } from '../shared/ChatDaemon';

export class ChatDaemonBrowser extends ChatDaemon {
  private chatContainer: HTMLElement | null = null;
  private currentRoomId: string | null = null;
  private currentCitizenId: string | null = null;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize browser-specific functionality
   */
  protected async initialize(): Promise<void> {
    console.log(`💬 ${this.toString()}: Initializing browser chat daemon`);
    
    // Set up event listeners for chat events
    this.setupEventListeners();
    
    // Initialize UI (will be created by chat widget)
    await this.initializeChatUI();
  }

  /**
   * Set up event listeners for real-time updates
   */
  private setupEventListeners(): void {
    // Listen for messages in subscribed rooms
    this.eventManager.events.on(CHAT_EVENTS.MESSAGE_SENT, (data: any) => {
      if (data.message && this.isSubscribedToRoom(data.message.roomId)) {
        this.displayNewMessage(data.message);
      }
    });

    // Listen for citizens joining/leaving
    this.eventManager.events.on(CHAT_EVENTS.CITIZEN_JOINED, (data: any) => {
      if (this.isSubscribedToRoom(data.roomId)) {
        this.updateParticipantList(data);
      }
    });

    this.eventManager.events.on(CHAT_EVENTS.CITIZEN_LEFT, (data: any) => {
      if (this.isSubscribedToRoom(data.roomId)) {
        this.updateParticipantList(data);
      }
    });

    // Listen for AI responses
    this.eventManager.events.on(CHAT_EVENTS.AI_RESPONSE, (data: any) => {
      if (data.message && this.isSubscribedToRoom(data.message.roomId)) {
        this.displayAIResponse(data.message);
      }
    });
  }

  /**
   * Initialize chat UI - connects to chat widget
   */
  private async initializeChatUI(): Promise<void> {
    // Look for chat-widget element
    const chatWidget = document.querySelector('chat-widget');
    if (chatWidget) {
      this.chatContainer = chatWidget as HTMLElement;
      await this.setupChatWidget();
    } else {
      console.warn(`💬 ${this.toString()}: chat-widget element not found, will initialize when available`);
    }
  }

  /**
   * Set up the chat widget with controls
   */
  private async setupChatWidget(): Promise<void> {
    if (!this.chatContainer) return;

    // Create basic chat UI structure
    this.chatContainer.innerHTML = `
      <div class="chat-container">
        <div class="chat-header">
          <h3>Chat</h3>
          <div class="room-selector">
            <select id="room-select">
              <option value="">Select Room...</option>
            </select>
            <button id="join-room-btn">Join</button>
          </div>
        </div>
        
        <div class="chat-messages" id="chat-messages">
          <div class="welcome-message">Welcome to the chat system! Join a room to start chatting.</div>
        </div>
        
        <div class="chat-participants" id="chat-participants">
          <h4>Participants</h4>
          <div class="participants-list"></div>
        </div>
        
        <div class="chat-input" id="chat-input">
          <input type="text" id="message-input" placeholder="Type a message..." disabled>
          <button id="send-btn" disabled>Send</button>
        </div>
        
        <div class="chat-controls">
          <input type="text" id="citizen-name" placeholder="Your name" value="User${Math.floor(Math.random()*1000)}">
          <select id="citizen-type">
            <option value="user">Human</option>
            <option value="agent">AI Agent</option>
            <option value="persona">Persona</option>
          </select>
        </div>
      </div>
    `;

    // Add basic styling
    const style = document.createElement('style');
    style.textContent = `
      .chat-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: #1a1f2e;
        color: #e0e6ed;
        padding: 1rem;
      }
      
      .chat-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        border-bottom: 1px solid #2d3748;
        padding-bottom: 1rem;
        margin-bottom: 1rem;
      }
      
      .room-selector {
        display: flex;
        gap: 0.5rem;
        align-items: center;
      }
      
      .chat-messages {
        flex: 1;
        overflow-y: auto;
        border: 1px solid #2d3748;
        padding: 1rem;
        margin-bottom: 1rem;
        border-radius: 4px;
        background: #0f1419;
        min-height: 300px;
      }
      
      .chat-message {
        margin-bottom: 0.5rem;
        padding: 0.5rem;
        border-radius: 4px;
        background: #1a1f2e;
      }
      
      .chat-message.ai-response {
        background: #2d4a22;
        border-left: 3px solid #48bb78;
      }
      
      .message-sender {
        font-weight: bold;
        margin-bottom: 0.25rem;
      }
      
      .message-timestamp {
        font-size: 0.8rem;
        opacity: 0.7;
        margin-left: 0.5rem;
      }
      
      .chat-participants {
        border: 1px solid #2d3748;
        padding: 1rem;
        border-radius: 4px;
        margin-bottom: 1rem;
        background: #0f1419;
      }
      
      .participants-list {
        margin-top: 0.5rem;
      }
      
      .participant {
        padding: 0.25rem 0;
        display: flex;
        align-items: center;
        gap: 0.5rem;
      }
      
      .participant-type {
        font-size: 0.8rem;
        opacity: 0.7;
      }
      
      .chat-input {
        display: flex;
        gap: 0.5rem;
        margin-bottom: 1rem;
      }
      
      .chat-input input {
        flex: 1;
        padding: 0.5rem;
        border: 1px solid #2d3748;
        border-radius: 4px;
        background: #1a1f2e;
        color: #e0e6ed;
      }
      
      .chat-controls {
        display: flex;
        gap: 0.5rem;
      }
      
      .chat-controls input,
      .chat-controls select,
      button {
        padding: 0.5rem;
        border: 1px solid #2d3748;
        border-radius: 4px;
        background: #1a1f2e;
        color: #e0e6ed;
      }
      
      button {
        background: #3182ce;
        cursor: pointer;
      }
      
      button:hover {
        background: #2c5aa0;
      }
      
      button:disabled {
        background: #4a5568;
        cursor: not-allowed;
      }
      
      .welcome-message {
        text-align: center;
        padding: 2rem;
        opacity: 0.7;
      }
    `;
    document.head.appendChild(style);

    // Set up event handlers
    await this.setupEventHandlers();
    
    // Load available rooms
    await this.loadAvailableRooms();
  }

  /**
   * Set up UI event handlers
   */
  private async setupEventHandlers(): Promise<void> {
    const joinBtn = document.getElementById('join-room-btn');
    const sendBtn = document.getElementById('send-btn');
    const messageInput = document.getElementById('message-input') as HTMLInputElement;
    const roomSelect = document.getElementById('room-select') as HTMLSelectElement;

    if (joinBtn) {
      joinBtn.addEventListener('click', () => this.handleJoinRoomFromUI());
    }

    if (sendBtn) {
      sendBtn.addEventListener('click', () => this.handleSendMessageFromUI());
    }

    if (messageInput) {
      messageInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          this.handleSendMessageFromUI();
        }
      });
    }

    // Auto-create some default rooms
    await this.createDefaultRooms();
  }

  /**
   * Create some default rooms for testing
   */
  private async createDefaultRooms(): Promise<void> {
    const defaultRooms = [
      { name: 'General Chat', category: 'general' },
      { name: 'AI Training', category: 'ai-training' },
      { name: 'Support', category: 'support' }
    ];

    for (const room of defaultRooms) {
      try {
        const message = JTAGMessageFactory.createRequest(
          this.context,
          'chat',
          'create-room',
          { 
            context: this.context,
            sessionId: this.context.uuid,
            ...room 
          },
          `create-room-${Date.now()}`
        );
        await this.router.postMessage(message);
      } catch (error) {
        console.error(`Failed to create default room ${room.name}:`, error);
      }
    }
  }

  /**
   * Load available rooms into selector
   */
  private async loadAvailableRooms(): Promise<void> {
    try {
      const message = JTAGMessageFactory.createRequest(
        this.context,
        'chat',
        'list-rooms',
        {
          context: this.context,
          sessionId: this.context.uuid
        },
        `list-rooms-${Date.now()}`
      );
      const response = await this.router.postMessage(message);
      const responsePayload = response as any;

      const roomSelect = document.getElementById('room-select') as HTMLSelectElement;
      if (roomSelect && responsePayload.success && responsePayload.rooms) {
        // Clear existing options except first
        roomSelect.innerHTML = '<option value="">Select Room...</option>';
        
        responsePayload.rooms.forEach((room: any) => {
          const option = document.createElement('option');
          option.value = room.roomId;
          option.textContent = `${room.name} (${room.participantCount} participants)`;
          roomSelect.appendChild(option);
        });
      }
    } catch (error) {
      console.error('Failed to load rooms:', error);
    }
  }

  /**
   * Handle joining a room (UI handler - different from daemon's handleJoinRoom)
   */
  private async handleJoinRoomFromUI(): Promise<void> {
    const roomSelect = document.getElementById('room-select') as HTMLSelectElement;
    const citizenNameInput = document.getElementById('citizen-name') as HTMLInputElement;
    const citizenTypeSelect = document.getElementById('citizen-type') as HTMLSelectElement;

    if (!roomSelect.value) {
      alert('Please select a room to join');
      return;
    }

    const citizenName = citizenNameInput.value.trim();
    if (!citizenName) {
      alert('Please enter your name');
      return;
    }

    try {
      const message = JTAGMessageFactory.createRequest(
        this.context,
        'chat',
        'join-room',
        {
          context: this.context,
          sessionId: this.context.uuid,
          roomId: roomSelect.value,
          citizenName,
          citizenType: citizenTypeSelect.value
        },
        `join-room-${Date.now()}`
      );
      const response = await this.router.postMessage(message);
      const responsePayload = response as any;

      if (responsePayload.success) {
        this.currentRoomId = responsePayload.roomId;
        this.currentCitizenId = responsePayload.citizenId;
        
        // Enable chat input
        const messageInput = document.getElementById('message-input') as HTMLInputElement;
        const sendBtn = document.getElementById('send-btn') as HTMLButtonElement;
        if (messageInput) messageInput.disabled = false;
        if (sendBtn) sendBtn.disabled = false;

        // Display welcome message
        this.displaySystemMessage(`Joined room: ${responsePayload.roomName}. ${responsePayload.participantCount} participants total.`);
        
        // Display recent messages
        if (responsePayload.recentMessages) {
          responsePayload.recentMessages.forEach((msg: ChatMessage) => {
            this.displayNewMessage(msg);
          });
        }

        console.log(`✅ Joined room ${responsePayload.roomName} as ${citizenName}`);
      } else {
        alert(`Failed to join room: ${responsePayload.error}`);
      }
    } catch (error) {
      console.error('Failed to join room:', error);
      alert('Failed to join room. Please try again.');
    }
  }

  /**
   * Handle sending a message (UI handler - different from daemon's handleSendMessage)
   */
  private async handleSendMessageFromUI(): Promise<void> {
    if (!this.currentRoomId || !this.currentCitizenId) {
      alert('Please join a room first');
      return;
    }

    const messageInput = document.getElementById('message-input') as HTMLInputElement;
    const content = messageInput.value.trim();
    
    if (!content) return;

    try {
      const message = JTAGMessageFactory.createRequest(
        this.context,
        'chat',
        'send-message',
        {
          context: this.context,
          sessionId: this.context.uuid,
          roomId: this.currentRoomId,
          citizenId: this.currentCitizenId,
          content
        },
        `send-message-${Date.now()}`
      );
      const response = await this.router.postMessage(message);
      const responsePayload = response as any;

      if (responsePayload.success) {
        messageInput.value = ''; // Clear input
      } else {
        alert(`Failed to send message: ${responsePayload.error}`);
      }
    } catch (error) {
      console.error('Failed to send message:', error);
      alert('Failed to send message. Please try again.');
    }
  }

  /**
   * Display new message in chat
   */
  private displayNewMessage(message: ChatMessage): void {
    const messagesContainer = document.getElementById('chat-messages');
    if (!messagesContainer) return;

    const messageEl = document.createElement('div');
    messageEl.className = 'chat-message';
    
    if (message.messageType === 'ai-response') {
      messageEl.classList.add('ai-response');
    }

    const timestamp = new Date(message.timestamp).toLocaleTimeString();
    
    messageEl.innerHTML = `
      <div class="message-sender">
        ${message.senderName} 
        <span class="participant-type">(${message.senderType})</span>
        <span class="message-timestamp">${timestamp}</span>
      </div>
      <div class="message-content">${this.escapeHtml(message.content)}</div>
    `;

    messagesContainer.appendChild(messageEl);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;

    // Remove welcome message if present
    const welcomeMsg = messagesContainer.querySelector('.welcome-message');
    if (welcomeMsg) {
      welcomeMsg.remove();
    }
  }

  /**
   * Display AI response with special styling
   */
  private displayAIResponse(message: ChatMessage): void {
    this.displayNewMessage(message);
    
    // Could add special effects for AI responses
    console.log(`🤖 AI Response from ${message.senderName}`);
  }

  /**
   * Display system message
   */
  private displaySystemMessage(content: string): void {
    const messagesContainer = document.getElementById('chat-messages');
    if (!messagesContainer) return;

    const messageEl = document.createElement('div');
    messageEl.className = 'chat-message system-message';
    messageEl.style.background = '#2d4a22';
    messageEl.style.fontStyle = 'italic';
    
    messageEl.innerHTML = `
      <div class="message-content">ℹ️ ${this.escapeHtml(content)}</div>
    `;

    messagesContainer.appendChild(messageEl);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;
  }

  /**
   * Update participant list
   */
  private updateParticipantList(data: any): void {
    // For now, just log the change
    console.log(`👥 Participant update:`, data);
    
    // TODO: Update actual participant list in UI
    // This would require tracking participants and updating the display
  }

  /**
   * Check if currently subscribed to room
   */
  private isSubscribedToRoom(roomId: string): boolean {
    return this.currentRoomId === roomId;
  }

  /**
   * Escape HTML to prevent XSS
   */
  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}