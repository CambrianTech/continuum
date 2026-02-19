/**
 * Secure Configuration Architecture - Following Security Best Practices
 * 
 * SECURITY PRINCIPLES:
 * 1. Separation of Concerns: Server/Client configurations are separate files
 * 2. Principle of Least Privilege: Clients only get what they need
 * 3. No Secret Exposure: Server-only data never goes to client side
 * 4. Environment Isolation: Development/test/production configs are separate
 * 5. Runtime Configuration: Sensitive values come from environment variables
 */

// SERVER-ONLY configuration - NEVER sent to client
export interface ServerSecurityConfig {
  readonly enable_authentication: boolean;
  readonly session_timeout_ms: number;
  readonly rate_limiting: {
    readonly enabled: boolean;
    readonly requests_per_minute: number;
  };
}

export interface ServerConfig {
  readonly port: number;
  readonly host: string;
  readonly protocol: 'ws' | 'wss';
  readonly bind_interface: string;
  readonly max_connections: number;
  readonly enable_cors: boolean;
}

export interface ServerPathsConfig {
  readonly logs: string;
  readonly screenshots: string;
  readonly data_directory: string;
  readonly pid_file: string;
  readonly datasets?: string;  // Optional custom datasets directory (set via DATASETS_DIR env var, e.g., /path/to/datasets)
}

export interface ServerEnvironmentConfig {
  readonly log_level: 'debug' | 'info' | 'warn' | 'error';
  readonly debug_mode: boolean;
}

// Storage Configuration - SERVER-ONLY
export interface StorageConfig {
  readonly strategy: 'file' | 'sql' | 'nosql' | 'memory' | 'hybrid';
  readonly backend: string; // 'file', 'sqlite', 'postgres', 'mongodb', etc.
  readonly connectionString?: string;
  readonly paths: {
    readonly data: string;
    readonly backups: string;
  };
  readonly options?: Record<string, any>;
  readonly features?: {
    readonly enableTransactions?: boolean;
    readonly enableIndexing?: boolean;
    readonly enableReplication?: boolean;
    readonly enableSharding?: boolean;
    readonly enableCaching?: boolean;
  };
}

// Default Storage Configuration
export const DEFAULT_STORAGE_CONFIG: StorageConfig = {
  strategy: 'sql',
  backend: 'sqlite',
  connectionString: undefined,
  paths: {
    data: '.continuum/jtag/data',
    backups: '.continuum/jtag/backups'
  },
  options: {
    basePath: '.continuum/jtag/data',
    dbPath: '.continuum/jtag/data/database.sqlite'
  },
  features: {
    enableTransactions: true,
    enableIndexing: true,
    enableReplication: false,
    enableSharding: false,
    enableCaching: true
  }
} as const;

// SERVER-ONLY configuration interface
export interface JTAGServerConfiguration {
  readonly server: ServerConfig;
  readonly paths: ServerPathsConfig;
  readonly security: ServerSecurityConfig;
  readonly environment: ServerEnvironmentConfig;
  readonly storage: StorageConfig;
}

// CLIENT-SAFE configuration - can be sent to browser
export interface ClientConfig {
  readonly ui_port: number;
  readonly host: string;
  readonly protocol: 'http' | 'https';
  readonly auto_connect: boolean;
  readonly reconnect_attempts: number;
}

export interface BrowserConfig {
  readonly headless: boolean;
  readonly devtools: boolean;
  readonly width: number;
  readonly height: number;
  readonly user_agent: string;
}

export interface UIConfig {
  readonly theme: 'light' | 'dark';
  readonly enable_animations: boolean;
  readonly show_debug_panel: boolean;
}

// CLIENT-SAFE configuration interface
export interface JTAGClientConfiguration {
  readonly client: ClientConfig;
  readonly browser: BrowserConfig;
  readonly ui: UIConfig;
}

// TEST configuration - extends both but with test-specific settings
export interface TestConfig {
  readonly timeout_ms: number;
  readonly retry_attempts: number;
  readonly screenshot_on_failure: boolean;
  readonly cleanup_after_test: boolean;
}

