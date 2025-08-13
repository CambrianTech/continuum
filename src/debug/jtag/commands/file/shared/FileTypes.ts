// ISSUES: 1 open, last updated 2025-08-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
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

import { CommandParams, CommandResult, createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Generic base parameters for all file operations
 */
export interface FileParams extends CommandParams {
  readonly filepath: string;
  readonly encoding?: string;
}

export const createFileParams = <T extends Partial<FileParams> = FileParams>(
  context: JTAGContext,
  sessionId: UUID,
  data: T & { filepath?: string }
): FileParams & T => createPayload(context, sessionId, {
  filepath: data.filepath ?? '',
  encoding: data.encoding ?? 'utf8',
  ...data
} as FileParams & T);

/**
 * Generic base result for all file operations
 */
export interface FileResult extends CommandResult {
  readonly success: boolean;
  readonly filepath: string;
  readonly exists: boolean;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createFileResult = <T extends Partial<FileResult> = FileResult>(
  context: JTAGContext,
  sessionId: UUID,
  data: T & { success: boolean; filepath: string }
): FileResult & T => createPayload(context, sessionId, {
  exists: data.exists ?? false,
  timestamp: data.timestamp ?? new Date().toISOString(),
  ...data
} as FileResult & T);

/**
 * Generic base command class for all file operations
 * Provides type-safe inheritance with proper generic constraints
 */
export abstract class FileCommand<
  TParams extends FileParams,
  TResult extends FileResult
> extends CommandBase<TParams, TResult> {

  constructor(name: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(name, context, subpath, commander);
  }

  abstract execute(params: TParams): Promise<TResult>;
}