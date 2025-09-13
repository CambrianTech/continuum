/**
 * Generator System Types
 * 
 * Well-typed interfaces for the modular generator system.
 * Replaces the `any` types and provides proper validation.
 */

// ============================================================================
// Core Configuration Types (Path aliases removed - keep only structure types)
// ============================================================================

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
  version: string;
  targets: Record<string, StructureTarget>;
  generatedAt: string;
  description: string;
}

// ============================================================================
// Package.json Configuration Types
// ============================================================================

export interface PackageJsonConfig {
  name?: string;
  version?: string;
  imports?: Record<string, string>;
  _importsGenerated?: {
    timestamp: string;
    source: string;
    pathCount: number;
  };
  [key: string]: unknown;
}

// ============================================================================
// TypeScript Configuration Types
// ============================================================================

export interface TypeScriptConfig {
  compilerOptions: {
    paths?: Record<string, string[]>;
    [key: string]: unknown;
  };
  include?: string[];
  exclude?: string[];
  _pathsGenerated?: {
    timestamp: string;
    source: string;
    pathCount: number;
    note: string;
  };
  [key: string]: unknown;
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
// Strategy Types (path alias strategy removed - keep only file discovery)
// ============================================================================

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
  debug(message: string, ...args: unknown[]): void;
  info(message: string, ...args: unknown[]): void;
  warn(message: string, ...args: unknown[]): void;
  error(message: string, ...args: unknown[]): void;
}

export interface GeneratorOptions {
  rootPath: string;
  logger?: GeneratorLogger;
  dryRun?: boolean;
  force?: boolean;
}