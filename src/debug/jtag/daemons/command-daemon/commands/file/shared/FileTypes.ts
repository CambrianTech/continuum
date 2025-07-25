// ISSUES: 2 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// TODO: Remove generic constraints - Record<string, any> defeats purpose of strong typing
// TODO: Consider splitting into individual command type files to follow ~50 line modular pattern

/**
 * File Command Base Types - Generic Foundation for File Operations
 * 
 * Provides type-safe base classes for all file operations using TypeScript generics.
 * Follows the modular command architecture with ~50 line modules and shared types.
 * 
 * CORE ARCHITECTURE:
 * - Generic FileParams<T> base for type-safe parameter extension
 * - Generic FileResult<T> base for consistent result structure
 * - Abstract base classes prevent direct instantiation
 * - Object.assign constructor pattern for elegant initialization
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Parameter validation and default value assignment
 * - Integration tests: Cross-command type compatibility
 * - Type tests: Generic constraint validation
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Generics enable type-safe command extension without duplication
 * - Abstract classes provide structure while allowing specialization
 * - Shared types ensure consistency across file operations
 */

import { CommandParams, CommandResult } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';

/**
 * Generic base parameters for all file operations
 * T extends Record<string, any> allows type-safe extension
 */
export abstract class FileParams<T extends Record<string, any> = {}> extends CommandParams {
  filepath!: string;
  encoding?: string;

  constructor(data: Partial<FileParams<T> & T> = {}) {
    super();
    Object.assign(this, {
      filepath: '',
      encoding: 'utf8',
      ...data
    });
  }
}

/**
 * Generic base result for all file operations
 * T extends Record<string, any> allows type-safe extension
 */
export abstract class FileResult<T extends Record<string, any> = {}> extends CommandResult {
  success!: boolean;
  filepath!: string;
  exists!: boolean;
  error?: string;
  environment!: JTAGContext['environment'];
  timestamp!: string;

  constructor(data: Partial<FileResult<T> & T>) {
    super();
    Object.assign(this, {
      success: false,
      filepath: '',
      exists: false,
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}

/**
 * Generic base command class for all file operations
 * Provides type-safe inheritance with proper generic constraints
 */
export abstract class FileCommand<
  TParams extends FileParams<any>,
  TResult extends FileResult<any>
> extends CommandBase<TParams, TResult> {

  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context, subpath, commander);
  }

  abstract execute(params: TParams): Promise<TResult>;
}