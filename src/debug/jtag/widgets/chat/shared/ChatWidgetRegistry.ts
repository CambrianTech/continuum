/**
 * Chat Widget Registry - Easy registration and usage of coordinated chat widgets
 * 
 * Provides a simple interface to register and use the three coordinated chat widgets:
 * - UserListWidget: Shows participants with search, star, status
 * - RoomListWidget: Room navigation with tabs and unread counts
 * - ChatWidgetBrowser: Real-time messaging in selected room
 * 
 * Usage:
 *   import { ChatWidgetRegistry } from './ChatWidgetRegistry';
 *   
 *   // Register all chat widgets
 *   ChatWidgetRegistry.registerAll();
 *   
 *   // Use in HTML
 *   <user-list-widget></user-list-widget>
 *   <room-list-widget></room-list-widget>
 *   <chat-widget></chat-widget>
 */

import { UserListWidget } from '../user-list/browser/UserListWidget';
import { RoomListWidget } from '../room-list/browser/RoomListWidget';
import { ChatWidgetBrowser } from '../chat-widget/browser/ChatWidgetBrowser';
import { DEFAULT_CHAT_CONFIG } from './shared/ChatModuleTypes';

export interface ChatWidgetRegistryConfig {
  // Widget element names (customizable)
  userListElementName?: string;
  roomListElementName?: string;
  chatWidgetElementName?: string;
  
  // Chat configuration
  chatConfig?: typeof DEFAULT_CHAT_CONFIG;
  
  // Feature flags
  autoRegister?: boolean;
  logRegistration?: boolean;
}

export class ChatWidgetRegistry {
  private static isRegistered = false;
  private static config: ChatWidgetRegistryConfig = {
    userListElementName: 'user-list-widget',
    roomListElementName: 'room-list-widget', 
    chatWidgetElementName: 'chat-widget',
    chatConfig: DEFAULT_CHAT_CONFIG,
    autoRegister: true,
    logRegistration: true
  };

  /**
   * Register all chat widgets with default names
   */
  static registerAll(config: Partial<ChatWidgetRegistryConfig> = {}): void {
    this.config = { ...this.config, ...config };
    
    if (this.isRegistered) {
      if (this.config.logRegistration) {
        console.warn('🔄 ChatWidgetRegistry: Already registered, skipping');
      }
      return;
    }

    try {
      // Register User List Widget
      if (!customElements.get(this.config.userListElementName!)) {
        customElements.define(this.config.userListElementName!, UserListWidget);
        if (this.config.logRegistration) {
          console.log(`✅ Registered: <${this.config.userListElementName!}>`);
        }
      }

      // Register Room List Widget  
      if (!customElements.get(this.config.roomListElementName!)) {
        customElements.define(this.config.roomListElementName!, RoomListWidget);
        if (this.config.logRegistration) {
          console.log(`✅ Registered: <${this.config.roomListElementName!}>`);
        }
      }

      // Register Chat Widget
      if (!customElements.get(this.config.chatWidgetElementName!)) {
        customElements.define(this.config.chatWidgetElementName!, ChatWidgetBrowser);
        if (this.config.logRegistration) {
          console.log(`✅ Registered: <${this.config.chatWidgetElementName!}>`);
        }
      }

      this.isRegistered = true;

      if (this.config.logRegistration) {
        console.log('🎯 ChatWidgetRegistry: All coordinated chat widgets registered successfully');
        console.log('📋 Usage: Add these elements to your HTML:');
        console.log(`   <${this.config.userListElementName!}></${this.config.userListElementName!}>`);
        console.log(`   <${this.config.roomListElementName!}></${this.config.roomListElementName!}>`);
        console.log(`   <${this.config.chatWidgetElementName!}></${this.config.chatWidgetElementName!}>`);
      }

    } catch (error) {
      console.error('❌ ChatWidgetRegistry: Registration failed:', error);
      throw error;
    }
  }

