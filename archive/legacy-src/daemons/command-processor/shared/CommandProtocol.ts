/**
 * Command Protocol - Universal daemon message contracts for symmetric architecture
 * 
 * Defines the message types and protocols used for communication between
 * command processing daemons across different execution contexts.
 * 
 * Used by:
 * - Server CommandProcessorDaemon (HTTP/IPC routing)
 * - Client CommandProcessorDaemon (WebSocket/DOM routing)
 * - Cross-context command delegation and mesh distribution
 */

import { DaemonMessage, DaemonResponse } from '../../base/DaemonProtocol';
import { TypedCommandRequest, CommandExecution } from './CommandTypes';
import { CareValidation } from './CareValidation';

// ✅ COMMAND EXECUTION MESSAGES
export interface CommandExecuteMessage extends DaemonMessage {
  type: 'command.execute';
  data: TypedCommandRequest;
}

export interface CommandExecuteResponse extends DaemonResponse {
  data: {
    execution: CommandExecution;
    careValidation?: CareValidation;
  };
}

// ✅ COMMAND ROUTING MESSAGES
export interface CommandRouteMessage extends DaemonMessage {
  type: 'command.route';
  data: {
    command: string;
    parameters: unknown;
    preferredProvider?: 'browser' | 'python' | 'cloud' | 'mesh' | 'auto';
    fallbackAllowed?: boolean;
  };
}

export interface CommandRouteResponse extends DaemonResponse {
  data: {
    selectedProvider: 'browser' | 'python' | 'cloud' | 'mesh';
    executionId: string;
    estimatedCost?: number;
  };
}

// ✅ COMMAND STATUS MESSAGES
export interface CommandStatusMessage extends DaemonMessage {
  type: 'command.status';
  data: {
    executionId: string;
  };
}

export interface CommandStatusResponse extends DaemonResponse {
  data: {
    execution: CommandExecution;
    progress?: number; // 0-100
  };
}

// ✅ WEBSOCKET COMMAND MESSAGES
export interface ExecuteCommandMessage extends DaemonMessage {
  type: 'execute_command';
  data: {
    command: string;
    parameters: unknown;
    context?: Record<string, any>;
    sessionId?: string;
  };
}

export interface ExecuteCommandResponse extends DaemonResponse {
  data: {
    result: unknown;
    executionTime: number;
    careValidation?: CareValidation;
  };
}

// ✅ HTTP API MESSAGES
export interface HandleApiMessage extends DaemonMessage {
  type: 'handle_api';
  data: {
    method: 'GET' | 'POST' | 'PUT' | 'DELETE';
    path: string;
    body?: unknown;
    headers?: Record<string, string>;
    query?: Record<string, string>;
  };
}

export interface HandleApiResponse extends DaemonResponse {
  data: {
    statusCode: number;
    body: unknown;
    headers?: Record<string, string>;
  };
}

// ✅ WIDGET API MESSAGES
export interface HandleWidgetApiMessage extends DaemonMessage {
  type: 'handle_widget_api';
  data: {
    widgetId: string;
    action: string;
    parameters: unknown;
    context?: Record<string, any>;
  };
}

export interface HandleWidgetApiResponse extends DaemonResponse {
  data: {
    widgetState: unknown;
    updates?: unknown[];
  };
}

// ✅ COMMAND PROTOCOL MESSAGE UNION TYPES
export type CommandProtocolMessage = 
  | CommandExecuteMessage
  | CommandRouteMessage
  | CommandStatusMessage
  | ExecuteCommandMessage
  | HandleApiMessage
  | HandleWidgetApiMessage;

export type CommandProtocolResponse = 
  | CommandExecuteResponse
  | CommandRouteResponse
  | CommandStatusResponse
  | ExecuteCommandResponse
  | HandleApiResponse
  | HandleWidgetApiResponse;

// ✅ MESSAGE TYPE CONSTANTS
export const COMMAND_MESSAGE_TYPES = {
  EXECUTE: 'command.execute' as const,
  ROUTE: 'command.route' as const,
  STATUS: 'command.status' as const,
  EXECUTE_WEBSOCKET: 'execute_command' as const,
  HANDLE_API: 'handle_api' as const,
  HANDLE_WIDGET_API: 'handle_widget_api' as const
} as const;

// ✅ TYPE GUARDS FOR MESSAGE VALIDATION
export function isCommandExecuteMessage(msg: DaemonMessage): msg is CommandExecuteMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.EXECUTE;
}

export function isCommandRouteMessage(msg: DaemonMessage): msg is CommandRouteMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.ROUTE;
}

export function isCommandStatusMessage(msg: DaemonMessage): msg is CommandStatusMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.STATUS;
}

export function isExecuteCommandMessage(msg: DaemonMessage): msg is ExecuteCommandMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.EXECUTE_WEBSOCKET;
}

export function isHandleApiMessage(msg: DaemonMessage): msg is HandleApiMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.HANDLE_API;
}

export function isHandleWidgetApiMessage(msg: DaemonMessage): msg is HandleWidgetApiMessage {
  return msg.type === COMMAND_MESSAGE_TYPES.HANDLE_WIDGET_API;
}

// ✅ PROTOCOL FACTORY FOR CREATING MESSAGES
export class CommandProtocolFactory {
  static createExecuteMessage(request: TypedCommandRequest): CommandExecuteMessage {
    return {
      id: `msg-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: COMMAND_MESSAGE_TYPES.EXECUTE,
      from: 'command-processor',
      to: 'command-executor',
      data: request,
      timestamp: new Date()
    };
  }

  static createRouteMessage(
    command: string,
    parameters: unknown,
    preferredProvider?: 'browser' | 'python' | 'cloud' | 'mesh' | 'auto'
  ): CommandRouteMessage {
    const data: CommandRouteMessage['data'] = {
      command,
      parameters,
      fallbackAllowed: true
    };
    
    if (preferredProvider) {
      data.preferredProvider = preferredProvider;
    }
    
    return {
      id: `msg-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: COMMAND_MESSAGE_TYPES.ROUTE,
      from: 'command-processor',
      to: 'command-router',
      data,
      timestamp: new Date()
    };
  }

  static createStatusMessage(executionId: string): CommandStatusMessage {
    return {
      id: `msg-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: COMMAND_MESSAGE_TYPES.STATUS,
      from: 'command-processor',
      to: 'command-tracker',
      data: { executionId },
      timestamp: new Date()
    };
  }
}