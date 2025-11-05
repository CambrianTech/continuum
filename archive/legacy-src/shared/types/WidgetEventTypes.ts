/**
 * Widget Event System - Strongly Typed Events
 * 
 * SHARED between browser and server - import from here only!
 * ALL widget events must use these types - no magic strings!
 * The linter will catch any mismatched event names or payloads.
 */

// Widget Event Names - Exhaustive enum, no magic strings allowed
export enum WidgetEventType {
  // Lifecycle events
  WIDGET_DISCOVERED = 'widget:discovered',
  WIDGET_REGISTERED = 'widget:registered',
  WIDGET_LOADED = 'widget:loaded',
  WIDGET_UNLOADED = 'widget:unloaded',
  WIDGET_ERROR = 'widget:error',
  
  // System events
  WIDGET_SYSTEM_READY = 'widget_system:ready',
  WIDGET_DISCOVERY_COMPLETE = 'widget_system:discovery_complete',
  WIDGET_COMPILATION_COMPLETE = 'widget_system:compilation_complete',
  
  // Asset events
  WIDGET_ASSET_LOADED = 'widget_asset:loaded',
  WIDGET_ASSET_FAILED = 'widget_asset:failed',
  
  // Health events
  WIDGET_HEALTH_CHECK = 'widget_health:check',
  WIDGET_HEALTH_REPORT = 'widget_health:report'
}

// Widget Status - No magic strings
export enum WidgetStatus {
  DISCOVERED = 'discovered',
  LOADING = 'loading',
  READY = 'ready',
  ERROR = 'error',
  UNLOADED = 'unloaded'
}

// Widget Asset Types - Strongly typed
export enum WidgetAssetType {
  CSS = 'css',
  JAVASCRIPT = 'javascript',
  HTML = 'html',
  IMAGE = 'image',
  CONFIG = 'config'
}

// Base event interface - All widget events extend this
export interface BaseWidgetEvent {
  type: WidgetEventType;
  timestamp: string;
  source: 'browser' | 'server' | 'daemon';
}

// Specific event payloads - Type-safe and validated
export interface WidgetDiscoveredEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_DISCOVERED;
  payload: {
    widgetId: string;
    widgetType: string;
    element: HTMLElement | string; // HTMLElement in browser, selector string on server
    manifest?: WidgetManifest;
  };
}

export interface WidgetRegisteredEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_REGISTERED;
  payload: {
    widgetId: string;
    widgetName: string;
    status: WidgetStatus;
    config: WidgetConfig;
  };
}

export interface WidgetLoadedEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_LOADED;
  payload: {
    widgetId: string;
    widgetName: string;
    loadTime: number;
    assets: LoadedAsset[];
  };
}

export interface WidgetErrorEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_ERROR;
  payload: {
    widgetId: string;
    widgetName?: string;
    error: string;
    errorType: 'compilation' | 'runtime' | 'asset' | 'validation';
    stack?: string;
  };
}

export interface WidgetSystemReadyEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_SYSTEM_READY;
  payload: {
    totalWidgets: number;
    loadedWidgets: number;
    failedWidgets: number;
    readyTime: number;
  };
}

export interface WidgetHealthReportEvent extends BaseWidgetEvent {
  type: WidgetEventType.WIDGET_HEALTH_REPORT;
  payload: {
    widgetId: string;
    widgetName: string;
    status: WidgetStatus;
    health: {
      isResponsive: boolean;
      errorCount: number;
      lastError?: string;
      memoryUsage?: number;
    };
  };
}

// Union type of all widget events - For type-safe event handling
export type WidgetEvent = 
  | WidgetDiscoveredEvent
  | WidgetRegisteredEvent  
  | WidgetLoadedEvent
  | WidgetErrorEvent
  | WidgetSystemReadyEvent
  | WidgetHealthReportEvent;

// Widget configuration types
export interface WidgetConfig {
  readonly name: string;
  readonly version: string;
  readonly type: 'ui' | 'data' | 'control' | 'display';
  readonly dependencies?: string[];
  readonly assets?: WidgetAssetConfig[];
  readonly features?: string[];
}

export interface WidgetAssetConfig {
  readonly type: WidgetAssetType;
  readonly path: string;
  readonly required: boolean;
  readonly async?: boolean;
}

export interface WidgetManifest {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly config: WidgetConfig;
  readonly discovered: string; // ISO timestamp
}

export interface LoadedAsset {
  readonly type: WidgetAssetType;
  readonly path: string;
  readonly size: number;
  readonly loadTime: number;
  readonly success: boolean;
  readonly error?: string;
}

// Event emitter interface - Type-safe event emission and listening
export interface WidgetEventEmitter {
  emit<T extends WidgetEvent>(event: T): void;
  on<T extends WidgetEventType>(
    eventType: T, 
    listener: (event: Extract<WidgetEvent, { type: T }>) => void
  ): void;
  off<T extends WidgetEventType>(
    eventType: T,
    listener: (event: Extract<WidgetEvent, { type: T }>) => void
  ): void;
}

// Type guard functions - Runtime type checking
export function isWidgetEvent(obj: any): obj is WidgetEvent {
  return obj && typeof obj === 'object' && 
         Object.values(WidgetEventType).includes(obj.type) &&
         typeof obj.timestamp === 'string' &&
         ['browser', 'server', 'daemon'].includes(obj.source);
}

export function isWidgetErrorEvent(event: WidgetEvent): event is WidgetErrorEvent {
  return event.type === WidgetEventType.WIDGET_ERROR;
}

export function isWidgetSystemReadyEvent(event: WidgetEvent): event is WidgetSystemReadyEvent {
  return event.type === WidgetEventType.WIDGET_SYSTEM_READY;
}

// Helper functions for creating type-safe events
export function createWidgetEvent<T extends WidgetEvent>(
  type: T['type'],
  payload: T['payload'],
  source: 'browser' | 'server' | 'daemon' = 'daemon'
): T {
  return {
    type,
    payload,
    timestamp: new Date().toISOString(),
    source
  } as T;
}

// Validation schema (for runtime validation if needed)
export const WIDGET_EVENT_SCHEMA = {
  requiredFields: ['type', 'timestamp', 'source', 'payload'],
  validSources: ['browser', 'server', 'daemon'] as const,
  validTypes: Object.values(WidgetEventType)
} as const;