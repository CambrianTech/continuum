/**
 * Strong TypeScript interfaces for Continuum package.json structure
 * 
 * Ensures consistency across all modules, commands, and daemons
 */

/**
 * Base package.json structure
 */
export interface PackageJson {
  name: string;
  version: string;
  description: string;
  main: string;
  type: 'module' | 'commonjs';
  continuum: ContinuumConfig;
  scripts?: Record<string, string>;
  keywords?: string[];
  author?: string;
  license?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

/**
 * Base Continuum module configuration - inherited by all module types
 */
export interface BaseContinuumConfig {
  // Classification
  category: ContinuumCategory;
  
  // Capabilities provided by this module
  capabilities: (ContinuumCapability | string)[];
  
  // Dependencies on other Continuum modules
  dependencies: string[];
  
  // Interfaces this module implements or uses
  interfaces: (ContinuumInterface | string)[];
  
  // Permissions required for operation
  permissions: (ContinuumPermission | string)[];
  
  // Execution priority
  priority?: ContinuumPriority;
}

/**
 * Command-specific configuration
 */
export interface ContinuumCommandConfig extends BaseContinuumConfig {
  command: string;         // Command name: 'connect', 'file_write', etc.
  category: ContinuumCategory;
}

/**
 * Daemon-specific configuration  
 */
export interface ContinuumDaemonConfig extends BaseContinuumConfig {
  daemon: string;          // Daemon name: 'session-manager', 'browser-manager', etc.
  category: ContinuumCategory.Core | ContinuumCategory.Integration;  // Daemons are always Core or Integration
  startupOrder?: number;   // Order for daemon startup
  healthCheck?: {
    enabled: boolean;
    intervalMs: number;
    timeoutMs: number;
  };
}

/**
 * Widget-specific configuration
 */
export interface ContinuumWidgetConfig extends BaseContinuumConfig {
  widget: string;          // Widget name: 'chat', 'sidebar', etc.
  category: ContinuumCategory.UI;          // Widgets are always UI category
  ui: {
    template?: string;     // HTML template file
    styles?: string[];     // CSS files
    scripts?: string[];    // JavaScript files
    dependencies?: string[]; // UI framework dependencies
  };
}

/**
 * Generic module configuration (for utilities, base classes, etc.)
 */
export interface ContinuumModuleConfig extends BaseContinuumConfig {
  module: string;          // Module name: 'system-startup', 'base-file-command', etc.
  type?: ContinuumType;    // Module type
}

/**
 * Union type for all Continuum configurations
 */
export type ContinuumConfig = 
  | ContinuumCommandConfig 
  | ContinuumDaemonConfig 
  | ContinuumWidgetConfig 
  | ContinuumModuleConfig;

/**
 * Module categories in Continuum
 */
export enum ContinuumCategory {
  Kernel = 'Kernel',                    // Core system commands
  Core = 'Core',                        // Essential daemons and infrastructure
  File = 'File',                        // File system operations
  Communication = 'Communication',      // Chat, messaging, networking
  Browser = 'Browser',                  // Browser automation and management
  Development = 'Development',          // Development tools and workflows
  UI = 'UI',                           // User interface components
  Monitoring = 'Monitoring',           // System monitoring and diagnostics
  Planning = 'Planning',               // Planning and analysis tools
  Input = 'Input',                     // Input handling and automation
  Testing = 'Testing',                 // Testing frameworks and utilities
  Documentation = 'Documentation',     // Documentation and help systems
  Integration = 'Integration'          // External service integrations
}

/**
 * Module types
 */
export enum ContinuumType {
  Command = 'command',                 // Executable command
  Daemon = 'daemon',                   // Background service
  Widget = 'widget',                   // UI component
  BaseClass = 'base-class',            // Shared base class
  Utility = 'utility',                // Utility module
  Integration = 'integration',         // External integration
  TestFramework = 'test-framework',    // Testing infrastructure
  System = 'system'                    // System module
}

/**
 * Capabilities that modules can provide
 */
export enum ContinuumCapability {
  // File system capabilities
  FileReading = 'file-reading',
  FileWriting = 'file-writing', 
  FileAppending = 'file-appending',
  DirectoryListing = 'directory-listing',
  ArtifactManagement = 'artifact-management',
  SessionAware = 'session-aware',
  DirectoryManagement = 'directory-management',
  
