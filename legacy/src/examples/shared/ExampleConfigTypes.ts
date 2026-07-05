/**
 * Example Configuration Types - Browser-safe type definitions
 *
 * These types are truly shared and can be safely imported in browser code.
 * NO Node.js dependencies, NO file system access, NO ServerConfig imports.
 */

/**
 * Example configuration structure
 */
export interface ExampleConfig {
  readonly active_example: string;
  readonly examples: Record<string, ExampleDefinition>;
}

/**
 * Individual example definition
 */
export interface ExampleDefinition {
  readonly name: string;
  readonly description: string;
  readonly ports: ExamplePorts;
  readonly paths: ExamplePaths;
  readonly features?: Record<string, boolean>;
}

/**
 * Port configuration for an example
 */
export interface ExamplePorts {
  readonly http_server: number;
  readonly websocket_server: number;
}

/**
 * Path configuration for an example
 */
export interface ExamplePaths {
  readonly directory: string;
  readonly html_file: string;
  readonly build_output: string;
}
