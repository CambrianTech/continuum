/**
 * Client Services - Organized Domain Services Index
 * 
 * Re-exports all client services and provides factory for creating service container.
 * Maintains clean organization with each service in its own file.
 */

// Re-export service interfaces and implementations
export type { IChatService } from './ChatService';
export { ChatService } from './ChatService';

export type { IUserService } from './UserService';
export { UserService } from './UserService';

export type { IWidgetService } from './WidgetService';
export { WidgetService } from './WidgetService';

// Import for factory function
import type { JTAGClient } from '../JTAGClient';
import { ChatService, type IChatService } from './ChatService';
import { UserService, type IUserService } from './UserService';
import { WidgetService, type IWidgetService } from './WidgetService';

// Service container interface
export interface JTAGClientServiceContainer {
  chat: IChatService;
  users: IUserService;
  widgets: IWidgetService;
}

// Factory function to create services container
export function createJTAGClientServices(client: JTAGClient): JTAGClientServiceContainer {
  return {
    chat: new ChatService(client),
    users: new UserService(client),
    widgets: new WidgetService(client)
  };
}