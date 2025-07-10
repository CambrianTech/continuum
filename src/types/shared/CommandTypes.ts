/**
 * Shared Command Types - Strongly Typed Command System
 * 
 * These interfaces are used by both:
 * - Server-side command processors and daemons
 * - Browser client command execution and validation
 * 
 * This ensures perfect type alignment and prevents command execution errors.
 */

// import { CommandEvent, WidgetEvent } from './EventTypes';

/**
 * Base command interface - all commands must extend this
 */
export interface BaseCommand {
  type: string;
  timestamp: string;
  source: 'client' | 'server' | 'system';
  requestId?: string;
}

/**
 * Command execution result - standardized response format
 */
export interface CommandResult<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  timestamp: string;
  executionTime?: number;
  warnings?: string[];
}

/**
 * Widget Commands - All widget-related operations
 */
export enum WidgetCommandType {
  DISCOVER = 'widget:discover',
  LIST = 'widget:list',
  GET_DEFINITION = 'widget:get_definition',
  REGISTER = 'widget:register',
  UNREGISTER = 'widget:unregister',
  LOAD = 'widget:load',
  UNLOAD = 'widget:unload',
  GET_STATE = 'widget:get_state',
  SET_STATE = 'widget:set_state',
  EXECUTE_ACTION = 'widget:execute_action'
}

/**
 * System Commands - Core system operations
 */
export enum SystemCommandType {
  HEALTH = 'system:health',
  STATUS = 'system:status',
  VERSION = 'system:version',
  SHUTDOWN = 'system:shutdown',
  RESTART = 'system:restart'
}

/**
 * Session Commands - Session management operations
 */
export enum SessionCommandType {
  CONNECT = 'session:connect',
  DISCONNECT = 'session:disconnect',
  GET_INFO = 'session:get_info',
  LIST_SESSIONS = 'session:list_sessions'
}

/**
 * Union type of all command types for validation
 */
export type AllCommandTypes = 
  | WidgetCommandType
  | SystemCommandType  
  | SessionCommandType;

/**
 * Widget Discovery Command - Find available widgets
 */
export interface WidgetDiscoverCommand extends BaseCommand {
  type: WidgetCommandType.DISCOVER;
  payload?: {
    filter?: {
      category?: string;
      status?: 'active' | 'inactive' | 'all';
      capabilities?: string[];
    };
    includeMetadata?: boolean;
  };
}

/**
 * Widget Discovery Result - List of discovered widgets
 */
export interface WidgetDiscoverResult {
  widgets: WidgetDefinition[];
  totalCount: number;
  categories: string[];
  metadata?: {
    discoveryTime: string;
    source: string;
    version: string;
  };
}

/**
 * Widget Definition - Complete widget specification
 */
export interface WidgetDefinition {
  id: string;
  name: string;
  version: string;
  category: string;
  description?: string;
  capabilities: WidgetCapability[];
  assets: {
    css?: string[];
    html?: string[];
    js?: string[];
  };
  config?: {
    defaultSize?: { width: number; height: number; };
    resizable?: boolean;
    draggable?: boolean;
    permissions?: string[];
  };
  status: 'active' | 'inactive' | 'error';
  metadata: {
    author?: string;
    license?: string;
    repository?: string;
    lastModified: string;
    fileSize?: number;
  };
}

/**
 * Widget Capabilities - What a widget can do
 */
export interface WidgetCapability {
  name: string;
  type: 'action' | 'event' | 'data' | 'ui';
  description?: string;
  parameters?: WidgetParameter[];
  returnType?: string;
}

/**
 * Widget Parameters - Strongly typed parameter definitions
 */
export interface WidgetParameter {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'object' | 'array';
  required: boolean;
  description?: string;
  defaultValue?: any;
  validation?: {
    min?: number;
    max?: number;
    pattern?: string;
    enum?: any[];
  };
}

/**
 * Widget List Command - Get list of widgets with optional filtering
 */
export interface WidgetListCommand extends BaseCommand {
  type: WidgetCommandType.LIST;
  payload?: {
    category?: string;
    status?: 'active' | 'inactive' | 'all';
    limit?: number;
    offset?: number;
  };
}

/**
 * Widget Definition Command - Get detailed widget information
 */
export interface WidgetGetDefinitionCommand extends BaseCommand {
  type: WidgetCommandType.GET_DEFINITION;
  payload: {
    widgetId: string;
    includeAssets?: boolean;
    includeMetadata?: boolean;
  };
}

/**
 * System Health Command - Check system status
 */
export interface SystemHealthCommand extends BaseCommand {
  type: SystemCommandType.HEALTH;
  payload?: {
    detailed?: boolean;
    components?: string[];
  };
}

/**
 * System Health Result - System status information
 */
export interface SystemHealthResult {
  status: 'healthy' | 'warning' | 'error';
  timestamp: string;
  uptime: number;
  components: {
    [componentName: string]: {
      status: 'healthy' | 'warning' | 'error';
      message?: string;
      lastCheck: string;
      metrics?: Record<string, any>;
    };
  };
  resources: {
    memory: {
      used: number;
      total: number;
      percentage: number;
    };
    cpu: {
      usage: number;
      processes: number;
    };
  };
}

