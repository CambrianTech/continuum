/**
 * Chat Data Service - Main Export
 * 
 * Provides the factory function expected by tests
 * Routes to server-only implementation since browser can't do database operations
 */

export { createChatDataService } from './server/ChatDataServiceServer';
export type { IChatDataService, ChatRoom, ChatMessage, ChatCitizen } from './shared/ChatDataTypes';