/**
 * Command Processor Module - Type Definitions
 * Clean, well-organized TypeScript interfaces for the command system
 */

export interface ProcessorConfig {
  commandDirs: string[];
  enableCaseInsensitive: boolean;
  enableTypeScriptOnly: boolean;
  logLevel: 'debug' | 'info' | 'warn' | 'error';
}

export interface CommandModule {
  default: any;
  [key: string]: any;
}

export interface CommandStats {
  totalCommands: number;
  categories: string[];
  loadedCommands: string[];
  failedLoads: number;
  initializationTime: number;
}

export interface ExecutionContext {
  processor: string;
  executionId: string;
  timestamp: Date;
  continuum?: any;
  continuonStatus?: any;
  [key: string]: any;
}

export interface CommandResult<T = any> {
  success: boolean;
  message: string;
  data?: T;
  error?: string;
  timestamp: string;
}

export interface CommandDefinition {
  name: string;
  category: string;
  description: string;
  icon: string;
  params: string;
  usage: string;
  examples: string[];
}

export interface LoadResult {
  loaded: number;
  errors: number;
  commands: string[];
}