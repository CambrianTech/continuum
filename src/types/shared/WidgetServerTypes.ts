/**
 * Widget Server Integration Types - Strong typing for widget ↔ server communication
 * Shared between browser widgets and server-side command handlers
 */

// Base event structure for all widget-server communication
export interface BaseWidgetServerEvent {
  readonly timestamp: number;
  readonly sessionId?: string;
  readonly widgetId: string;
}

// Session Events
export interface SessionCreatedEvent extends BaseWidgetServerEvent {
  readonly type: 'session:created';
  readonly sessionType: 'development' | 'production' | 'test';
  readonly owner: string;
  readonly capabilities: readonly string[];
}

export interface SessionJoinedEvent extends BaseWidgetServerEvent {
  readonly type: 'session:joined';
  readonly sessionType: 'development' | 'production' | 'test';
  readonly owner: string;
  readonly joinedBy: string;
  readonly userCount: number;
}

// Health Events
export interface HealthStatus {
  readonly overall: 'healthy' | 'degraded' | 'unhealthy';
  readonly score: number;
  readonly components: readonly {
    readonly name: string;
    readonly status: 'healthy' | 'degraded' | 'unhealthy';
    readonly lastCheck: string;
    readonly details?: string;
  }[];
  readonly issues: readonly {
    readonly severity: 'info' | 'warn' | 'error' | 'critical';
    readonly component: string;
    readonly message: string;
    readonly timestamp: string;
  }[];
}

export interface HealthUpdatedEvent extends BaseWidgetServerEvent {
  readonly type: 'health:updated';
  readonly health: HealthStatus;
  readonly previousHealth?: HealthStatus;
  readonly changedComponents: readonly string[];
}

// Data Events
export type DataSourceType = 
  | 'personas' 
  | 'projects' 
  | 'sessions' 
  | 'health'
  | 'commands'
  | 'daemons'
  | 'widgets'
  | 'logs'
  | 'metrics';

export interface DataUpdatedEvent extends BaseWidgetServerEvent {
  readonly type: 'data:updated';
  readonly dataSource: DataSourceType;
  readonly updateType: 'created' | 'updated' | 'deleted' | 'bulk_update';
  readonly affectedItems: readonly string[];
  readonly metadata?: Record<string, unknown>;
}

// Widget Request Types
export interface WidgetDataRequest {
  readonly dataSource: DataSourceType;
  readonly params?: Record<string, unknown>;
  readonly filters?: {
    readonly limit?: number;
    readonly offset?: number;
    readonly sortBy?: string;
    readonly sortOrder?: 'asc' | 'desc';
    readonly search?: string;
  };
}

export interface WidgetCommandRequest {
  readonly command: string;
  readonly params?: Record<string, unknown>;
  readonly timeout?: number;
  readonly priority?: 'low' | 'normal' | 'high';
}

// Widget Response Types
export interface WidgetDataResponse<T = unknown> {
  readonly success: boolean;
  readonly dataSource: DataSourceType;
  readonly data?: T;
  readonly error?: string;
  readonly timestamp: number;
  readonly metadata?: {
    readonly totalCount?: number;
    readonly hasMore?: boolean;
    readonly fromCache?: boolean;
  };
}

export interface WidgetCommandResponse<T = unknown> {
  readonly success: boolean;
  readonly command: string;
  readonly result?: T;
  readonly error?: string;
  readonly timestamp: number;
  readonly executionTime?: number;
}

// Server Control Event Types
export interface WidgetServerControlEvent extends BaseWidgetServerEvent {
  readonly eventType: 'screenshot' | 'refresh' | 'export' | 'validate' | 'fetch-data' | 'execute-command';
  readonly data: unknown;
}

export interface WidgetScreenshotEvent extends WidgetServerControlEvent {
  readonly eventType: 'screenshot';
  readonly data: {
    readonly target?: 'widget' | 'page' | 'element';
    readonly selector?: string;
    readonly includeContext?: boolean;
    readonly filename?: string;
    readonly format?: 'png' | 'jpg' | 'webp';
  };
}

export interface WidgetRefreshEvent extends WidgetServerControlEvent {
  readonly eventType: 'refresh';
  readonly data: {
    readonly preserveState?: boolean;
    readonly refreshType?: 'soft' | 'hard';
    readonly clearCache?: boolean;
  };
}

export interface WidgetExportEvent extends WidgetServerControlEvent {
  readonly eventType: 'export';
  readonly data: {
    readonly format: 'json' | 'csv' | 'xml' | 'yaml';
    readonly includeMetadata?: boolean;
    readonly filename?: string;
  };
}

export interface WidgetValidateEvent extends WidgetServerControlEvent {
  readonly eventType: 'validate';
  readonly data: {
    readonly validateAssets?: boolean;
    readonly validateContent?: boolean;
    readonly validateAPI?: boolean;
    readonly strictMode?: boolean;
  };
}

export interface WidgetFetchDataEvent extends WidgetServerControlEvent {
  readonly eventType: 'fetch-data';
  readonly data: WidgetDataRequest;
}

export interface WidgetExecuteCommandEvent extends WidgetServerControlEvent {
  readonly eventType: 'execute-command';
  readonly data: WidgetCommandRequest;
}

// Server event names for type safety
export type ServerEventName = 
  | 'session:created'
  | 'session:joined' 
  | 'health:updated'
  | 'data:updated';

// Widget control event names for type safety
export type WidgetControlEventName =
  | 'widget:screenshot'
  | 'widget:refresh' 
  | 'widget:export'
  | 'widget:validate'
  | 'widget:fetch-data'
  | 'widget:execute-command';

// Widget event names for type safety  
export type WidgetEventName =
  | 'server:session-created'
  | 'server:session-joined'
  | 'server:health-updated'
  | 'server:data-updated';

// Union types for type safety
export type ServerEvent = SessionCreatedEvent | SessionJoinedEvent | HealthUpdatedEvent | DataUpdatedEvent;

export type WidgetControlEvent = 
  | WidgetScreenshotEvent 
  | WidgetRefreshEvent 
  | WidgetExportEvent 
  | WidgetValidateEvent
  | WidgetFetchDataEvent
  | WidgetExecuteCommandEvent;

// Event listener function types
export type ServerEventListener<T extends ServerEvent = ServerEvent> = (event: T) => void | Promise<void>;
export type WidgetEventListener<T extends WidgetControlEvent = WidgetControlEvent> = (event: T) => void | Promise<void>;

// Widget capabilities interface
export interface WidgetCapabilities {
  readonly canFetchData: readonly DataSourceType[];
  readonly canExecuteCommands: readonly string[];
  readonly respondsToEvents: readonly ServerEvent['type'][];
  readonly supportsExport: readonly ('json' | 'csv' | 'xml' | 'yaml')[];
  readonly requiresAuth: boolean;
  readonly updateFrequency?: 'realtime' | 'periodic' | 'manual';
}

// Widget registration interface
export interface WidgetRegistration {
  readonly widgetId: string;
  readonly widgetName: string;
  readonly version: string;
  readonly capabilities: WidgetCapabilities;
  readonly dependencies?: readonly string[];
  readonly metadata?: Record<string, unknown>;
}

// Command data source mapping - commands declare what data they provide
export interface CommandDataSourceMapping {
  readonly command: string;
  readonly dataSource: DataSourceType;
  readonly description?: string;
  readonly aliases?: readonly string[];
}

// Event mapping configuration - dynamic event setup
export interface EventMappingConfig {
  readonly serverEvent: ServerEventName;
  readonly widgetEvent: WidgetEventName;
  readonly enabled: boolean;
}