  // System capabilities  
  SystemControl = 'system-control',
  DaemonManagement = 'daemon-management',
  ProcessCoordination = 'process-coordination',
  KernelOperations = 'kernel-operations',
  SystemCoordination = 'system-coordination',
  
  // Session capabilities
  SessionManagement = 'session-management',
  SessionCoordination = 'session-coordination',
  SessionIsolation = 'session-isolation',
  ConnectionIdentity = 'connection-identity',
  ArtifactCoordination = 'artifact-coordination',
  
  // Browser capabilities
  BrowserOrchestration = 'browser-orchestration',
  TabManagement = 'tab-management',
  DevtoolsIntegration = 'devtools-integration',
  BrowserLaunching = 'browser-launching',
  BrowserControl = 'browser-control',
  
  // Communication capabilities
  DaemonCommunication = 'daemon-communication',
  WebsocketManagement = 'websocket-management',
  MessageRouting = 'message-routing',
  
  // Testing capabilities
  UniversalTesting = 'universal-testing',
  LayerValidation = 'layer-validation',
  ModularCompliance = 'modular-compliance',
  IntegrationTesting = 'integration-testing',
  SystemHealth = 'system-health',
  
  // UI capabilities
  WidgetDiscovery = 'widget-discovery',
  UiRendering = 'ui-rendering',
  TemplateManagement = 'template-management',
  
  // Development capabilities
  TypescriptCompilation = 'typescript-compilation',
  CodeGeneration = 'code-generation',
  HotReloading = 'hot-reloading',
  
  // Monitoring capabilities
  HealthMonitoring = 'health-monitoring',
  PerformanceTracking = 'performance-tracking',
  ErrorReporting = 'error-reporting',
  
  // Directory capabilities
  RetentionPolicies = 'retention-policies',
  DirectoryAnalytics = 'directory-analytics',
  IntelligentOrganization = 'intelligent-organization',
  ContinuumDirectoryDaemon = 'continuum-directory-daemon',
  PathResolution = 'path-resolution'
}

/**
 * Interfaces that modules can implement or depend on
 */
export enum ContinuumInterface {
  CommandBus = 'command-bus',          // Command execution interface
  DaemonProtocol = 'daemon-protocol',      // Daemon communication protocol
  FileSystem = 'file-system',          // File system operations
  SessionManagement = 'session-management',   // Session management interface
  BrowserManagement = 'browser-management',   // Browser control interface
  WebsocketProtocol = 'websocket-protocol',   // WebSocket communication
  UiRendering = 'ui-rendering',         // UI rendering interface
  TestingFramework = 'testing-framework',    // Testing infrastructure
  WidgetSystem = 'widget-system',        // Widget management interface
  SystemManagement = 'system-management',    // System control interface
  ArtifactStorage = 'artifact-storage',     // Artifact management interface
  TemplateEngine = 'template-engine',      // Template processing interface
  EventSystem = 'event-system'        // Event handling interface
}

/**
 * Permissions required by modules
 */
export enum ContinuumPermission {
  // File system permissions
  Read = 'read',
  Write = 'write',
  Append = 'append',
  CreateFiles = 'create-files',
  CreateDirectories = 'create-directories',
  List = 'list',
  DirectoryStats = 'directory-stats',
  CreateSymlinks = 'create-symlinks',
  
  // System permissions
  Kernel = 'kernel',
  System = 'system',
  DaemonControl = 'daemon-control',
  ProcessManagement = 'process-management',
  DaemonManagement = 'daemon-management',
  
  // Session permissions
  SessionManagement = 'session-management',
  SessionCreation = 'session-creation',
  SessionIsolation = 'session-isolation',
  
  // Browser permissions
  BrowserControl = 'browser-control',
  BrowserLaunching = 'browser-launching',
  DevtoolsAccess = 'devtools-access',
  
  // Network permissions
  NetworkAccess = 'network-access',
  WebsocketServer = 'websocket-server',
  HttpServer = 'http-server',
  
  // Communication permissions
  DaemonCommunication = 'daemon-communication',
  MessageRouting = 'message-routing',
  EventHandling = 'event-handling',
  
