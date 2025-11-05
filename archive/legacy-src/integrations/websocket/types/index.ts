/**
 * WebSocket Module Types - Main Exports
 * All TypeScript interfaces and types for the WebSocket module
 */

export * from './Connection';
export type { WebSocketMessage, CommandRequest, EventMessage, AuthRequest, AuthResult, ClientInfo } from './Message';
export type { DaemonConnection, CommandProcessor, EventProcessor, CommandDefinition, ParameterDefinition, EventResult } from './Daemon';
export type { CommandResult } from '../../../types/shared/CommandTypes'; // Use shared CommandResult as primary