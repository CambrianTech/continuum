/**
 * RustCoreIPC Base - Core connection and request logic
 *
 * This is the foundation that all domain modules build upon.
 * Contains: socket connection, binary framing, request/response handling.
 */

import net from 'net';
import path from 'path';
import { EventEmitter } from 'events';
import { SOCKETS } from '../../../../shared/config';

/**
 * Resolve socket path to absolute path.
 */
export function resolveSocketPath(socketPath: string): string {
	if (path.isAbsolute(socketPath)) {
		return socketPath;
	}
	return path.resolve(process.cwd(), socketPath);
}

/**
 * Get the default continuum-core socket path (resolved to absolute).
 */
export function getContinuumCoreSocketPath(): string {
	return resolveSocketPath(SOCKETS.CONTINUUM_CORE);
}

/** JSON response from IPC */
export interface IPCJsonResponse {
	success: boolean;
	result?: any;
	error?: string;
	requestId?: number;
}

/** Full IPC response including optional binary payload */
export interface IPCResponse {
	response: IPCJsonResponse;
	binaryData?: Buffer;
}

/**
 * Base IPC Client - Core connection and request logic only.
 * Domain-specific methods are added via mixins.
 *
 * NOTE: All instance members are public due to TypeScript mixin limitations.
 * Members prefixed with _ are internal and should not be used directly.
 */
export class RustCoreIPCClientBase extends EventEmitter {
	// Internal members (public for mixin compatibility, but treat as private)
	public _socket: net.Socket | null = null;
	public _buffer: Buffer = Buffer.alloc(0);
	public _pendingRequests: Map<number, { resolve: (result: IPCResponse) => void; reject: (err: Error) => void; timer: ReturnType<typeof setTimeout> }> = new Map();
	public _nextRequestId = 1;
	public _connected = false;
	public _socketPath: string;

	/** Rate-limit slow IPC warnings */
	private static slowWarningTimestamps: Map<string, number> = new Map();
	private static readonly SLOW_IPC_THRESHOLD_MS = 500;
	private static readonly SLOW_WARNING_COOLDOWN_MS = 10_000;

	/** Default timeout for IPC requests (60s — generous for heavy TTS synthesis) */
	private static readonly REQUEST_TIMEOUT_MS = 60_000;

	constructor(socketPath: string) {
		super();
		this._socketPath = socketPath;
	}

	/** Whether the client is currently connected */
	get connected(): boolean {
		return this._connected;
	}

	/** The socket path this client connects to */
	get socketPath(): string {
		return this._socketPath;
	}

	/**
	 * Connect to continuum-core server
	 */
	async connect(): Promise<void> {
		if (this._connected) {
			return;
		}

		return new Promise((resolve, reject) => {
			this._socket = net.createConnection(this._socketPath);

			this._socket.on('connect', () => {
				this._connected = true;
				this.emit('connect');
				resolve();
			});

			this._socket.on('data', (data: Buffer) => {
				this._onData(data);
			});

			this._socket.on('error', (err) => {
				this._rejectAllPending(err instanceof Error ? err : new Error(String(err)));
				this.emit('error', err);
				reject(err);
			});

			this._socket.on('close', () => {
				this._connected = false;
				this._rejectAllPending(new Error('IPC socket closed'));
				this.emit('close');
			});
		});
	}

	/**
	 * Process incoming binary data using length-prefixed framing.
	 * @internal
	 */
	public _onData(data: Buffer): void {
		this._buffer = Buffer.concat([this._buffer, data]);

		while (this._buffer.length >= 4) {
			const totalLength = this._buffer.readUInt32BE(0);
			const frameEnd = 4 + totalLength;

			if (this._buffer.length < frameEnd) {
				break;
			}

			const payload = this._buffer.subarray(4, frameEnd);
			this._buffer = this._buffer.subarray(frameEnd);

			const separatorIndex = payload.indexOf(0);

			let jsonBytes: Buffer;
			let binaryData: Buffer | undefined;

			if (separatorIndex !== -1) {
				jsonBytes = payload.subarray(0, separatorIndex);
				binaryData = payload.subarray(separatorIndex + 1);
			} else {
				jsonBytes = payload;
			}

			try {
				const response: IPCJsonResponse = JSON.parse(jsonBytes.toString('utf8'));
				this._handleResponse(response, binaryData);
			} catch (e) {
				console.error('Failed to parse IPC response JSON:', e);
			}
		}
	}

