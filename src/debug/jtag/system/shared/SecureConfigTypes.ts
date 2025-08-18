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
}

export interface ServerEnvironmentConfig {
  readonly log_level: 'debug' | 'info' | 'warn' | 'error';
  readonly debug_mode: boolean;
}

// SERVER-ONLY configuration interface
export interface JTAGServerConfiguration {
  readonly server: ServerConfig;
  readonly paths: ServerPathsConfig;
  readonly security: ServerSecurityConfig;
  readonly environment: ServerEnvironmentConfig;
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

// Utility functions for secure configuration
export function buildServerUrl(config: { server: Pick<ServerConfig, 'protocol' | 'host' | 'port'> }): string {
  return `${config.server.protocol}://${config.server.host}:${config.server.port}`;
}

export function buildClientUrl(config: { client: Pick<ClientConfig, 'protocol' | 'host' | 'ui_port'> }): string {
  return `${config.client.protocol}://${config.client.host}:${config.client.ui_port}`;
}

// Security validation
export function validateServerConfig(config: any): config is JTAGServerConfiguration {
  return config && 
         typeof config.server?.port === 'number' &&
         typeof config.server?.host === 'string' &&
         ['ws', 'wss'].includes(config.server?.protocol);
}

export function validateClientConfig(config: any): config is JTAGClientConfiguration {
  return config && 
         typeof config.client?.ui_port === 'number' &&
         typeof config.client?.host === 'string' &&
         ['http', 'https'].includes(config.client?.protocol);
}