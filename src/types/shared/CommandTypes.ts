/**
 * Shared Command Types - Strongly Typed Command System
 * 
 * These interfaces are used by both:
 * - Server-side command processors and daemons
 * - Browser client command execution and validation
 * 
 * This ensures perfect type alignment and prevents command execution errors.
 * 
 * BREAKTHROUGH: Strong typing for command categories prevents "Core" vs "core" typos
 * This implementation resulted from systematic middle-out methodology documented in:
 * @see ../../TECHNICAL_DEBT.md - Analysis of brittle patterns discovered during migration
 * @see ../../MIDDLE_OUT_SUCCESS.md - Complete methodology and quantified results
 */

// import { CommandEvent, WidgetEvent } from './EventTypes';

/**
 * Strongly typed command categories - prevents typos like "Core" vs "core"
 */
export const COMMAND_CATEGORIES = {
  CORE: 'core',
  SYSTEM: 'system',
  BROWSER: 'browser',
  FILE: 'file',
  AI: 'ai',
  SESSION: 'session',
  DEVELOPMENT: 'development',
  COMMUNICATION: 'communication',
  MONITORING: 'monitoring',
  TESTING: 'testing',
  UI: 'ui',
  DATABASE: 'database',
  PLANNING: 'planning',
  DOCS: 'docs',
  EVENTS: 'events',
  INPUT: 'input',
  KERNEL: 'kernel',
  PERSONA: 'persona',
  ACADEMY: 'academy',
  OTHER: 'other'
} as const;

/**
 * Command category type - only allows predefined categories
 */
export type CommandCategory = typeof COMMAND_CATEGORIES[keyof typeof COMMAND_CATEGORIES];

/**
 * Parameter types - strongly typed to prevent typos
 */
export type ParameterType = 
  | 'string'
  | 'number'
  | 'boolean'
  | 'array'
  | 'object'
  | 'file'
  | 'url'
  | 'email'
  | 'date'
  | 'json';

/**
 * Parameter validation rules
 */
export interface ParameterValidation {
  readonly min?: number;
  readonly max?: number;
  readonly pattern?: string;
  readonly allowEmpty?: boolean;
  readonly custom?: (value: unknown) => boolean;
}

/**
 * Parameter definition with strong typing
 */
export interface ParameterDefinition {
  readonly type: ParameterType;
  readonly description?: string;
  readonly required?: boolean;
  readonly default?: unknown;
  readonly choices?: readonly string[];
  readonly enum?: readonly string[]; // Legacy compatibility - same as choices
  readonly validation?: ParameterValidation;
}

/**
 * Command definition interface - consistent across all command modules
 */
export interface CommandDefinition {
  readonly name: string;
  readonly category: CommandCategory;
  readonly description: string;
  readonly icon?: string; // Optional icon (emoji or Unicode)
  readonly parameters?: Record<string, ParameterDefinition>;
  readonly examples?: readonly string[] | readonly { description: string; command: string; }[]; // Support both formats
  readonly usage?: string;
  readonly version?: string;
  readonly deprecated?: boolean;
  readonly aliases?: readonly string[];
}

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
 * This replaces ALL duplicate CommandResult interfaces across the codebase
 */
export interface CommandResult<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  timestamp?: string;
  executionTime?: number;
  warnings?: string[];
  processor?: string;
  duration?: number;
  message?: string; // Legacy compatibility - will be deprecated
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

/**
 * Category Normalization - Prevents "Core" vs "core" typos
 */

/**
 * Helper function to validate command category
 */
export function isValidCommandCategory(category: string): category is CommandCategory {
  return Object.values(COMMAND_CATEGORIES).includes(category as CommandCategory);
}

/**
 * Helper function to normalize command category
 * This fixes the "Core" vs "core" typo issue by converting all variations to lowercase
 */