  /**
   * Register individual widgets
   */
  static registerUserListWidget(elementName = 'user-list-widget'): void {
    if (!customElements.get(elementName)) {
      customElements.define(elementName, UserListWidget);
      if (this.config.logRegistration) {
        console.log(`✅ Registered UserListWidget as: <${elementName}>`);
      }
    }
  }

  static registerRoomListWidget(elementName = 'room-list-widget'): void {
    if (!customElements.get(elementName)) {
      customElements.define(elementName, RoomListWidget);
      if (this.config.logRegistration) {
        console.log(`✅ Registered RoomListWidget as: <${elementName}>`);
      }
    }
  }

  static registerChatWidget(elementName = 'chat-widget'): void {
    if (!customElements.get(elementName)) {
      customElements.define(elementName, ChatWidgetBrowser);
      if (this.config.logRegistration) {
        console.log(`✅ Registered ChatWidgetBrowser as: <${elementName}>`);
      }
    }
  }

  /**
   * Check if widgets are registered
   */
  static isWidgetRegistered(elementName: string): boolean {
    return !!customElements.get(elementName);
  }

  static areAllWidgetsRegistered(): boolean {
    return this.isWidgetRegistered(this.config.userListElementName!) &&
           this.isWidgetRegistered(this.config.roomListElementName!) &&
           this.isWidgetRegistered(this.config.chatWidgetElementName!);
  }

  /**
   * Get widget registration status
   */
  static getRegistrationStatus(): {
    userList: boolean;
    roomList: boolean;
    chatWidget: boolean;
    allRegistered: boolean;
  } {
    return {
      userList: this.isWidgetRegistered(this.config.userListElementName!),
      roomList: this.isWidgetRegistered(this.config.roomListElementName!),
      chatWidget: this.isWidgetRegistered(this.config.chatWidgetElementName!),
      allRegistered: this.areAllWidgetsRegistered()
    };
  }

  /**
   * Programmatically create widget instances
   */
  static createUserListWidget(): UserListWidget {
    if (!this.isWidgetRegistered(this.config.userListElementName!)) {
      this.registerUserListWidget();
    }
    return document.createElement(this.config.userListElementName!) as UserListWidget;
  }

  static createRoomListWidget(): RoomListWidget {
    if (!this.isWidgetRegistered(this.config.roomListElementName!)) {
      this.registerRoomListWidget();
    }
    return document.createElement(this.config.roomListElementName!) as RoomListWidget;
  }

  static createChatWidget(): ChatWidgetBrowser {
    if (!this.isWidgetRegistered(this.config.chatWidgetElementName!)) {
      this.registerChatWidget();
    }
    return document.createElement(this.config.chatWidgetElementName!) as ChatWidgetBrowser;
  }

  /**
   * Create complete coordinated chat system
   */
  static createChatSystem(containerElement?: HTMLElement): {
    userList: UserListWidget;
    roomList: RoomListWidget;
    chatWidget: ChatWidgetBrowser;
    container?: HTMLElement;
  } {
    // Ensure all widgets are registered
    this.registerAll();

    // Create widget instances
    const userList = this.createUserListWidget();
    const roomList = this.createRoomListWidget();
    const chatWidget = this.createChatWidget();

    // Add to container if provided
    if (containerElement) {
      containerElement.appendChild(userList);
      containerElement.appendChild(roomList);
      containerElement.appendChild(chatWidget);
    }

    return {
      userList,
      roomList,
      chatWidget,
      container: containerElement
    };
  }

  /**
   * Reset registration (for testing)
   */
  static reset(): void {
    this.isRegistered = false;
    console.log('🔄 ChatWidgetRegistry: Reset for re-registration');
  }

  /**
   * Get current configuration
   */
  static getConfig(): ChatWidgetRegistryConfig {
    return { ...this.config };
  }
}

// Auto-register if enabled
if (typeof window !== 'undefined' && DEFAULT_CHAT_CONFIG) {
  // Register when DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      ChatWidgetRegistry.registerAll();
    });
  } else {
    // DOM already loaded
    ChatWidgetRegistry.registerAll();
  }
}