/**
 * Command Event Definitions - Co-located with Command daemon
 */

export const COMMAND_EVENTS = {
  REGISTERED: 'command.registered',
  EXECUTED: 'command.executed',
  FAILED: 'command.failed'
} as const;

export interface CommandEventData {
  [COMMAND_EVENTS.REGISTERED]: {
    commandName: string;
    commandType: string;
    endpoint: string;
  };
  
  [COMMAND_EVENTS.EXECUTED]: {
    commandName: string;
    duration: number;
    success: boolean;
    resultType: string;
  };
  
  [COMMAND_EVENTS.FAILED]: {
    commandName: string;
    error: string;
    parameters?: unknown; // Keep optional but use unknown instead of any
  };
}

export type CommandEventName = typeof COMMAND_EVENTS[keyof typeof COMMAND_EVENTS];