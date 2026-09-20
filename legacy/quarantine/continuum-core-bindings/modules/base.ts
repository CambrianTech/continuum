/**
 * RustCoreIPC Base - Core connection and request logic
 *
 * This is the foundation that all domain modules build upon.
 * Contains: socket connection, binary framing, request/response handling.
 */

import net from 'net';
import path from 'path';
import { EventEmitter } from 'events';
import { SOCKETS } from '../../../../src/shared/config';

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

/**
 * Continuum-core endpoint resolution.
 *
 * Where the TypeScript side of continuum connects to the Rust continuum-core
 * server. Two transport options:
 *   - Unix domain socket (path like /Users/.../continuum-core.sock) — primary
 *     path when native TS can reach the same filesystem as the Rust server.
 *   - TCP (host:port) — required when the TS side is inside a Docker container
 *     and the Rust server runs native on the host Mac, because Unix sockets
 *     don't traverse Docker Desktop's VM boundary.
 *
 * Controlled by the CONTINUUM_CORE_URL env var (set by install.sh / compose):
 *   tcp://host:port   → TCP connection
 *   unix:///path.sock → explicit Unix override
 *   (unset)           → default Unix path from SOCKETS.CONTINUUM_CORE
 *
 * The wire protocol is the same either way — length-prefixed JSON, matching
 * the Rust IPC server's IpcStream trait (which accepts both transports).
 */
export interface CoreEndpoint {
	/** Unix domain socket path (set when isTcp=false) */
	path?: string;
	/** TCP host (set when isTcp=true) */
	host?: string;
	/** TCP port (set when isTcp=true) */
	port?: number;
	/** Which transport — tells callers which `net.createConnection` overload to use */
	isTcp: boolean;
	/** Human-readable description for logging */
	description: string;
}

/**
 * Resolve the continuum-core endpoint from CONTINUUM_CORE_URL env (if set)
 * or fall back to the given default Unix socket path.
 */