/**
 * Session Connect Command - Connect to a session
 */
export interface SessionConnectCommand extends BaseCommand {
  type: SessionCommandType.CONNECT;
  payload: {
    sessionType: 'development' | 'production' | 'test';
    owner: string;
    options?: {
      focus?: boolean;
      killZombies?: boolean;
      forceNew?: boolean;
    };
  };
}

/**
 * Command Factory Functions - Type-safe command creation
 */
export const CommandFactory = {
  /**
   * Create a widget discovery command
   */
  widgetDiscover: (
    filter?: { category?: string; status?: 'active' | 'inactive' | 'all'; capabilities?: string[]; },
    includeMetadata = true
  ): WidgetDiscoverCommand => ({
    type: WidgetCommandType.DISCOVER,
    timestamp: new Date().toISOString(),
    source: 'client',
    payload: {
      ...(filter && { filter }),
      includeMetadata
    }
  }),

  /**
   * Create a widget list command
   */
  widgetList: (
    category?: string,
    status: 'active' | 'inactive' | 'all' = 'all'
  ): WidgetListCommand => ({
    type: WidgetCommandType.LIST,
    timestamp: new Date().toISOString(),
    source: 'client',
    payload: {
      ...(category && { category }),
      status
    }
  }),

  /**
   * Create a widget definition command
   */
  widgetGetDefinition: (
    widgetId: string,
    includeAssets = true,
    includeMetadata = true
  ): WidgetGetDefinitionCommand => ({
    type: WidgetCommandType.GET_DEFINITION,
    timestamp: new Date().toISOString(),
    source: 'client',
    payload: {
      widgetId,
      includeAssets,
      includeMetadata
    }
  }),

  /**
   * Create a system health command
   */
  systemHealth: (
    detailed = false,
    components?: string[]
  ): SystemHealthCommand => ({
    type: SystemCommandType.HEALTH,
    timestamp: new Date().toISOString(),
    source: 'client',
    payload: {
      detailed,
      ...(components && { components })
    }
  }),

  /**
   * Create a session connect command
   */
  sessionConnect: (
    sessionType: 'development' | 'production' | 'test',
    owner: string,
    options?: SessionConnectCommand['payload']['options']
  ): SessionConnectCommand => ({
    type: SessionCommandType.CONNECT,
    timestamp: new Date().toISOString(),
    source: 'client',
    payload: {
      sessionType,
      owner,
      ...(options && { options })
    }
  })
} as const;

/**
 * Command Type Guards - Runtime type checking
 */
export function isWidgetDiscoverCommand(command: BaseCommand): command is WidgetDiscoverCommand {
  return command.type === WidgetCommandType.DISCOVER;
}

export function isWidgetListCommand(command: BaseCommand): command is WidgetListCommand {
  return command.type === WidgetCommandType.LIST;
}

export function isWidgetGetDefinitionCommand(command: BaseCommand): command is WidgetGetDefinitionCommand {
  return command.type === WidgetCommandType.GET_DEFINITION;
}

export function isSystemHealthCommand(command: BaseCommand): command is SystemHealthCommand {
  return command.type === SystemCommandType.HEALTH;
}

export function isSessionConnectCommand(command: BaseCommand): command is SessionConnectCommand {
  return command.type === SessionCommandType.CONNECT;
}

/**
 * Command Selectors - Compiler-enforced command selection
 */
export const CommandSelectors = {
  /**
   * Select widget commands
   */
  widget: {
    discover: () => WidgetCommandType.DISCOVER,
    list: () => WidgetCommandType.LIST,
    getDefinition: () => WidgetCommandType.GET_DEFINITION,
    register: () => WidgetCommandType.REGISTER,
    unregister: () => WidgetCommandType.UNREGISTER
  },
  
  /**
   * Select system commands
   */
  system: {
    health: () => SystemCommandType.HEALTH,
    status: () => SystemCommandType.STATUS,
    version: () => SystemCommandType.VERSION
  },
  
  /**
   * Select session commands
   */
  session: {
    connect: () => SessionCommandType.CONNECT,
    disconnect: () => SessionCommandType.DISCONNECT,
    getInfo: () => SessionCommandType.GET_INFO
  }
} as const;

/**
 * Command Validation - Ensure commands match expected structure
 */
export function validateCommand(command: any): command is BaseCommand {
  return (
    typeof command === 'object' &&
    command !== null &&
    typeof command.type === 'string' &&
    typeof command.timestamp === 'string' &&
    ['client', 'server', 'system'].includes(command.source)
  );
}

/**
 * Result Factory - Type-safe result creation
 */
export const ResultFactory = {
  /**
   * Create a successful result
   */
  success: <T>(data: T, executionTime?: number, warnings?: string[]): CommandResult<T> => ({
    success: true,
    data,
    timestamp: new Date().toISOString(),
    ...(executionTime !== undefined && { executionTime }),
    ...(warnings && { warnings })
  }),

  /**
   * Create an error result
   */
  error: (error: string, executionTime?: number): CommandResult => ({
    success: false,
    error,
    timestamp: new Date().toISOString(),
    ...(executionTime !== undefined && { executionTime })
  })
} as const;