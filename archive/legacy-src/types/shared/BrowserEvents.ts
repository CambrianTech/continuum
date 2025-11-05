/**
 * Browser Events - Strongly Typed Events
 * 
 * SHARED between browser and server - import from here only!
 * ALL browser events must use these types - no magic strings!
 * Used by BrowserManagerDaemon, browser adapters, and client-side code.
 */

// Browser Event Names - Exhaustive enum
export enum BrowserEventType {
  // Browser lifecycle
  BROWSER_LAUNCH = 'browser:launch',
  BROWSER_CLOSE = 'browser:close',
  BROWSER_FOCUS = 'browser:focus',
  BROWSER_REFRESH = 'browser:refresh',
  BROWSER_NAVIGATE = 'browser:navigate',
  
  // Tab management
  TAB_CREATE = 'browser:tab_create',
  TAB_CLOSE = 'browser:tab_close',
  TAB_FOCUS = 'browser:tab_focus',
  TAB_REFRESH = 'browser:tab_refresh',
  TAB_COUNT = 'browser:tab_count',
  
  // Browser health
  BROWSER_READY = 'browser:ready',
  BROWSER_ERROR = 'browser:error',
  BROWSER_HEALTH_CHECK = 'browser:health_check',
  
  // Session integration
  BROWSER_SESSION_ATTACH = 'browser:session_attach',
  BROWSER_SESSION_DETACH = 'browser:session_detach'
}

// Browser Action Types - What the browser can do
export enum BrowserActionType {
  LAUNCH = 'launch',
  FOCUS = 'focus', 
  REFRESH = 'refresh',
  CLOSE = 'close',
  NAVIGATE = 'navigate',
  COUNT_TABS = 'count_tabs',
  CLOSE_TABS = 'close_tabs'
}

// Base browser event interface
export interface BaseBrowserEvent {
  type: BrowserEventType;
  timestamp: string;
  source: 'browser' | 'server' | 'daemon' | 'adapter';
  sessionId?: string;
}

// Browser refresh event - Used for tab refresh functionality
export interface BrowserRefreshEvent extends BaseBrowserEvent {
  type: BrowserEventType.BROWSER_REFRESH | BrowserEventType.TAB_REFRESH;
  payload: {
    url: string;
    sessionId?: string;
    focus?: boolean;
    reason?: 'user_request' | 'session_join' | 'auto_refresh' | 'npm_start';
  };
}

// Browser launch event
export interface BrowserLaunchEvent extends BaseBrowserEvent {
  type: BrowserEventType.BROWSER_LAUNCH;
  payload: {
    url: string;
    sessionId?: string;
    browserType?: string;
    focus?: boolean;
    killZombies?: boolean;
  };
}

// Browser focus event
export interface BrowserFocusEvent extends BaseBrowserEvent {
  type: BrowserEventType.BROWSER_FOCUS;
  payload: {
    url: string;
    sessionId?: string;
  };
}

// Tab count event
export interface BrowserTabCountEvent extends BaseBrowserEvent {
  type: BrowserEventType.TAB_COUNT;
  payload: {
    url: string;
    count: number;
    sessionId?: string;
  };
}

// Browser error event
export interface BrowserErrorEvent extends BaseBrowserEvent {
  type: BrowserEventType.BROWSER_ERROR;
  payload: {
    error: string;
    action?: BrowserActionType;
    url?: string;
    sessionId?: string;
    stack?: string;
  };
}

// Browser ready event
export interface BrowserReadyEvent extends BaseBrowserEvent {
  type: BrowserEventType.BROWSER_READY;
  payload: {
    url: string;
    sessionId?: string;
    browserType: string;
    pid?: number;
  };
}

// Union type of all browser events
export type BrowserEvent = 
  | BrowserRefreshEvent
  | BrowserLaunchEvent
  | BrowserFocusEvent
  | BrowserTabCountEvent
  | BrowserErrorEvent
  | BrowserReadyEvent;

// Browser adapter result types - For strong typing of adapter responses
export interface BrowserActionResult {
  success: boolean;
  action: BrowserActionType;
  url: string;
  details?: {
    count?: number;
    focused?: boolean;
    refreshed?: boolean;
    error?: string;
  } | undefined;
  timestamp: string;
}

// Type guard functions
export function isBrowserEvent(obj: any): obj is BrowserEvent {
  return obj && typeof obj === 'object' && 
         Object.values(BrowserEventType).includes(obj.type) &&
         typeof obj.timestamp === 'string' &&
         ['browser', 'server', 'daemon', 'adapter'].includes(obj.source);
}

export function isBrowserRefreshEvent(event: BrowserEvent): event is BrowserRefreshEvent {
  return event.type === BrowserEventType.BROWSER_REFRESH || 
         event.type === BrowserEventType.TAB_REFRESH;
}

export function isBrowserErrorEvent(event: BrowserEvent): event is BrowserErrorEvent {
  return event.type === BrowserEventType.BROWSER_ERROR;
}

// Helper function for creating type-safe browser events
export function createBrowserEvent<T extends BrowserEvent>(
  type: T['type'],
  payload: T['payload'],
  source: 'browser' | 'server' | 'daemon' | 'adapter' = 'daemon',
  sessionId?: string
): T {
  return {
    type,
    payload,
    timestamp: new Date().toISOString(),
    source,
    sessionId
  } as T;
}

// Helper function for creating browser action results
export function createBrowserActionResult(
  action: BrowserActionType,
  url: string,
  success: boolean,
  details?: BrowserActionResult['details']
): BrowserActionResult {
  return {
    success,
    action,
    url,
    details,
    timestamp: new Date().toISOString()
  };
}

// Validation constants
export const BROWSER_EVENT_SCHEMA = {
  requiredFields: ['type', 'timestamp', 'source', 'payload'],
  validSources: ['browser', 'server', 'daemon', 'adapter'] as const,
  validTypes: Object.values(BrowserEventType),
  validActions: Object.values(BrowserActionType)
} as const;