export function resolveCoreEndpoint(defaultPath: string = SOCKETS.CONTINUUM_CORE): CoreEndpoint {
	const envUrl = process.env.CONTINUUM_CORE_URL;
	if (envUrl) {
		const tcpMatch = envUrl.match(/^tcp:\/\/([^:/]+):(\d+)$/);
		if (tcpMatch) {
			const host = tcpMatch[1];
			const port = parseInt(tcpMatch[2], 10);
			return { host, port, isTcp: true, description: `tcp://${host}:${port}` };
		}
		// Unix override via env. Supports bare path or unix://path form.
		const unixPath = envUrl.replace(/^unix:\/\//, '');
		const resolved = resolveSocketPath(unixPath);
		return { path: resolved, isTcp: false, description: resolved };
	}
	const resolved = resolveSocketPath(defaultPath);
	return { path: resolved, isTcp: false, description: resolved };
}

/**
 * Open a net.Socket to the continuum-core endpoint — picks TCP vs Unix per
 * the endpoint.isTcp flag. Protocol is identical either way.
 */
export function connectToCoreEndpoint(endpoint: CoreEndpoint): net.Socket {
	if (endpoint.isTcp) {
		return net.createConnection({ host: endpoint.host!, port: endpoint.port! });
	}
	return net.createConnection(endpoint.path!);
}

/**
 * Resolve continuum-core endpoint as a single socketPath-style string.
 * Returns `tcp://host:port` if CONTINUUM_CORE_URL is a tcp URL, otherwise
 * the absolute Unix socket path. Used by callers that store `socketPath` as
 * a single string config field — they pass the result straight through to
 * `connectToSocketPathOrUrl` at connection time.
 */
export function resolveCoreEndpointString(defaultPath: string = SOCKETS.CONTINUUM_CORE): string {
	const endpoint = resolveCoreEndpoint(defaultPath);
	if (endpoint.isTcp) return `tcp://${endpoint.host}:${endpoint.port}`;
	return endpoint.path!;
}

/**
 * Open a net.Socket given either a Unix socket filesystem path or a
 * `tcp://host:port` URL string. String-oriented twin of `connectToCoreEndpoint`
 * — fits existing callers that carry one `socketPath: string` through layers
 * of config. Protocol over the wire is identical either way.
 */
export function connectToSocketPathOrUrl(socketPathOrUrl: string): net.Socket {
	const tcpMatch = socketPathOrUrl.match(/^tcp:\/\/([^:/]+):(\d+)$/);
	if (tcpMatch) {
		return net.createConnection({ host: tcpMatch[1], port: parseInt(tcpMatch[2], 10) });
	}
	return net.createConnection(socketPathOrUrl);
}

/** JSON response from IPC — result is untyped at the wire boundary (JSON.parse output) */
export interface IPCJsonResponse {
	success: boolean;
	// eslint-disable-next-line @typescript-eslint/no-explicit-any -- true wire boundary: raw JSON.parse result
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
	// Public due to TypeScript mixin limitations — treat as private (prefixed with _)
	public _reconnectAttempts = 0;
	public _reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	public _wasConnected = false;
	public _reconnectPromise: Promise<void> | null = null;
	public _reconnectResolve: (() => void) | null = null;

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
	 * Connect to continuum-core server.
	 * Auto-reconnects with exponential backoff if the connection drops
	 * after a successful initial connection (e.g. core container restart).
	 */
	async connect(): Promise<void> {
		if (this._connected) {
			return;
		}

		return new Promise((resolve, reject) => {
			// Honor CONTINUUM_CORE_URL (tcp://... for containerized callers on
			// Mac) over the default Unix path. Protocol identical either way.
			const endpoint = resolveCoreEndpoint(this._socketPath);
			this._socket = connectToCoreEndpoint(endpoint);

			this._socket.on('connect', () => {
				this._connected = true;
				this._wasConnected = true;
				this._reconnectAttempts = 0;
				if (this._reconnectResolve) {
					this._reconnectResolve();
					this._reconnectResolve = null;
					this._reconnectPromise = null;
				}
				this.emit('connect');
				resolve();
			});

			this._socket.on('data', (data: Buffer) => {
				this._onData(data);
			});

			this._socket.on('error', (err) => {
				this._connected = false;
				this._rejectAllPending(err instanceof Error ? err : new Error(String(err)));
				this.emit('connection-error', err);
				// Always reject THIS connect() promise on socket error.
				// Promise.reject is a no-op if already settled, so this is
				// safe for both initial connects + post-reconnect calls.
				//
				// Pre-fix this only rejected when !_wasConnected, which left
				// reconnect attempts hanging forever — `await this.connect()`
				// in _scheduleReconnect's try/catch never resolved or
				// rejected when the backend was dead, so the catch block
				// (which increments _reconnectAttempts + reschedules) never
				// fired. Counter stuck at 1 + no further reconnect attempts.
				// Carl's #980 Bug 4 sub-bug: "[IPC] Reconnecting to
				// continuum-core in 1000ms (attempt 1)" repeated forever.
				reject(err);
			});

			this._socket.on('close', () => {
				this._connected = false;
				this._socket = null;
				this._rejectAllPending(new Error('IPC socket closed'));
				this.emit('close');
				// Auto-reconnect if we were previously connected (core restarted)
				if (this._wasConnected) {
					this._scheduleReconnect();
				}
			});
		});
	}

	/**
	 * Schedule reconnection with exponential backoff.
	 * Creates a promise that _ensureConnected() can await.
	 */
	public _scheduleReconnect(): void {
		if (this._reconnectTimer) return;
		if (!this._reconnectPromise) {
			this._reconnectPromise = new Promise<void>(resolve => {
				this._reconnectResolve = resolve;
			});
		}
		const delay = Math.min(1000 * Math.pow(2, this._reconnectAttempts), 15000);
		console.log(`[IPC] Reconnecting to continuum-core in ${delay}ms (attempt ${this._reconnectAttempts + 1})`);
		this._reconnectTimer = setTimeout(async () => {
			this._reconnectTimer = null;
			try {
				await this.connect();
				console.log('[IPC] Reconnected to continuum-core');
			} catch {
				this._reconnectAttempts++;
				if (this._reconnectAttempts < 20) {
					this._scheduleReconnect();
				} else {
					console.error('[IPC] Gave up reconnecting to continuum-core after 20 attempts');
					if (this._reconnectResolve) {
						// Don't leave waiters hanging forever — they'll get "not connected" errors
						this._reconnectResolve();
						this._reconnectResolve = null;
						this._reconnectPromise = null;
					}
				}
			}
		}, delay);
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
	 * If reconnecting (core restarted), waits up to 15s for the connection
	 * to come back rather than failing immediately.
	 * @internal
	 */
	public async _ensureConnected(): Promise<void> {
		if (this._connected && this._socket) return;

		// If a reconnection is in progress, wait for it
		if (this._reconnectPromise) {
			await Promise.race([
				this._reconnectPromise,
				new Promise<void>((_, reject) =>
					setTimeout(() => reject(new Error('Timed out waiting for continuum-core reconnection (15s)')), 15_000)
				),
			]);
			if (this._connected && this._socket) return;
		}

		throw new Error('Not connected to continuum-core server');
	}

	/**
	 * Send a request and wait for full response (including optional binary data).
	 */
	async requestFull(command: Record<string, unknown>, timeoutMs?: number): Promise<IPCResponse> {
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
					// Slow IPC tracked via metrics — no stdout spam
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
	async request(command: Record<string, unknown>): Promise<IPCJsonResponse> {
		const { response } = await this.requestFull(command);
		return response;
	}

	/**
	 * Execute any Rust command by name.
	 */
	async execute<T = unknown>(commandName: string, params: Record<string, unknown> = {}): Promise<{ success: boolean; data?: T; error?: string }> {
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