  // Testing permissions
  Execute = 'execute',
  NetworkTest = 'network-test',
  SystemTest = 'system-test'
}

/**
 * Execution priority levels
 */
export enum ContinuumPriority {
  Critical = 'critical',     // Kernel-level, system-critical operations
  High = 'high',             // Important system operations
  Normal = 'normal',         // Standard operations
  Low = 'low',               // Background, non-critical operations
  Background = 'background'  // Background tasks
}

/**
 * Utility functions for working with Continuum packages
 */
export class ContinuumPackageUtils {
  /**
   * Validate base Continuum configuration
   */
  static validateBaseConfig(config: any): config is BaseContinuumConfig {
    return (
      typeof config === 'object' &&
      typeof config.category === 'string' &&
      Array.isArray(config.capabilities) &&
      Array.isArray(config.dependencies) &&
      Array.isArray(config.interfaces) &&
      Array.isArray(config.permissions)
    );
  }

  /**
   * Validate specific module type configuration
   */
  static validateConfig(config: any): config is ContinuumConfig {
    if (!this.validateBaseConfig(config)) return false;
    
    // Check for exactly one module type identifier
    const identifiers = ['command' in config, 'daemon' in config, 'widget' in config, 'module' in config].filter(Boolean);
    return identifiers.length === 1;
  }

  /**
   * Check if configuration is for a command
   */
  static isCommandConfig(config: ContinuumConfig): config is ContinuumCommandConfig {
    return 'command' in config;
  }

  /**
   * Check if configuration is for a daemon
   */
  static isDaemonConfig(config: ContinuumConfig): config is ContinuumDaemonConfig {
    return 'daemon' in config;
  }

  /**
   * Check if configuration is for a widget
   */
  static isWidgetConfig(config: ContinuumConfig): config is ContinuumWidgetConfig {
    return 'widget' in config;
  }

  /**
   * Check if configuration is for a generic module
   */
  static isModuleConfig(config: ContinuumConfig): config is ContinuumModuleConfig {
    return 'module' in config;
  }

  /**
   * Check if a module has a specific capability
   */
  static hasCapability(config: ContinuumConfig, capability: ContinuumCapability | string): boolean {
    return config.capabilities.includes(capability as any);
  }

  /**
   * Check if a module requires a specific permission
   */
  static requiresPermission(config: ContinuumConfig, permission: ContinuumPermission | string): boolean {
    return config.permissions.includes(permission as any);
  }

  /**
   * Get module dependencies
   */
  static getDependencies(config: ContinuumConfig): string[] {
    return config.dependencies;
  }

  /**
   * Get module type (command, daemon, widget, module)
   */
  static getModuleType(config: ContinuumConfig): ContinuumType {
    if (this.isCommandConfig(config)) return ContinuumType.Command;
    if (this.isDaemonConfig(config)) return ContinuumType.Daemon;
    if (this.isWidgetConfig(config)) return ContinuumType.Widget;
    if (this.isModuleConfig(config)) return config.type || ContinuumType.Utility;
    return ContinuumType.Utility;
  }

  /**
   * Get module identifier
   */
  static getModuleId(config: ContinuumConfig): string {
    if (this.isCommandConfig(config)) return config.command;
    if (this.isDaemonConfig(config)) return config.daemon;
    if (this.isWidgetConfig(config)) return config.widget;
    if (this.isModuleConfig(config)) return config.module;
    return 'unknown';
  }

  /**
   * Get startup order for daemons
   */
  static getStartupOrder(config: ContinuumConfig): number {
    if (this.isDaemonConfig(config)) {
      return config.startupOrder || 999;
    }
    return 999;
  }

  /**
   * Check if two modules are compatible (can work together)
   */
  static areCompatible(configA: ContinuumConfig, configB: ContinuumConfig): boolean {
    // Check if configA depends on configB or vice versa
    const idA = this.getModuleId(configA);
    const idB = this.getModuleId(configB);
    
    return (
      configA.dependencies.includes(idB) ||
      configB.dependencies.includes(idA) ||
      // Share common interfaces
      configA.interfaces.some(iface => configB.interfaces.includes(iface as any))
    );
  }
}