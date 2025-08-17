/**
 * Chat Daemon Browser - UI Interface for Chat System
 * 
 * Browser-side chat daemon that delegates all operations to server via data daemon.
 * Handles UI updates and user interactions only.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import {
  ChatCreateRoomParams,
  ChatJoinRoomParams,
  ChatSendMessageParams,
  ChatListRoomsParams,
  ChatLeaveRoomParams
} from '../shared/ChatTypes';

/**
 * Chat operation payload for browser operations
 */
interface ChatBrowserPayload {
  readonly operation: 'update_ui' | 'show_notification' | 'highlight_message' | 'scroll_to_message';
  readonly params: any;
}

/**
 * ChatDaemonBrowser - Browser Chat Interface
 * 
 * Thin browser client that delegates all data operations to server.
 * Focuses purely on UI updates and user interaction.
 */
export class ChatDaemonBrowser extends DaemonBase {
  public readonly subpath: string = 'chat';
  private chatContainer: HTMLElement | null = null;
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('chat-daemon-browser', context, router);
  }

  /**
   * Handle incoming browser-specific messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as unknown as ChatBrowserPayload;
    
    try {
      switch (payload.operation) {
        case 'update_ui':
          await this.updateChatUI(payload.params);
          break;
        case 'show_notification':
          await this.showNotification(payload.params);
          break;
        case 'highlight_message':
          await this.highlightMessage(payload.params);
          break;
        case 'scroll_to_message':
          await this.scrollToMessage(payload.params);
          break;
        default:
          return createBaseResponse(false, this.context, this.context.uuid, {
            error: `Unknown browser chat operation: ${payload.operation}`
          });
      }
      
      return createBaseResponse(true, this.context, this.context.uuid, {
        operation: payload.operation,
        timestamp: new Date().toISOString()
      });
      
    } catch (error: any) {
      return createBaseResponse(false, this.context, this.context.uuid, {
        error: `Chat browser daemon error: ${error.message}`
      });
    }
  }


  /**
   * Initialize chat container in DOM
   */
  private initializeChatContainer(): void {
    this.chatContainer = document.querySelector('#chat-container') || 
                        document.querySelector('.chat-container') ||
                        document.querySelector('[data-chat-container]');
    
    if (!this.chatContainer) {
      console.log('💬 ChatDaemonBrowser: No chat container found in DOM');
    } else {
      console.log('💬 ChatDaemonBrowser: Connected to chat container');
    }
  }

  /**
   * Set up UI event listeners for chat interactions
   * TEMPORARILY DISABLED to prevent infinite loops with ChatWidget
   */
  private setupUIEventListeners(): void {
    // TODO: Re-enable these listeners with proper loop prevention
    // Currently disabled because ChatWidget triggers these events,
    // causing infinite delegation loops
    
    console.log('💬 ChatDaemonBrowser: Event listeners disabled (preventing widget loops)');
  }

  /**
   * Update chat UI with new data
   */
  private async updateChatUI(params: any): Promise<void> {
    if (!this.chatContainer) {
      console.warn('💬 ChatDaemonBrowser: No chat container available for UI update');
      return;
    }

    // Update UI based on operation type
    switch (params.type) {
      case 'new-message':
        this.displayNewMessage(params.message);
        break;
      case 'participant-joined':
        this.displayParticipantJoined(params.participant);
        break;
      case 'participant-left':
        this.displayParticipantLeft(params.participant);
        break;
      case 'room-list-updated':
        this.updateRoomList(params.rooms);
        break;
      default:
        console.log(`💬 ChatDaemonBrowser: Unknown UI update type: ${params.type}`);
    }
  }

  /**
   * Display new message in chat UI
   */
  private displayNewMessage(message: any): void {
    if (!this.chatContainer) return;
    
    const messageElement = document.createElement('div');
    messageElement.className = 'chat-message';
    messageElement.innerHTML = `
      <div class="message-header">
        <span class="sender-name">${message.senderName}</span>
        <span class="timestamp">${new Date(message.timestamp).toLocaleTimeString()}</span>
      </div>
      <div class="message-content">${message.content}</div>
    `;
    
    this.chatContainer.appendChild(messageElement);
    this.scrollToBottom();
  }

  /**
   * Show notification to user
   */
  private async showNotification(params: any): Promise<void> {
    console.log(`🔔 ChatDaemonBrowser: ${params.message}`);
    
    // Could integrate with browser notification API
    if ('Notification' in window && Notification.permission === 'granted') {
      new Notification('Chat System', {
        body: params.message,
        icon: '/favicon.ico'
      });
    }
  }

  /**
   * Highlight specific message
   */
  private async highlightMessage(params: any): Promise<void> {
    const messageElement = document.querySelector(`[data-message-id="${params.messageId}"]`);
    if (messageElement) {
      messageElement.classList.add('highlighted');
      setTimeout(() => messageElement.classList.remove('highlighted'), 3000);
    }
  }

  /**
   * Scroll to specific message
   */
  private async scrollToMessage(params: any): Promise<void> {
    const messageElement = document.querySelector(`[data-message-id="${params.messageId}"]`);
    if (messageElement) {
      messageElement.scrollIntoView({ behavior: 'smooth' });
    }
  }

  /**
   * Display participant joined notification
   */
  private displayParticipantJoined(participant: any): void {
    console.log(`👋 ${participant.displayName} joined the chat`);
    // UI update logic would go here
  }

  /**
   * Display participant left notification
   */
  private displayParticipantLeft(participant: any): void {
    console.log(`👋 ${participant.displayName} left the chat`);
    // UI update logic would go here
  }

  /**
   * Update room list in UI
   */
  private updateRoomList(rooms: any[]): void {
    console.log(`📋 Updated room list: ${rooms.length} rooms available`);
    // Room list UI update logic would go here
  }

  /**
   * Scroll chat container to bottom
   */
  private scrollToBottom(): void {
    if (this.chatContainer) {
      this.chatContainer.scrollTop = this.chatContainer.scrollHeight;
    }
  }

  /**
   * Delegate operation to server via router
   */
  private async delegateToServer(operation: string, params: any): Promise<void> {
    try {
      const message = this.createRequestMessage(`chat/${operation}`, {
        ...params,
        sessionId: this.context.uuid
      });
      
      await this.router.postMessage(message);
      console.log(`📨 ChatDaemonBrowser: Delegated ${operation} to server`);
      
    } catch (error) {
      console.error(`❌ ChatDaemonBrowser: Failed to delegate ${operation}:`, error);
    }
  }

  /**
   * Initialize daemon-specific functionality
   */
  protected async initialize(): Promise<void> {
    console.log('💬 ChatDaemonBrowser: Initializing browser chat interface');
    
    // Find or create chat container in DOM
    this.initializeChatContainer();
    
    // Set up UI event listeners
    this.setupUIEventListeners();
  }
}