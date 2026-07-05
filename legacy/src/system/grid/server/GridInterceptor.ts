/**
 * Grid Routing Interceptor — hooks Commands.execute() into the Grid.
 *
 * Registered at server startup. Before any command executes locally,
 * this interceptor checks the Rust GridRouter:
 * - If the router says "remote" → forwards via grid/send → returns remote result
 * - If the router says "local" → returns null → command executes locally as normal
 *
 * This makes Grid routing completely transparent. Calling code doesn't change.
 * genome/train on a Mac with no GPU auto-routes to the 5090 tower.
 */

import type { GridRoutingInterceptor } from '@system/core/shared/Commands';
import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridInterceptor implements GridRoutingInterceptor {
	private rustClient: RustCoreIPCClient;
	private _connected = false;

	constructor() {
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async tryRouteRemote<T extends CommandParams, U extends CommandResult>(
		command: string,
		params: Partial<T> | undefined,
	): Promise<U | null> {
		if (!this._connected) {
			try {
				await this.rustClient.connect();
				this._connected = true;
			} catch {
				// Rust core not available — execute locally
				return null;
			}
		}

		try {
			// Check the Rust GridRouter: should this command run remotely?
			// Pass the command name and any routingHint from params.
			const routingHint = (params as Record<string, unknown>)?.routingHint as string | undefined;
			const nodeId = (params as Record<string, unknown>)?.nodeId as string | undefined;

			const route = await this.rustClient.gridRoute(
				command,
				routingHint,
			);

			if (route.route === 'local') {
				return null; // Execute locally
			}

			// Route remotely via grid/send
			const targetNodeId = nodeId || route.nodeId;
			if (!targetNodeId) {
				return null; // No target node — execute locally
			}

			// Strip grid-internal params before forwarding
			const forwardParams = { ...params } as Record<string, unknown>;
			delete forwardParams.routingHint;
			delete forwardParams.nodeId;

			const result = await this.rustClient.gridSend(targetNodeId, command, forwardParams);
			return result as U;
		} catch {
			// Grid routing failed — fall back to local execution
			return null;
		}
	}
}
