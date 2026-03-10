/**
 * Sentinel Mixin - TypeScript wrapper for Rust SentinelModule
 *
 * Routes process execution through Rust for:
 * - Process isolation (kill_on_drop prevents zombie processes)
 * - Concurrent execution limits
 * - Real-time log streaming to files
 * - Fault tolerance (one sentinel crash doesn't cascade)
 */

import type { RustCoreIPCClientBase } from './base';

/**
 * Sentinel execution handle
 */
export interface SentinelHandle {
	id: string;
	sentinelType: string;
	status: 'running' | 'completed' | 'failed' | 'cancelled';
	progress: number;
	startTime: number;
	endTime?: number;
	exitCode?: number;
	error?: string;
	workingDir: string;
	logsDir: string;
}

/**
 * Log stream info
 */
export interface LogStreamInfo {
	name: string;
	path: string;
	size: number;
	modifiedAt: string;
}

/**
 * Sentinel run parameters
 */
export interface SentinelRunParams {
	/** Command to execute (e.g., "npm", "cargo", "xcodebuild") */
	command: string;
	/** Arguments for the command */
	args?: string[];
	/** Working directory */
	workingDir?: string;
	/** Environment variables */
	env?: Record<string, string>;
	/** Timeout in seconds (default: 600 = 10 minutes) */
	timeout?: number;
	/** Sentinel type for categorization (default: "build") */
	type?: string;
	/** Parent persona ID for escalation routing on completion */
	parentPersonaId?: string;
	/** Entity ID for execution record persistence */
	entityId?: string;
	/** Human-readable sentinel name */
	sentinelName?: string;
	/** Escalation rules (e.g., retry, notify, block) */
	escalationRules?: Record<string, unknown>;
}

/**
 * Sentinel run result
 */
export interface SentinelRunResult {
	handle: string;
	status: string;
	logsDir: string;
}

/**
 * Sentinel status result
 */
export interface SentinelStatusResult {
	handle: SentinelHandle;
}

/**
 * Sentinel list result
 */
export interface SentinelListResult {
	handles: SentinelHandle[];
	total: number;
}

/**
 * Sentinel logs list result
 */
export interface SentinelLogsListResult {
	handle: string;
	logsDir: string;
	streams: LogStreamInfo[];
}

/**
 * Sentinel logs read result
 */
export interface SentinelLogsReadResult {
	handle: string;
	stream: string;
	content: string;
	lineCount: number;
	totalLines: number;
	offset: number;
	truncated: boolean;
}

/**
 * Sentinel logs tail result
 */
export interface SentinelLogsTailResult {
	handle: string;
	stream: string;
	content: string;
	lineCount: number;
	totalLines: number;
}

/**
 * Pipeline step types for multi-step execution
 */
export type PipelineStep =
	| { type: 'shell'; cmd: string; args?: string[]; timeoutSecs?: number; workingDir?: string; allowFailure?: boolean; env?: Record<string, string> }
	| { type: 'llm'; prompt: string; model?: string; provider?: string; maxTokens?: number; temperature?: number; systemPrompt?: string; tools?: string[]; agentMode?: boolean; maxIterations?: number; activeAdapters?: Array<{ name: string; path: string; domain?: string; scale?: number }> }
	| { type: 'command'; command: string; params?: Record<string, unknown> }
	| { type: 'condition'; if: string; then: PipelineStep[]; else?: PipelineStep[] }
	| { type: 'loop'; count?: number; while?: string; until?: string; steps: PipelineStep[]; maxIterations?: number }
	| { type: 'emit'; event: string; payload?: Record<string, unknown> }
	| { type: 'watch'; event: string; timeoutSecs?: number }
	| { type: 'parallel'; branches: PipelineStep[][]; failFast?: boolean }
	| { type: 'sentinel'; pipeline: Pipeline }
	| { type: 'codingagent'; prompt: string; provider?: string; workingDir?: string; systemPrompt?: string; model?: string; allowedTools?: string[]; maxTurns?: number; maxBudgetUsd?: number; permissionMode?: string; resumeSessionId?: string; captureTraining?: boolean; personaId?: string };

/**
 * Pipeline definition
 */
export interface Pipeline {
	name?: string;
	steps: PipelineStep[];
	workingDir?: string;
	timeoutSecs?: number;
	inputs?: Record<string, unknown>;
}

/**
 * Step result from pipeline execution
 */
export interface StepResult {
	stepIndex: number;
	stepType: string;
	success: boolean;
	durationMs: number;
	output?: string;
	error?: string;
	exitCode?: number;
	data?: unknown;
}

/**
 * Pipeline execution result
 */
export interface PipelineResult {
	handle: string;
	success: boolean;
	totalDurationMs: number;
	stepsCompleted: number;
	stepsTotal: number;
	stepResults: StepResult[];
	error?: string;
}

/**
 * Mixin interface for type-safety
 */
export interface SentinelMixin {
	sentinelRun(params: SentinelRunParams): Promise<SentinelRunResult>;
	sentinelExecute(params: SentinelRunParams): Promise<{ success: boolean; exitCode: number; output: string; handle: string }>;
	sentinelStatus(handle: string): Promise<SentinelStatusResult>;
	sentinelList(): Promise<SentinelListResult>;
	sentinelCancel(handle: string): Promise<{ handle: string; status: string }>;
	sentinelLogsList(handle: string): Promise<SentinelLogsListResult>;
	sentinelLogsRead(handle: string, stream?: string, offset?: number, limit?: number): Promise<SentinelLogsReadResult>;
	sentinelLogsTail(handle: string, stream?: string, lines?: number): Promise<SentinelLogsTailResult>;
	sentinelPipeline(pipeline: Pipeline): Promise<PipelineResult>;
}

