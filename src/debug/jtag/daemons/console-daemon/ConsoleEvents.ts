/**
 * Console Event Definitions - Modular event category
 */

export const ConsoleEvents = {
  ATTACHED: 'console.attached',
  QUEUE_DRAIN_START: 'console.queue-drain-start',
  QUEUE_DRAIN_COMPLETE: 'console.queue-drain-complete',
  MESSAGE_PROCESSED: 'console.message-processed'
} as const;

export interface ConsoleEventData {
  [ConsoleEvents.ATTACHED]: {
    environment: 'browser' | 'server';
    interceptedMethods: string[];
  };
  
  [ConsoleEvents.QUEUE_DRAIN_START]: {
    queueSize: number;
    environment: 'browser' | 'server';
  };
  
  [ConsoleEvents.QUEUE_DRAIN_COMPLETE]: {
    processedCount: number;
    failedCount: number;
    duration: number;
  };
  
  [ConsoleEvents.MESSAGE_PROCESSED]: {
    level: 'log' | 'info' | 'warn' | 'error' | 'debug';
    component: string;
    messageLength: number;
  };
}

export type ConsoleEventName = typeof ConsoleEvents[keyof typeof ConsoleEvents];