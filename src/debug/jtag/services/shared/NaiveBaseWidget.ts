/**
 * Naive BaseWidget - Clean Service-Based Widget Foundation
 * 
 * Demonstrates clean architecture using dependency injection and services
 * instead of the 780-line god class with hardcoded daemon connections.
 * 
 * This shows how widgets should work:
 * - Clean service injection
 * - One-line operations instead of 45-line methods  
 * - Zero hardcoded daemon connections
 * - Proper separation of concerns
 * - Type safety throughout
 * 
 * Compare to BaseWidget.ts for architectural improvement demonstration.
 */

import type { JTAGContext } from '../../system/core/types/JTAGTypes';
import type { IChatService } from '../chat/ChatService';
import type { IUserService } from '../user/UserService';
import type { IAIService } from '../ai/AIService';
import type { IServiceRegistry } from './ServiceBase';
import type { BaseUser } from '../../api/types/User';

export interface IWidgetServices {
  chat: IChatService;
  user: IUserService;
  ai: IAIService;
}

export interface NaiveWidgetConfig {
  widgetId: string;
  widgetName: string;
  version?: string;
  template?: string;
  customTheme?: Record<string, string>;
}

/**
 * Clean widget base using dependency injection and services.
 * 
 * Instead of 780 lines with hardcoded daemon connections,
 * this provides clean abstractions through service injection.
 */
export abstract class NaiveBaseWidget extends HTMLElement {
  protected readonly widgetId: string;
  protected readonly widgetName: string;
  protected readonly services: IWidgetServices;
  protected readonly context: JTAGContext;
  
  private currentUser: BaseUser | null = null;

  constructor(
    config: NaiveWidgetConfig,
    serviceRegistry: IServiceRegistry,
    context: JTAGContext
  ) {
    super();
    
    this.widgetId = config.widgetId;
    this.widgetName = config.widgetName;
    this.context = context;
    
    // Clean service injection - no hardcoded daemon connections!
    this.services = {
      chat: serviceRegistry.get<IChatService>('ChatService')!,
      user: serviceRegistry.get<IUserService>('UserService')!,
      ai: serviceRegistry.get<IAIService>('AIService')!
    };
    
    this.validateServices();
    this.initializeWidget();
  }

  /**
   * Widget lifecycle - template method pattern
   */
  private async initializeWidget(): Promise<void> {
    await this.onWidgetInitialize();
    await this.loadCurrentUser();
    await this.renderWidget();
    this.onWidgetReady();
  }

  // Abstract methods for subclasses to implement
  protected abstract onWidgetInitialize(): Promise<void>;
  protected abstract renderWidget(): Promise<void>;
  protected abstract onWidgetReady(): void;

  /**
   * Current user management - simple one-liner vs BaseWidget's complexity
   */
  protected async getCurrentUser(): Promise<BaseUser | null> {
    if (!this.currentUser) {
      this.currentUser = await this.services.user.getCurrentUser();
    }
    return this.currentUser;
  }

  private async loadCurrentUser(): Promise<void> {
    try {
      this.currentUser = await this.services.user.getCurrentUser();
    } catch (error) {
      console.warn(`${this.widgetName}: Could not load current user:`, error);
    }
  }

  /**
   * Chat operations - clean one-liners instead of BaseWidget's 45-line methods
   */
  protected async sendChatMessage(roomId: string, text: string): Promise<boolean> {
    try {
      const user = await this.getCurrentUser();
      if (!user) return false;

      const result = await this.services.chat.sendMessage({
        roomId,
        content: { text },
        sender: user
      });

      return result.success;
    } catch (error) {
      console.error(`${this.widgetName}: Failed to send message:`, error);
      return false;
    }
  }

  protected async createChatRoom(name: string, description?: string): Promise<string | null> {
    try {
      const user = await this.getCurrentUser();
      if (!user) return null;

      const result = await this.services.chat.createRoom({
        name,
        description,
        creator: user
      });

      return result.success ? result.roomId : null;
    } catch (error) {
      console.error(`${this.widgetName}: Failed to create room:`, error);
      return null;
    }
  }

  protected async joinChatRoom(roomId: string): Promise<boolean> {
    try {
      const user = await this.getCurrentUser();
      if (!user) return false;

      const result = await this.services.chat.joinRoom({ roomId, user });
      return result.success;
    } catch (error) {
      console.error(`${this.widgetName}: Failed to join room:`, error);
      return false;
    }
  }

  /**
   * AI operations - direct service access instead of hardcoded daemon calls
   */
  protected async chatWithPersona(personaId: string, message: string): Promise<string | null> {
    try {
      const response = await this.services.ai.sendPersonaMessage(personaId, message, {
        widgetContext: this.widgetName,
        userId: this.currentUser?.id
      });
      return response;
    } catch (error) {
      console.error(`${this.widgetName}: Failed to chat with persona:`, error);
      return null;
    }
  }

  protected async executeAgentTask(agentId: string, command: string, params: Record<string, any>): Promise<any> {
    try {
      return await this.services.ai.executeAgentCommand(agentId, command, params);
    } catch (error) {
      console.error(`${this.widgetName}: Failed to execute agent task:`, error);
      return null;
    }
  }

  /**
   * User operations - clean service-based access
   */
  protected async getUserCapabilities(): Promise<string[]> {
    const user = await this.getCurrentUser();
    return user ? this.services.user.getUserCapabilities(user) : [];
  }

  protected async checkPermission(action: string, resource: string): Promise<boolean> {
    const user = await this.getCurrentUser();
    return user ? this.services.user.checkUserPermission(user, action, resource) : false;
  }

  protected async searchUsers(query: string): Promise<BaseUser[]> {
    return await this.services.user.searchUsers(query);
  }

  /**
   * Event broadcasting - using proper service abstraction
   */
  protected broadcastWidgetEvent(eventType: string, data: any): void {
    const event = new CustomEvent(`widget:${eventType}`, {
      detail: {
        widgetId: this.widgetId,
        widgetName: this.widgetName,
        data
      }
    });
    
    this.dispatchEvent(event);
    document.dispatchEvent(event); // Global event bus
  }

  /**
   * Service validation - ensure all required services are available
   */
  private validateServices(): void {
    const requiredServices: (keyof IWidgetServices)[] = ['chat', 'user', 'ai'];
    
    for (const serviceName of requiredServices) {
      if (!this.services[serviceName]) {
        throw new Error(`${this.widgetName}: Required service '${serviceName}' not available in service registry`);
      }
    }
  }

  /**
   * Utility methods for common operations
   */
  protected log(message: string, data?: any): void {
    console.log(`🧩 ${this.widgetName}[${this.widgetId}]:`, message, data || '');
  }

  protected error(message: string, error?: any): void {
    console.error(`❌ ${this.widgetName}[${this.widgetId}]:`, message, error || '');
  }
}

/**
 * Factory function for creating widgets with service injection
 */
export function createNaiveWidget<T extends NaiveBaseWidget>(
  WidgetClass: new (config: NaiveWidgetConfig, registry: IServiceRegistry, context: JTAGContext) => T,
  config: NaiveWidgetConfig,
  serviceRegistry: IServiceRegistry,
  context: JTAGContext
): T {
  return new WidgetClass(config, serviceRegistry, context);
}