	/**
	 * @internal
	 */
	public _handleResponse(response: IPCJsonResponse, binaryData?: Buffer): void {
		if (response.requestId !== undefined) {
			const pending = this._pendingRequests.get(response.requestId);
			if (pending) {
				clearTimeout(pending.timer);
				this._pendingRequests.delete(response.requestId);
				pending.resolve({ response, binaryData });
			}
		}
	}

	/**
	 * Reject all pending requests (called on socket close/error).
	 */
	public _rejectAllPending(err: Error): void {
		for (const [_id, pending] of this._pendingRequests) {
			clearTimeout(pending.timer);
			pending.reject(err);
		}
		this._pendingRequests.clear();
	}

	/**
	 * Ensure connected before making request.
	 * @internal
	 */
	public async _ensureConnected(): Promise<void> {
		if (!this._connected || !this._socket) {
			throw new Error('Not connected to continuum-core server');
		}
	}

	/**
	 * Send a request and wait for full response (including optional binary data).
	 */
	async requestFull(command: any, timeoutMs?: number): Promise<IPCResponse> {
		await this._ensureConnected();

		const requestId = this._nextRequestId++;
		const requestWithId = { ...command, requestId };
		const timeout = timeoutMs ?? RustCoreIPCClientBase.REQUEST_TIMEOUT_MS;

		return new Promise((resolve, reject) => {
			const json = JSON.stringify(requestWithId) + '\n';
			const start = performance.now();

			const timer = setTimeout(() => {
				this._pendingRequests.delete(requestId);
				reject(new Error(`IPC timeout: ${command.command} did not respond within ${timeout}ms`));
			}, timeout);

			this._pendingRequests.set(requestId, {
				resolve: (result) => {
					const duration = performance.now() - start;
					if (duration > RustCoreIPCClientBase.SLOW_IPC_THRESHOLD_MS) {
						const now = Date.now();
						const lastWarned = RustCoreIPCClientBase.slowWarningTimestamps.get(command.command) ?? 0;
						if (now - lastWarned > RustCoreIPCClientBase.SLOW_WARNING_COOLDOWN_MS) {
							RustCoreIPCClientBase.slowWarningTimestamps.set(command.command, now);
							console.warn(`⚠️  Slow IPC call: ${command.command} took ${duration.toFixed(0)}ms`);
						}
					}
					resolve(result);
				},
				reject,
				timer,
			});

			this._socket!.write(json, (err) => {
				if (err) {
					clearTimeout(timer);
					this._pendingRequests.delete(requestId);
					reject(err);
				}
			});
		});
	}

	/**
	 * Send a request and wait for JSON response (ignores binary payload).
	 */
	async request(command: any): Promise<IPCJsonResponse> {
		const { response } = await this.requestFull(command);
		return response;
	}

	/**
	 * Execute any Rust command by name.
	 */
	async execute<T = any>(commandName: string, params: Record<string, any> = {}): Promise<{ success: boolean; data?: T; error?: string }> {
		const response = await this.request({
			command: commandName,
			...params,
		});

		if (!response.success) {
			return {
				success: false,
				error: response.error || `Command '${commandName}' failed`,
			};
		}

		return {
			success: true,
			data: response.result as T,
		};
	}

	/**
	 * Health check
	 */
	async healthCheck(): Promise<boolean> {
		const response = await this.request({ command: 'health-check' });
		return response.success && response.result?.healthy === true;
	}

	/**
	 * Disconnect from server
	 */
	disconnect(): void {
		if (this._socket) {
			this._socket.destroy();
			this._socket = null;
			this._connected = false;
		}
	}
}
