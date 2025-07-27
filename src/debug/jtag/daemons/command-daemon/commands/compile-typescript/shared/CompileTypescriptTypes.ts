// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Compile TypeScript Command - Shared Types
 * 
 * Minimal, focused types for TypeScript compilation only.
 * Follows screenshot/navigate pattern - simple params and results.
 * 
 * DESIGN PRINCIPLES:
 * ✅ Single responsibility - only TypeScript compilation
 * ✅ Clean parameter interface with sensible defaults
 * ✅ Object.assign() constructor pattern
 * ✅ No over-engineering or god objects
 * ✅ Focused scope - just TSC compilation
 * 
 * SCOPE:
 * - Browser: Uses monaco/typescript service if available
 * - Server: Uses local tsc executable or typescript module
 * - Consistent interface across contexts
 */

import { CommandParams, CommandResult, createPayload, type JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from 'crypto';

export interface CompileTypescriptParams extends CommandParams {
  readonly source: string;
  readonly filename?: string;
  readonly outputPath?: string;
  readonly strict?: boolean;
  readonly target?: 'es5' | 'es2015' | 'es2020' | 'esnext';
}

export const createCompileTypescriptParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    source?: string;
    filename?: string;
    outputPath?: string;
    strict?: boolean;
    target?: 'es5' | 'es2015' | 'es2020' | 'esnext';
  }
): CompileTypescriptParams => createPayload(context, sessionId, {
  source: data.source ?? '',
  filename: data.filename ?? 'code.ts',
  outputPath: data.outputPath ?? './dist',
  strict: data.strict ?? true,
  target: data.target ?? 'es2020',
  ...data
});

export interface CompileTypescriptResult extends CommandResult {
  readonly success: boolean;
  readonly output?: string;
  readonly outputPath?: string;
  readonly errors: string[];
  readonly warnings: string[];
  readonly compilationTime?: number;
  readonly timestamp: string;
}

export const createCompileTypescriptResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    output?: string;
    outputPath?: string;
    errors?: string[];
    warnings?: string[];
    compilationTime?: number;
  }
): CompileTypescriptResult => createPayload(context, sessionId, {
  output: data.output,
  outputPath: data.outputPath,
  errors: data.errors ?? [],
  warnings: data.warnings ?? [],
  compilationTime: data.compilationTime,
  timestamp: new Date().toISOString(),
  ...data
});