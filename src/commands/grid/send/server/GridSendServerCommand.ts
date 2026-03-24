/**
 * Grid Send Command - Server Implementation
 *
 * Executes a command on a remote grid node via Rust GridModule IPC.
 * Unwraps the remote response to extract the actual result,
 * stripping transport envelope (context, sessionId, etc.).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridSendParams, GridSendResult } from '../shared/GridSendTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridSendServerCommand extends CommandBase<GridSendParams, GridSendResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/send', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridSendParams): Promise<GridSendResult> {
		await this.rustClient.connect();
		const rawResult = await this.rustClient.gridSend(params.nodeId, params.remoteCommand, params.params);

		// The Rust grid returns the remote node's full response including
		// transport envelope (context, sessionId, server info, etc.).
		// Extract the actual command result, stripping the envelope.
		const remoteResult = this.unwrapRemoteResult(rawResult);

		return {
			success: true,
			remoteResult,
			nodeId: params.nodeId,
			remoteCommand: params.remoteCommand,
		} as unknown as GridSendResult;
	}

	/**
	 * Unwrap the remote node's response, stripping transport metadata.
	 *
	 * The raw result from Rust typically contains:
	 *   { context: {...}, server: {...}, browser: {...}, success: true, ...actualData }
	 * or for command results:
	 *   { context: {...}, sessionId: ..., success: true, items: [...], ...actualData }
	 *
	 * We only strip keys whose VALUES are clearly transport metadata (objects with
	 * known transport fields), not by key name alone — a legitimate result could
	 * have a field called 'server' or 'timestamp'.
	 */
	private unwrapRemoteResult(raw: unknown): unknown {
		if (raw == null || typeof raw !== 'object') return raw;

		const obj = raw as Record<string, unknown>;

		// Only strip 'context' — it's always a JTAGContext envelope and never a legitimate result field.
		// 'server' and 'browser' are ping-specific response sections — strip only if they look like
		// transport metadata (have 'type'+'runtime' or 'type'+'platform' fields).
		const cleaned: Record<string, unknown> = {};
		for (const [key, value] of Object.entries(obj)) {
			if (key === 'context' && this.isTransportEnvelope(value)) {
				continue; // Always strip JTAGContext
			}
			if ((key === 'server' || key === 'browser') && this.isTransportMetadata(value)) {
				continue; // Strip ping-style server/browser info blocks
			}
			cleaned[key] = value;
		}

		return cleaned;
	}

	/** Check if value looks like a JTAGContext (has uuid + environment fields) */
	private isTransportEnvelope(value: unknown): boolean {
		if (value == null || typeof value !== 'object') return false;
		const obj = value as Record<string, unknown>;
		return 'uuid' in obj && 'environment' in obj;
	}

	/** Check if value looks like transport metadata (ping server/browser blocks) */
	private isTransportMetadata(value: unknown): boolean {
		if (value == null || typeof value !== 'object') return false;
		const obj = value as Record<string, unknown>;
		return 'type' in obj && ('runtime' in obj || 'platform' in obj) && 'packageName' in obj;
	}
}
