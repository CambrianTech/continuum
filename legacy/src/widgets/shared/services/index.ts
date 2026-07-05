/**
 * Widget Services - Clean Service Layer Exports
 * 
 * Exports all widget services that replace BaseWidget god object functionality.
 * Services provide clean separation of concerns with adapter pattern for JTAG integration.
 */

// Service registry and dependency injection
export * from './WidgetServiceRegistry';

// Individual services  
export * from './data/WidgetDataService';
export * from './events/WidgetEventService';
export * from './resources/WidgetResourceService';
export * from './ai/WidgetAIService';

// Re-export main interfaces for convenience
export type {
  IWidgetService,
  IWidgetServiceRegistry,
  WidgetServiceContext,
  WidgetServiceName
} from './WidgetServiceRegistry';

export type {
  IWidgetDataService,
  DataStoreOptions
} from './data/WidgetDataService';

export type {
  IWidgetEventService,
  BroadcastOptions,
  EventHandler
} from './events/WidgetEventService';

export type {
  IWidgetResourceService,
  ResourceType,
  ScreenshotOptions
} from './resources/WidgetResourceService';

export type {
  IWidgetAIService,
  AIQueryOptions,
  AIQueryResult
} from './ai/WidgetAIService';