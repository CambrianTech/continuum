/**
 * RustCoreIPC Grid Module — inter-node transport and routing.
 *
 * Thin TypeScript wrapper over Rust GridModule.
 * All logic is in Rust (modules/grid/). This is just the IPC bridge.
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	GridNode as RustGridNode,
	TransportAddress as RustTransportAddress,
	NodeCapability as RustNodeCapability,
	TrustLevel as RustTrustLevel,
} from '../../../../protocol/typescript/grid';

// Re-export generated types (already camelCase from ts-rs)
export type { RustGridNode as GridNode, RustTransportAddress as TransportAddress, RustNodeCapability as NodeCapability, RustTrustLevel as TrustLevel };

// ============================================================================
// Response types
// ============================================================================

export interface GridStatusResponse {
	transports: Array<{
		name: string;
		connected: boolean;
		address: string | null;
		encrypted: boolean;
	}>;
	totalNodes: number;
	onlineNodes: number;
	gridDir: string;
}

export interface GridPingResponse {
	nodeId: string;
	nodeName: string | null;
	latencyMs: number;
	transport: string;
	responseType: string;
}

export interface GridDiscoverResponse {
	totalDiscovered: number;
	transports: Array<{
		transport: string;
		discovered?: number;
		error?: string;
	}>;
}

export interface GridPairResponse {
	paired: boolean;
	nodeId: string;
	trustLevel: string;
}

export interface GridRouteResponse {
	route: 'local' | 'remote';
	nodeId?: string;
	nodeName?: string | null;
	reason: string;
}

export interface GridSetupCheckResponse {
	ready: boolean;
	tailscaleIp: string | null;
	dnsName: string | null;
	peerCount: number;
	checks: Array<{
		check: string;
		status: 'pass' | 'fail' | 'warn' | 'info' | 'skip';
		detail: string;
		peers?: string[];
	}>;
	actions: string[];
	summary: string;
}

// ============================================================================
// Mixin
// ============================================================================

export interface GridMixin {
	gridStatus(): Promise<GridStatusResponse>;
	gridNodes(): Promise<RustGridNode[]>;
	gridPing(nodeId: string): Promise<GridPingResponse>;
	gridSend(nodeId: string, command: string, params?: Record<string, unknown>): Promise<unknown>;
	gridDiscover(): Promise<GridDiscoverResponse>;
	gridPair(address: string, name?: string, trust?: string, gpu?: string, vramMb?: number): Promise<GridPairResponse>;
	gridTrust(nodeId: string, trust: string): Promise<{ nodeId: string; trustLevel: string }>;
	gridAudit(limit?: number): Promise<unknown[]>;
	gridRoute(command: string, routingHint?: string): Promise<GridRouteResponse>;
	gridSetupCheck(): Promise<GridSetupCheckResponse>;
}

export function GridMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements GridMixin {
		async gridStatus(): Promise<GridStatusResponse> {
			const response = await this.request({ command: 'grid/status' });
			if (!response.success) throw new Error(response.error || 'grid/status failed');
			return response.result as GridStatusResponse;
		}

		async gridNodes(): Promise<RustGridNode[]> {
			const response = await this.request({ command: 'grid/nodes' });
			if (!response.success) throw new Error(response.error || 'grid/nodes failed');
			return response.result as RustGridNode[];
		}

		async gridPing(nodeId: string): Promise<GridPingResponse> {
			const response = await this.request({ command: 'grid/ping', nodeId });
			if (!response.success) throw new Error(response.error || 'grid/ping failed');
			return response.result as GridPingResponse;
		}

		async gridSend(nodeId: string, remoteCommand: string, remoteParams?: Record<string, unknown>): Promise<unknown> {
			// IPC uses 'command' for routing. Rust handler reads remaining fields as params.
			// We pass the remote command name as 'remoteCommand' to avoid TS object literal collision.
			const response = await this.request({
				command: 'grid/send', nodeId, remoteCommand, params: remoteParams,
			});
			if (!response.success) throw new Error(response.error || 'grid/send failed');
			return response.result;
		}

		async gridDiscover(): Promise<GridDiscoverResponse> {
			const response = await this.request({ command: 'grid/discover' });
			if (!response.success) throw new Error(response.error || 'grid/discover failed');
			return response.result as GridDiscoverResponse;
		}

		async gridPair(address: string, name?: string, trust?: string, gpu?: string, vramMb?: number): Promise<GridPairResponse> {
			const response = await this.request({ command: 'grid/pair', address, name, trust, gpu, vramMb });
			if (!response.success) throw new Error(response.error || 'grid/pair failed');
			return response.result as GridPairResponse;
		}

		async gridTrust(nodeId: string, trust: string): Promise<{ nodeId: string; trustLevel: string }> {
			const response = await this.request({ command: 'grid/trust', nodeId, trust });
			if (!response.success) throw new Error(response.error || 'grid/trust failed');
			return response.result as { nodeId: string; trustLevel: string };
		}

		async gridAudit(limit?: number): Promise<unknown[]> {
			const response = await this.request({ command: 'grid/audit', limit });
			if (!response.success) throw new Error(response.error || 'grid/audit failed');
			return response.result as unknown[];
		}

		async gridRoute(targetCommand: string, routingHint?: string): Promise<GridRouteResponse> {
			const response = await this.request({
				command: 'grid/route', targetCommand, routingHint,
			});
			if (!response.success) throw new Error(response.error || 'grid/route failed');
			return response.result as GridRouteResponse;
		}

		async gridSetupCheck(): Promise<GridSetupCheckResponse> {
			const response = await this.request({ command: 'grid/setup-check' });
			if (!response.success) throw new Error(response.error || 'grid/setup-check failed');
			return response.result as GridSetupCheckResponse;
		}
	};
}
