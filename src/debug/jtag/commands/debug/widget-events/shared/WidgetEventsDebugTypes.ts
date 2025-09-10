/**
 * Widget Events Debug Command Types
 * 
 * Specialized command for debugging widget event listeners and event system connectivity
 * Replaces raw exec commands for widget event debugging
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface WidgetEventsDebugParams extends CommandParams {
  widgetSelector?: string;
  eventName?: string;
  includeHandlers?: boolean;
  testServerEvents?: boolean;
  roomId?: string;
}

export interface EventHandlerInfo {
  eventName: string;
  handlerCount: number;
  hasDispatcher: boolean;
  listenerType: 'widget' | 'dom' | 'both';
}

export interface WidgetEventsDebugResult extends CommandResult {
  success: boolean;
  widgetFound: boolean;
  widgetPath: string;
  widgetMethods: string[];
  
  // Event System Status
  eventSystem: {
    hasEventEmitter: boolean;
    eventEmitterSize: number;
    eventTypes: string[];
    dispatcherTypes: string[];
    domListeners: string[];
  };
  
  // Event Handler Analysis
  eventHandlers: EventHandlerInfo[];
  
  // Connectivity Tests
  connectivity: {
    serverEventsWorking: boolean;
    domEventsWorking: boolean;
    dispatcherWorking: boolean;
  };
  
  // Debugging Info
  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };
  
  error?: string;
}

export interface EventTestResult {
  eventName: string;
  serverEventReceived: boolean;
  handlersCalled: number;
  handlerErrors: string[];
  timingMs: number;
}

export const createWidgetEventsDebugResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<WidgetEventsDebugResult>, 'context' | 'sessionId'>
): WidgetEventsDebugResult => createPayload(context, sessionId, {
  success: false,
  widgetFound: false,
  widgetPath: '',
  widgetMethods: [],
  eventSystem: {
    hasEventEmitter: false,
    eventEmitterSize: 0,
    eventTypes: [],
    dispatcherTypes: [],
    domListeners: []
  },
  eventHandlers: [],
  connectivity: {
    serverEventsWorking: false,
    domEventsWorking: false,
    dispatcherWorking: false
  },
  debugging: {
    logs: [],
    warnings: [],
    errors: []
  },
  ...data
});