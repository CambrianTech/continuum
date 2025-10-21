/**
 * Shared Types Index - Central export for all shared types
 * 
 * This provides clean imports across the entire system:
 * import { UserPersona, ChatRoom, CommandResult } from '../types/shared';
 */

// Core system types
export * from './core/UserPersona';

// Session management types  
export * from './session/SessionTypes';

// Chat system types (after session to avoid conflicts)
export * from './chat/ChatTypes';

// Network communication types (renamed to avoid MessageType conflict)
export type { 
  WebSocketMessage, 
  CommandMessage, 
  ResponseMessage, 
  EventMessage, 
  APIResponse
} from './network/NetworkTypes';
export { MessageType as NetworkMessageType } from './network/NetworkTypes';

// Command system types
export * from './commands/CommandTypes';