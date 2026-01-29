/**
 * Tree Command Types
 *
 * Displays hierarchical command structure auto-generated from command registry.
 * Shows parent/child relationships like a file tree.
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Tree command parameters
 */
export interface TreeParams extends CommandParams {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  /**
   * Filter to specific command path (e.g., "ai" shows ai/*, "data" shows data/*)
   * Omit to show all commands
   */
  filter?: string;

  /**
   * Show command descriptions alongside names
   * @default false
   */
  showDescriptions?: boolean;

  /**
   * Maximum depth to display (1 = top level only, 2 = one level deep, etc.)
   * Omit for unlimited depth
   */
  maxDepth?: number;
}

/**
 * Tree node representing a command or command group
 */
export interface TreeNode {
  /** Node name (command part after last /) */
  name: string;

  /** Full command path (e.g., "ai/model/list") */
  fullPath: string;

  /** Is this a leaf command (true) or a parent group (false) */
  isCommand: boolean;

  /** Command description (if isCommand=true) */
  description?: string;

  /** Child nodes */
  children: TreeNode[];
}

/**
 * Tree command result
 */
export interface TreeResult extends CommandResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;

  /** Root nodes of the command tree */
  readonly tree: readonly TreeNode[];

  /** Total number of commands found */
  readonly commandCount: number;

  /** ASCII tree visualization */
  readonly visualization: string;

  readonly error?: string;
}

/**
 * Create TreeResult from TreeParams (type-safe factory)
 */
export function createTreeResultFromParams(
  params: TreeParams,
  data: Partial<Omit<TreeResult, 'context' | 'sessionId'>>
): TreeResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success ?? false,
    tree: data.tree ?? [],
    commandCount: data.commandCount ?? 0,
    visualization: data.visualization ?? '',
    error: data.error
  };
}

/**
 * Tree — Type-safe command executor
 *
 * Usage:
 *   import { Tree } from '...shared/TreeTypes';
 *   const result = await Tree.execute({ ... });
 */
export const Tree = {
  execute(params: CommandInput<TreeParams>): Promise<TreeResult> {
    return Commands.execute<TreeParams, TreeResult>('workspace/tree', params as Partial<TreeParams>);
  },
  commandName: 'workspace/tree' as const,
} as const;