/**
 * Mixin that adds sentinel methods to the IPC client.
 */
export function SentinelMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements SentinelMixin {
		/**
		 * Execute a command via Rust SentinelModule with full process isolation.
		 *
		 * Returns immediately with a handle. Use sentinelStatus() to check progress.
		 * Logs are written to .sentinel-workspaces/{handle}/logs/
		 */
		async sentinelRun(params: SentinelRunParams): Promise<SentinelRunResult> {
			const response = await this.request({
				command: 'sentinel/run',
				type: params.type || 'build',
				cmd: params.command, // 'cmd' in Rust (avoids collision with IPC 'command' field)
				args: params.args || [],
				workingDir: params.workingDir,
				env: params.env,
				timeout: params.timeout,
				// Escalation metadata — Rust stores alongside handle, pushes on completion
				...(params.parentPersonaId && { parentPersonaId: params.parentPersonaId }),
				...(params.entityId && { entityId: params.entityId }),
				...(params.sentinelName && { sentinelName: params.sentinelName }),
				...(params.escalationRules && { escalationRules: params.escalationRules }),
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/run failed');
			}

			return response.result as SentinelRunResult;
		}

		/**
		 * Execute a command synchronously (waits for completion).
		 *
		 * Uses Rust sentinel/await (tokio::sync::watch channel) — zero polling.
		 * Single IPC call that blocks until the sentinel finishes.
		 */
		async sentinelExecute(params: SentinelRunParams): Promise<{
			success: boolean;
			exitCode: number;
			output: string;
			handle: string;
		}> {
			const runResult = await this.sentinelRun(params);
			const handle = runResult.handle;
			const timeout = params.timeout || 600;

			// Single IPC call — Rust awaits watch channel, no polling.
			// Use requestFull with explicit timeout (default IPC timeout is 60s, sentinels can run for minutes).
			const ipcTimeoutMs = timeout * 1000 + 5000; // sentinel timeout + 5s grace
			const { response: awaitResponse } = await this.requestFull({
				command: 'sentinel/await',
				handle,
				timeout,
			}, ipcTimeoutMs);

			if (!awaitResponse.success) {
				return {
					success: false,
					exitCode: -1,
					output: awaitResponse.error || `Await failed for ${handle}`,
					handle,
				};
			}

			const finalHandle = (awaitResponse.result as SentinelStatusResult).handle;

			// Read output from logs
			let output = '';
			try {
				const logs = await this.sentinelLogsTail(handle, 'combined', 10000);
				output = logs.content;
			} catch {
				// Pipeline-type sentinels don't produce combined log streams
			}

			if (!output) {
				try {
					const stepsLog = await this.sentinelLogsRead(handle, 'steps', undefined, undefined);
					if (stepsLog.content) {
						const lines = stepsLog.content.trim().split('\n');
						const lastStep = JSON.parse(lines[lines.length - 1]);
						output = lastStep.output || finalHandle.error || '';
					}
				} catch {
					output = finalHandle.error || '';
				}
			}

			return {
				success: finalHandle.status === 'completed' && (finalHandle.exitCode === 0 || finalHandle.exitCode === undefined),
				exitCode: finalHandle.exitCode ?? (finalHandle.status === 'completed' ? 0 : -1),
				output,
				handle,
			};
		}

		/**
		 * Get status of a sentinel by handle
		 */
		async sentinelStatus(handle: string): Promise<SentinelStatusResult> {
			const response = await this.request({
				command: 'sentinel/status',
				handle,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/status failed');
			}

			return response.result as SentinelStatusResult;
		}

		/**
		 * List all sentinel handles
		 */
		async sentinelList(): Promise<SentinelListResult> {
			const response = await this.request({
				command: 'sentinel/list',
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/list failed');
			}

			return response.result as SentinelListResult;
		}

		/**
		 * Cancel a running sentinel
		 */
		async sentinelCancel(handle: string): Promise<{ handle: string; status: string }> {
			const response = await this.request({
				command: 'sentinel/cancel',
				handle,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/cancel failed');
			}

			return response.result as { handle: string; status: string };
		}

		/**
		 * List log streams for a sentinel
		 */
		async sentinelLogsList(handle: string): Promise<SentinelLogsListResult> {
			const response = await this.request({
				command: 'sentinel/logs/list',
				handle,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/logs/list failed');
			}

			return response.result as SentinelLogsListResult;
		}

		/**
		 * Read log content from a sentinel
		 */
		async sentinelLogsRead(
			handle: string,
			stream: string = 'combined',
			offset: number = 0,
			limit: number = 1000
		): Promise<SentinelLogsReadResult> {
			const response = await this.request({
				command: 'sentinel/logs/read',
				handle,
				stream,
				offset,
				limit,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/logs/read failed');
			}

			return response.result as SentinelLogsReadResult;
		}

		/**
		 * Tail the last N lines of a log stream
		 */
		async sentinelLogsTail(
			handle: string,
			stream: string = 'combined',
			lines: number = 20
		): Promise<SentinelLogsTailResult> {
			const response = await this.request({
				command: 'sentinel/logs/tail',
				handle,
				stream,
				lines,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/logs/tail failed');
			}

			return response.result as SentinelLogsTailResult;
		}

		/**
		 * Execute a pipeline (multi-step with LLM, conditions, loops).
		 *
		 * Routes directly to Rust SentinelModule's pipeline executor.
		 * This is the primary way to execute complex, multi-step tasks.
		 */
		async sentinelPipeline(pipeline: Pipeline): Promise<PipelineResult> {
			const response = await this.request({
				command: 'sentinel/pipeline',
				pipeline,
			});

			if (!response.success) {
				throw new Error(response.error || 'sentinel/pipeline failed');
			}

			return response.result as PipelineResult;
		}
	};
}
