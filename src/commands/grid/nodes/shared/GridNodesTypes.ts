/**
 * Grid Nodes Command - Shared Types
 *
 * Lists all known nodes on the Grid mesh.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { GridNode } from '../../../../workers/continuum-core/bindings/modules/grid';

export interface GridNodesParams extends CommandParams {
}

export interface GridNodesResult extends CommandResult {
	nodes: GridNode[];
}