export function normalizeCommandCategory(category: string): CommandCategory {
  const normalized = category.toLowerCase();
  
  // Handle common variations and typos
  const categoryMap: Record<string, CommandCategory> = {
    'core': COMMAND_CATEGORIES.CORE,
    'system': COMMAND_CATEGORIES.SYSTEM,
    'browser': COMMAND_CATEGORIES.BROWSER,
    'file': COMMAND_CATEGORIES.FILE,
    'ai': COMMAND_CATEGORIES.AI,
    'session': COMMAND_CATEGORIES.SESSION,
    'development': COMMAND_CATEGORIES.DEVELOPMENT,
    'dev': COMMAND_CATEGORIES.DEVELOPMENT,
    'communication': COMMAND_CATEGORIES.COMMUNICATION,
    'comm': COMMAND_CATEGORIES.COMMUNICATION,
    'monitoring': COMMAND_CATEGORIES.MONITORING,
    'monitor': COMMAND_CATEGORIES.MONITORING,
    'testing': COMMAND_CATEGORIES.TESTING,
    'test': COMMAND_CATEGORIES.TESTING,
    'ui': COMMAND_CATEGORIES.UI,
    'database': COMMAND_CATEGORIES.DATABASE,
    'db': COMMAND_CATEGORIES.DATABASE,
    'planning': COMMAND_CATEGORIES.PLANNING,
    'docs': COMMAND_CATEGORIES.DOCS,
    'documentation': COMMAND_CATEGORIES.DOCS,
    'events': COMMAND_CATEGORIES.EVENTS,
    'event': COMMAND_CATEGORIES.EVENTS,
    'input': COMMAND_CATEGORIES.INPUT,
    'kernel': COMMAND_CATEGORIES.KERNEL,
    'persona': COMMAND_CATEGORIES.PERSONA,
    'academy': COMMAND_CATEGORIES.ACADEMY,
    'other': COMMAND_CATEGORIES.OTHER
  };
  
  return categoryMap[normalized] || COMMAND_CATEGORIES.OTHER;
}

/**
 * Helper function to get category display name
 */
export function getCategoryDisplayName(category: CommandCategory): string {
  const displayNames: Record<CommandCategory, string> = {
    [COMMAND_CATEGORIES.CORE]: 'Core',
    [COMMAND_CATEGORIES.SYSTEM]: 'System',
    [COMMAND_CATEGORIES.BROWSER]: 'Browser',
    [COMMAND_CATEGORIES.FILE]: 'File',
    [COMMAND_CATEGORIES.AI]: 'AI',
    [COMMAND_CATEGORIES.SESSION]: 'Session',
    [COMMAND_CATEGORIES.DEVELOPMENT]: 'Development',
    [COMMAND_CATEGORIES.COMMUNICATION]: 'Communication',
    [COMMAND_CATEGORIES.MONITORING]: 'Monitoring',
    [COMMAND_CATEGORIES.TESTING]: 'Testing',
    [COMMAND_CATEGORIES.UI]: 'UI',
    [COMMAND_CATEGORIES.DATABASE]: 'Database',
    [COMMAND_CATEGORIES.PLANNING]: 'Planning',
    [COMMAND_CATEGORIES.DOCS]: 'Documentation',
    [COMMAND_CATEGORIES.EVENTS]: 'Events',
    [COMMAND_CATEGORIES.INPUT]: 'Input',
    [COMMAND_CATEGORIES.KERNEL]: 'Kernel',
    [COMMAND_CATEGORIES.PERSONA]: 'Persona',
    [COMMAND_CATEGORIES.ACADEMY]: 'Academy',
    [COMMAND_CATEGORIES.OTHER]: 'Other'
  };
  
  return displayNames[category] || 'Unknown';
}

/**
 * Create a standardized error result
 */
export function createErrorResult(error: string | Error, processor?: string): CommandResult {
  const result: CommandResult = {
    success: false,
    error: error instanceof Error ? error.message : error,
    timestamp: new Date().toISOString()
  };
  
  if (processor) {
    result.processor = processor;
  }
  
  return result;
}

/**
 * Create a standardized success result
 */
export function createSuccessResult(data?: unknown, processor?: string): CommandResult {
  const result: CommandResult = {
    success: true,
    data,
    timestamp: new Date().toISOString()
  };
  
  if (processor) {
    result.processor = processor;
  }
  
  return result;
}