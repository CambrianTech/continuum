/**
 * Example Configuration Types
 * 
 * Centralized configuration for all JTAG examples (test-bench, widget-ui, etc.)
 * Provides type-safe access to example-specific settings and port configurations.
 */

export interface ExamplePortConfiguration {
  readonly http_server: number;
  readonly websocket_server: number;
}

export interface ExamplePathConfiguration {
  readonly directory: string;
  readonly html_file: string;
  readonly build_output: string;
}

export interface ExampleFeatures {
  readonly browser_automation: boolean;
  readonly screenshot_testing: boolean;
  readonly chat_integration: boolean;
  readonly widget_testing: boolean;
  readonly auto_launch_browser: boolean;
}

export interface ExampleDefinition {
  readonly name: string;
  readonly description: string;
  readonly ports: ExamplePortConfiguration;
  readonly paths: ExamplePathConfiguration;
  readonly features: ExampleFeatures;
}

export interface WebSocketConnectionConfig {
  readonly host: string;
  readonly protocol: 'ws' | 'wss';
  readonly reconnect_attempts: number;
  readonly timeout_ms: number;
}

export interface BuildSettings {
  readonly target: string;
  readonly format: string;
  readonly minify: boolean;
  readonly sourcemap: boolean;
}

export interface CommonConfiguration {
  readonly websocket_connection: WebSocketConnectionConfig;
  readonly build_settings: BuildSettings;
}

export interface ExamplesConfiguration {
  readonly active_example: string;
  readonly examples: Record<string, ExampleDefinition>;
  readonly common: CommonConfiguration;
}

export type ExampleName = 'test-bench' | 'widget-ui';