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

import { CommandParams, CommandResult } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class CompileTypescriptParams extends CommandParams {
  source!: string;
  filename?: string;
  outputPath?: string;
  strict?: boolean;
  target?: 'es5' | 'es2015' | 'es2020' | 'esnext';

  constructor(data: Partial<CompileTypescriptParams> = {}) {
    super();
    Object.assign(this, {
      source: '',
      filename: 'code.ts',
      outputPath: './dist',
      strict: true,
      target: 'es2020',
      ...data
    });
  }
}

export class CompileTypescriptResult extends CommandResult {
  success: boolean;
  output?: string;
  outputPath?: string;
  errors: string[];
  warnings: string[];
  compilationTime?: number;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<CompileTypescriptResult>) {
    super();
    this.success = data.success ?? false;
    this.output = data.output;
    this.outputPath = data.outputPath;
    this.errors = data.errors ?? [];
    this.warnings = data.warnings ?? [];
    this.compilationTime = data.compilationTime;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}