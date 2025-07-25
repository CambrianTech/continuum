/**
 * Console Event Definitions - Modular event category
 */

import type { LogLevel } from '@shared/LogLevels';
import type { JTAGContext } from '@shared/JTAGTypes';

export const CONSOLE_EVENTS = {
  ATTACHED: 'console.attached',
  QUEUE_DRAIN_START: 'console.queue-drain-start',
  QUEUE_DRAIN_COMPLETE: 'console.queue-drain-complete',
  MESSAGE_PROCESSED: 'console.message-processed'
} as const;

export interface ConsoleEventData {
  [CONSOLE_EVENTS.ATTACHED]: {
    environment: JTAGContext['environment'];
    interceptedMethods: string[];
  };
  
  [CONSOLE_EVENTS.QUEUE_DRAIN_START]: {
    queueSize: number;
    environment: JTAGContext['environment'];
  };
  
  [CONSOLE_EVENTS.QUEUE_DRAIN_COMPLETE]: {
    processedCount: number;
    failedCount: number;
    duration: number;
  };
  
  [CONSOLE_EVENTS.MESSAGE_PROCESSED]: {
    level: LogLevel;
    component: string;
    messageLength: number;
  };
}

export type ConsoleEventName = typeof CONSOLE_EVENTS[keyof typeof CONSOLE_EVENTS];