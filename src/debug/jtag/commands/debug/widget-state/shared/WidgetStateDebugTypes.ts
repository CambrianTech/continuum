/**
 * Widget State Debug Command Types
 *
 * Addresses DEBUG-FRICTION.md critical gap: Widget introspection during development
 * Enables inspection of widget internal state, event listeners, and DOM structure
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface WidgetStateDebugParams extends CommandParams {
  widgetSelector?: string;
  includeMessages?: boolean;
  testDataConnectivity?: boolean;
  roomId?: string;
  includeEventListeners?: boolean;
  includeDomInfo?: boolean;
  includeDimensions?: boolean;
  extractRowData?: boolean;
  rowSelector?: string;
  countOnly?: boolean; // Return only entityCount, no heavy data
}

export interface WidgetStateDebugResult extends CommandResult {
  success: boolean;
  widgetFound: boolean;
  widgetPath: string;
  widgetType: string;
  methods: string[];

  // Widget internal state
  state: {
    properties: Record<string, unknown>;
    messageCount?: number;
    currentRoomId?: string;
    entityCount?: number;
    entities?: Array<{ id: string; name?: string; displayName?: string }>;
    entityIds?: string[]; // Just IDs for test verification (lightweight)
  };

  messages: Array<{ id: string; content?: Record<string, unknown>; [key: string]: unknown }>;

  // Event system information
  eventSystem?: {
    hasEventEmitter: boolean;
    eventEmitterSize: number;
    eventTypes: string[];
    eventListeners: Array<{
      event: string;
      handlerCount: number;
    }>;
  };

  // DOM structure information
  domInfo?: {
    elementCount: number;
    visibleText: string;
    cssClasses: string[];
    hasShadowRoot: boolean;
  };

  // Widget dimensions and computed styles
  dimensions?: {
    width: number;
    height: number;
    scrollHeight: number;
    clientHeight: number;
    hasScrollbar: boolean;
    computedHeight: string;
    computedDisplay: string;
    computedFlex: string;
    computedOverflow: string;
  };

  // Row data extraction (e.g., room items, user items)
  rowData?: Array<{
    index: number;
    id: string;
    textContent: string;
    attributes: Record<string, string>;
    classes: string[];
  }>;

  dataTest?: {
    rawData: Record<string, unknown>;
    filteredData: Record<string, unknown>;
    filterResults: string[];
  };

  connectivity: {
    hasJtagOperation: boolean;
    windowJtag: boolean;
    windowWidgetDaemon: boolean;
    canExecuteCommands: boolean;
  };

  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };

  error?: string;
}

export const createWidgetStateDebugResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<WidgetStateDebugResult>, 'context' | 'sessionId'>
): WidgetStateDebugResult => createPayload(context, sessionId, {
  success: false,
  widgetFound: false,
  widgetPath: '',
  widgetType: 'unknown',
  methods: [],
  state: {
    properties: {},
  },
  messages: [],
  connectivity: {
    hasJtagOperation: false,
    windowJtag: false,
    windowWidgetDaemon: false,
    canExecuteCommands: false,
  },
  debugging: {
    logs: [],
    warnings: [],
    errors: [],
  },
  ...data
});