export interface TestEnvironmentConfig {
  readonly test_mode: boolean;
  readonly verbose_logging: boolean;
  readonly isolated_sessions: boolean;
}

export interface JTAGTestConfiguration {
  readonly server: Pick<ServerConfig, 'port' | 'host' | 'protocol'>;
  readonly client: Pick<ClientConfig, 'ui_port' | 'host' | 'protocol'>;
  readonly test_settings: TestConfig;
  readonly environment: TestEnvironmentConfig;
}

// INSTANCE configuration - generic instances concept
export interface InstancePorts {
  readonly http_server: number;
  readonly websocket_server: number;
}

export interface InstancePaths {
  readonly directory: string;
  readonly html_file: string;
  readonly build_output: string;
}

export interface InstanceCapabilities {
  readonly browser_automation?: boolean;
  readonly screenshot_testing?: boolean;
  readonly chat_integration?: boolean;
  readonly widget_testing?: boolean;
  readonly auto_launch_browser?: boolean;
}

export interface InstanceConfiguration {
  readonly name: string;
  readonly description: string;
  readonly ports: InstancePorts;
  readonly paths: InstancePaths;
  readonly capabilities: InstanceCapabilities;
}

export interface InstanceConfigData {
  readonly active_instance: string;
  readonly instances: Record<string, InstanceConfiguration>;
}

// UNIFIED JTAG Configuration - context.config interface  
export interface JTAGConfig {
  readonly instance: InstanceConfiguration;
  readonly server: JTAGServerConfiguration;
  readonly client: JTAGClientConfiguration;
  readonly test?: JTAGTestConfiguration;
}

// Utility functions for secure configuration
export function buildServerUrl(config: { server: Pick<ServerConfig, 'protocol' | 'host' | 'port'> }): string {
  return `${config.server.protocol}://${config.server.host}:${config.server.port}`;
}

export function buildClientUrl(config: { client: Pick<ClientConfig, 'protocol' | 'host' | 'ui_port'> }): string {
  return `${config.client.protocol}://${config.client.host}:${config.client.ui_port}`;
}

export function buildServerWebSocketUrl(ports: InstancePorts): string {
  return `ws://localhost:${ports.websocket_server}`;
}

export function buildClientHttpUrl(ports: InstancePorts): string {
  return `http://localhost:${ports.http_server}`;
}

// Configuration validation - runtime type checking
export function validateInstanceConfig(config: unknown): config is InstanceConfiguration {
  return !!(
    config &&
    typeof config === 'object' &&
    config !== null &&
    'name' in config &&
    'ports' in config &&
    'paths' in config &&
    'capabilities' in config &&
    typeof (config as any).name === 'string' &&
    typeof (config as any).ports?.http_server === 'number' &&
    typeof (config as any).ports?.websocket_server === 'number' &&
    typeof (config as any).paths?.directory === 'string' &&
    typeof (config as any).capabilities === 'object'
  );
}

export function validateServerConfig(config: unknown): config is JTAGServerConfiguration {
  return !!(
    config &&
    typeof config === 'object' &&
    config !== null &&
    'server' in config &&
    typeof (config as any).server?.port === 'number' &&
    typeof (config as any).server?.host === 'string' &&
    ['ws', 'wss'].includes((config as any).server?.protocol)
  );
}

export function validateClientConfig(config: unknown): config is JTAGClientConfiguration {
  return !!(
    config &&
    typeof config === 'object' &&
    config !== null &&
    'client' in config &&
    typeof (config as any).client?.ui_port === 'number' &&
    typeof (config as any).client?.host === 'string' &&
    ['http', 'https'].includes((config as any).client?.protocol)
  );
}

export function validateJTAGConfig(config: unknown): config is JTAGConfig {
  return !!(
    config &&
    typeof config === 'object' &&
    config !== null &&
    'instance' in config &&
    'server' in config &&
    'client' in config &&
    'test' in config &&
    validateInstanceConfig((config as any).instance) &&
    validateServerConfig((config as any).server) &&
    validateClientConfig((config as any).client)
  );
}