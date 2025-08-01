// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Compile TypeScript Command - Abstract Base Class
 * 
 * Clean abstraction for TypeScript compilation. Follows the exact pattern
 * of screenshot/navigate/click commands with proper generics and minimal interface.
 * 
 * DESIGN ANALYSIS:
 * ✅ Focused on single language - TypeScript only
 * ✅ Clean CommandBase inheritance with proper generics
 * ✅ Sensible default parameters
 * ✅ Single abstract method to implement
 * ✅ No unnecessary complexity
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { type CompileTypescriptParams, createCompileTypescriptParams } from '@commandsCompileTypescript/shared/CompileTypescriptTypes';
import type { CompileTypescriptResult } from '@commandsCompileTypescript/shared/CompileTypescriptTypes';

export abstract class CompileTypescriptCommand extends CommandBase<CompileTypescriptParams, CompileTypescriptResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('compile-typescript', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): CompileTypescriptParams {
    return createCompileTypescriptParams(this.context, sessionId, {
      source: '',
      target: 'es2020',
      strict: true
    });
  }

  abstract execute(params: CompileTypescriptParams): Promise<CompileTypescriptResult>;
}