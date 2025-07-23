/**
 * Command Event Definitions - Co-located with Command daemon
 */

export const CommandEvents = {
  REGISTERED: 'command.registered',
  EXECUTED: 'command.executed',
  FAILED: 'command.failed'
} as const;

export interface CommandEventData {
  [CommandEvents.REGISTERED]: {
    commandName: string;
    commandType: string;
    endpoint: string;
  };
  
  [CommandEvents.EXECUTED]: {
    commandName: string;
    duration: number;
    success: boolean;
    resultType: string;
  };
  
  [CommandEvents.FAILED]: {
    commandName: string;
    error: string;
    parameters?: unknown; // Keep optional but use unknown instead of any
  };
}

export type CommandEventName = typeof CommandEvents[keyof typeof CommandEvents];