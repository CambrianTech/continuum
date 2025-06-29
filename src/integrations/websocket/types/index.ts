/**
 * WebSocket Module Types - Main Exports
 * All TypeScript interfaces and types for the WebSocket module
 */

export * from './Connection';
export type { WebSocketMessage, CommandRequest, EventMessage, AuthRequest, AuthResult, ClientInfo } from './Message';
export type { DaemonConnection, CommandProcessor, EventProcessor, CommandDefinition, ParameterDefinition, EventResult } from './Daemon';
export type { CommandResult } from './Daemon'; // Use Daemon's CommandResult as primary