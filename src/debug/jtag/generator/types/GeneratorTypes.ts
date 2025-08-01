/**
 * Generator System Types
 * 
 * Well-typed interfaces for the modular generator system.
 * Replaces the `any` types and provides proper validation.
 */

// ============================================================================
// Core Configuration Types
// ============================================================================

export interface PathMapping {
  alias: string;
  relativePath: string;
  description: string;
  essential: boolean; // True for manually curated, false for auto-generated
}

export interface PathMappingsConfig {
  version: string;
  generatedAt: string;
  mappings: Record<string, PathMapping>;
  description: string;
}

// ============================================================================
// Structure Generation Types  
// ============================================================================

export interface StructureTarget {
  outputFile: string;
  environment: 'browser' | 'server';
  daemonPaths?: string[];
  commandPaths?: string[];
  excludePatterns: string[];
}

export interface StructureConfig {
  targets: Record<string, StructureTarget>;
  generatedAt: string;
  description: string;
}

// ============================================================================
// Generated File Content Types
// ============================================================================

export interface DaemonEntry {
  name: string;
  className: string;
  importPath: string;
  disabled?: boolean;
  reason?: string;
}

export interface CommandEntry {
  name: string;
  className: string;
  importPath: string;
  disabled?: boolean;
  reason?: string;
}

export interface GeneratedStructureFile {
  environment: 'browser' | 'server';
  daemons: DaemonEntry[];
  commands: CommandEntry[];
  generatedAt: string;
  sourceFiles: string[]; // Files that were scanned to generate this
}

// ============================================================================
// File Management Types
// ============================================================================

export interface FileUpdate {
  filePath: string;
  backupPath?: string;
  content: string;
  reason: string;
}

export interface GenerationResult {
  success: boolean;
  filesUpdated: FileUpdate[];
  errors: string[];
  warnings: string[];
  stats: {
    pathMappingsGenerated: number;
    structureFilesGenerated: number;
    daemonsFound: number;
    commandsFound: number;
  };
}

// ============================================================================
// Strategy Types (for pluggable behavior)
// ============================================================================

export interface AliasStrategy {
  name: string;
  generateAlias(relativePath: string): string;
  description: string;
}

export interface FileDiscoveryStrategy {
  name: string;
  findFiles(patterns: string[], excludePatterns: string[], rootPath: string): string[];
  description: string;
}

// ============================================================================
// Configuration Validation Types
// ============================================================================

export interface ValidationRule<T> {
  name: string;
  validate(config: T): ValidationResult;
  description: string;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

// ============================================================================
// Utility Types
// ============================================================================

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface GeneratorLogger {
  debug(message: string, ...args: any[]): void;
  info(message: string, ...args: any[]): void;
  warn(message: string, ...args: any[]): void;
  error(message: string, ...args: any[]): void;
}

export interface GeneratorOptions {
  rootPath: string;
  logger?: GeneratorLogger;
  dryRun?: boolean;
  force?: boolean;
}