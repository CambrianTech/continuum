/**
 * JTAG Configuration Types - Shared across system and tests
 * 
 * These types define the configuration structure that can be used by:
 * - Core system components (reading from config.json)
 * - Tests (reading from tests.json or their own config)
 * - Any other consumers that need typed configuration
 */

export interface ServerConfig {
  readonly port: number;
  readonly host: string;
  readonly protocol: 'ws' | 'wss';
}

export interface ClientConfig {
  readonly ui_port: number;
  readonly host: string;
  readonly protocol: 'http' | 'https';
}

export interface PathsConfig {
  readonly test_bench: string;
  readonly logs: string;
  readonly screenshots: string;
}

export interface BrowserConfig {
  readonly headless: boolean;
  readonly devtools: boolean;
  readonly width: number;
  readonly height: number;
}

export interface EnvironmentConfig {
  readonly test_mode: boolean;
  readonly verbose_logging: boolean;
}

/**
 * Complete JTAG configuration structure
 * All fields are required and non-optional for type safety
 */
export interface JTAGConfiguration {
  readonly server: ServerConfig;
  readonly client: ClientConfig;
  readonly paths: PathsConfig;
  readonly browser: BrowserConfig;
  readonly environment: EnvironmentConfig;
}

/**
 * Utility function to build server URL from typed config
 */
export function buildServerUrl(config: JTAGConfiguration): string {
  return `${config.server.protocol}://${config.server.host}:${config.server.port}`;
}