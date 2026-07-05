/**
 * Grid Deploy Command - Shared Types
 *
 * Pull latest code and rebuild on grid nodes. Runs git pull + npm run build:ts on each reachable node via SSH over Tailscale. Keeps all nodes in sync without manual SSH.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Grid Deploy Command Parameters
 */
export interface GridDeployParams extends CommandParams {
  // Comma-separated node names or IPs to deploy to. Default: all known grid nodes.
  nodes?: string;
  // Git branch to checkout. Default: current branch on each node.
  branch?: string;
  // Skip npm run build:ts after pull (just update code). Default: false.
  skipBuild?: boolean;
  // Restart the system (npm stop + npm start) after build. Default: false.
  restart?: boolean;
}

/**
 * Factory function for creating GridDeployParams
 */
export const createGridDeployParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Comma-separated node names or IPs to deploy to. Default: all known grid nodes.
    nodes?: string;
    // Git branch to checkout. Default: current branch on each node.
    branch?: string;
    // Skip npm run build:ts after pull (just update code). Default: false.
    skipBuild?: boolean;
    // Restart the system (npm stop + npm start) after build. Default: false.
    restart?: boolean;
  }
): GridDeployParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  nodes: data.nodes ?? '',
  branch: data.branch ?? '',
  skipBuild: data.skipBuild ?? false,
  restart: data.restart ?? false,
  ...data
});

/**
 * Grid Deploy Command Result
 */
export interface GridDeployResult extends CommandResult {
  success: boolean;
  // Array of { nodeId, status, branch, buildSuccess, error? } per node
  deployedNodes: object;
  // Number of nodes successfully deployed
  totalDeployed: number;
  error?: JTAGError;
}

/**
 * Factory function for creating GridDeployResult with defaults
 */
export const createGridDeployResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of { nodeId, status, branch, buildSuccess, error? } per node
    deployedNodes?: object;
    // Number of nodes successfully deployed
    totalDeployed?: number;
    error?: JTAGError;
  }
): GridDeployResult => createPayload(context, sessionId, {
  deployedNodes: data.deployedNodes ?? {},
  totalDeployed: data.totalDeployed ?? 0,
  ...data
});

/**
 * Smart Grid Deploy-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGridDeployResultFromParams = (
  params: GridDeployParams,
  differences: Omit<GridDeployResult, 'context' | 'sessionId' | 'userId'>
): GridDeployResult => transformPayload(params, differences);

/**
 * Grid Deploy — Type-safe command executor
 *
 * Usage:
 *   import { GridDeploy } from '...shared/GridDeployTypes';
 *   const result = await GridDeploy.execute({ ... });
 */
export const GridDeploy = {
  execute(params: CommandInput<GridDeployParams>): Promise<GridDeployResult> {
    return Commands.execute<GridDeployParams, GridDeployResult>('grid/deploy', params as Partial<GridDeployParams>);
  },
  commandName: 'grid/deploy' as const,
} as const;
