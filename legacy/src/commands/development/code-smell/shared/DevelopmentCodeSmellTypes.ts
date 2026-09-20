/**
 * Code Smell Detection Command
 *
 * Scans codebase for structural violations: raw Commands.execute, any casts,
 * god classes, missing accessors, type violations. Uses Generator SDK audit
 * infrastructure + grep patterns.
 *
 * Designed to be runnable by sentinels and local AI personas — output is
 * structured and verifiable, making it ideal for lesser models to act on.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export type SmellCategory = 'any-casts' | 'raw-execute' | 'god-class' | 'missing-accessor' | 'missing-types' | 'record-unknown';

export interface CodeSmellLocation {
  readonly file: string;
  readonly line?: number;
  readonly text?: string;
}

export interface CodeSmellCategoryResult {
  readonly category: SmellCategory;
  readonly count: number;
  readonly locations: readonly CodeSmellLocation[];
  readonly fixable: boolean;
  readonly description: string;
}

export interface DevelopmentCodeSmellParams extends CommandParams {
  /** Filter by smell category (default: all) */
  readonly category?: SmellCategory | 'all';
  /** Limit scan to a specific directory path relative to src/ */
  readonly path?: string;
  /** Auto-fix where possible */
  readonly fix?: boolean;
  /** Show individual file-level details */
  readonly verbose?: boolean;
}

export interface DevelopmentCodeSmellResult extends CommandResult {
  readonly success: boolean;
  readonly totalSmells: number;
  readonly categories: readonly CodeSmellCategoryResult[];
  readonly summary: string;
  readonly fixed?: number;
}

export const createDevelopmentCodeSmellParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DevelopmentCodeSmellParams, 'context' | 'sessionId' | 'userId'>
): DevelopmentCodeSmellParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

export const createDevelopmentCodeSmellResultFromParams = (
  params: DevelopmentCodeSmellParams,
  differences: Omit<Partial<DevelopmentCodeSmellResult>, 'context' | 'sessionId'>
): DevelopmentCodeSmellResult => transformPayload(params, {
  success: false,
  totalSmells: 0,
  categories: [],
  summary: '',
  ...differences
});

/**
 * DevelopmentCodeSmell — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentCodeSmell } from '...shared/DevelopmentCodeSmellTypes';
 *   const result = await DevelopmentCodeSmell.execute({});
 *   const anyOnly = await DevelopmentCodeSmell.execute({ category: 'any-casts' });
 */
export const DevelopmentCodeSmell = {
  execute(params: CommandInput<DevelopmentCodeSmellParams>): Promise<DevelopmentCodeSmellResult> {
    return Commands.execute<DevelopmentCodeSmellParams, DevelopmentCodeSmellResult>(
      'development/code-smell', params as Partial<DevelopmentCodeSmellParams>
    );
  },
  commandName: 'development/code-smell' as const,
} as const;
