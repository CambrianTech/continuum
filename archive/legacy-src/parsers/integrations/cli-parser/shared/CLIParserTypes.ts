/**
 * CLI Parser Types - Shared types for CLI integration parsing
 */

export interface CLIInputFormat {
  args: string[];
  command?: string;
  flags?: Record<string, string | boolean>;
}

export interface CLIOutputFormat {
  success: boolean;
  data?: any;
  error?: string;
  executionTime?: number;
}

export interface CLIParserConfig {
  enableColors: boolean;
  enableEmojis: boolean;
  verboseOutput: boolean;
  maxOutputLength: number;
}

export interface CLIFormattingOptions {
  showTimestamp: boolean;
  showExecutionTime: boolean;
  showStatusIcon: boolean;
  compactMode: boolean;
}

export const DEFAULT_CLI_CONFIG: CLIParserConfig = {
  enableColors: true,
  enableEmojis: true,
  verboseOutput: false,
  maxOutputLength: 10000
};

export const DEFAULT_CLI_FORMATTING: CLIFormattingOptions = {
  showTimestamp: false,
  showExecutionTime: true,
  showStatusIcon: true,
  compactMode: false
};