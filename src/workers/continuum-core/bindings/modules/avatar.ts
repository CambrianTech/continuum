/**
 * RustCoreIPC Avatar Module - Bevy 3D avatar snapshots
 *
 * Captures a single-frame PNG from the Bevy renderer for use as
 * profile pictures. The snapshot is saved to ~/.continuum/avatars/
 * and served via HTTP at /avatars/{identity}.png.
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface AvatarSnapshotResult {
	/** Relative URL path: "/avatars/{identity}.png" */
	path: string;
	/** Whether the result came from disk cache */
	cached: boolean;
}

// ============================================================================
// Mixin
// ============================================================================

export interface AvatarMixin {
	avatarSnapshot(identity: string, width?: number, height?: number, force?: boolean): Promise<AvatarSnapshotResult>;
}

export function AvatarMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements AvatarMixin {
		/**
		 * Capture a Bevy 3D avatar snapshot as PNG.
		 * Returns the URL path to the saved image file.
		 * Cached on disk — subsequent calls return immediately unless force=true.
		 */
		async avatarSnapshot(
			identity: string,
			width?: number,
			height?: number,
			force?: boolean,
		): Promise<AvatarSnapshotResult> {
			const response = await this.request({
				command: 'avatar/snapshot',
				identity,
				...(width !== undefined && { width }),
				...(height !== undefined && { height }),
				...(force !== undefined && { force }),
			});
			if (!response.success) throw new Error(response.error || 'Avatar snapshot failed');
			const result = response.result as { path: string; cached: boolean };
			return { path: result.path, cached: result.cached };
		}
	};
}
