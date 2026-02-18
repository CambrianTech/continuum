/**
 * RustCoreIPC Code Module - File operations, change tracking, code intelligence, shell
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	EditMode,
	ReadResult,
	WriteResult,
	SearchResult,
	TreeResult,
	UndoResult,
	HistoryResult,
	GitStatusInfo,
	ShellExecuteResponse,
	ShellPollResponse,
	ShellSessionInfo,
	ShellWatchResponse,
	SentinelRule,
} from '../../../../shared/generated';

// ============================================================================
// Mixin
// ============================================================================

export interface CodeMixin {
	// File operations
	codeCreateWorkspace(personaId: string, workspaceRoot: string, readRoots?: string[]): Promise<void>;
	codeRead(personaId: string, filePath: string, startLine?: number, endLine?: number): Promise<ReadResult>;
	codeWrite(personaId: string, filePath: string, content: string, description?: string): Promise<WriteResult>;
	codeEdit(personaId: string, filePath: string, editMode: EditMode, description?: string): Promise<WriteResult>;
	codeDelete(personaId: string, filePath: string, description?: string): Promise<WriteResult>;
	codeDiff(personaId: string, filePath: string, editMode: EditMode): Promise<{ success: boolean; unified: string }>;
	codeUndo(personaId: string, changeId?: string, count?: number): Promise<UndoResult>;
	codeHistory(personaId: string, filePath?: string, limit?: number): Promise<HistoryResult>;
	codeSearch(personaId: string, pattern: string, fileGlob?: string, maxResults?: number): Promise<SearchResult>;
	codeTree(personaId: string, path?: string, maxDepth?: number, includeHidden?: boolean): Promise<TreeResult>;
	// Git operations
	codeGitStatus(personaId: string): Promise<GitStatusInfo>;
	codeGitDiff(personaId: string, staged?: boolean): Promise<{ success: boolean; diff: string }>;
	codeGitLog(personaId: string, count?: number): Promise<{ success: boolean; log: string }>;
	codeGitAdd(personaId: string, paths: string[]): Promise<{ staged: string[] }>;
	codeGitCommit(personaId: string, message: string): Promise<{ hash: string }>;
	codeGitPush(personaId: string, remote?: string, branch?: string): Promise<{ output: string }>;
	// Shell operations
	shellCreate(personaId: string, workspaceRoot: string): Promise<ShellSessionInfo>;
	shellExecute(personaId: string, cmd: string, options?: { timeoutMs?: number; wait?: boolean }): Promise<ShellExecuteResponse>;
	shellPoll(personaId: string, executionId: string): Promise<ShellPollResponse>;
	shellKill(personaId: string, executionId: string): Promise<void>;
	shellCd(personaId: string, path: string): Promise<{ cwd: string }>;
	shellStatus(personaId: string): Promise<ShellSessionInfo>;
	shellDestroy(personaId: string): Promise<void>;
	shellWatch(personaId: string, executionId: string): Promise<ShellWatchResponse>;
	shellSentinel(personaId: string, executionId: string, rules: SentinelRule[]): Promise<{ applied: boolean; ruleCount: number }>;
}

export function CodeMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements CodeMixin {
		// ────────────────────────────────────────────────────────────────
		// File Operations
		// ────────────────────────────────────────────────────────────────

		async codeCreateWorkspace(personaId: string, workspaceRoot: string, readRoots?: string[]): Promise<void> {
			const response = await this.request({
				command: 'code/create-workspace',
				persona_id: personaId,
				workspace_root: workspaceRoot,
				read_roots: readRoots ?? [],
			});
			if (!response.success) throw new Error(response.error || 'Failed to create workspace');
		}

		async codeRead(personaId: string, filePath: string, startLine?: number, endLine?: number): Promise<ReadResult> {
			const response = await this.request({
				command: 'code/read',
				persona_id: personaId,
				file_path: filePath,
				start_line: startLine ?? null,
				end_line: endLine ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to read file');
			return response.result as ReadResult;
		}

		async codeWrite(personaId: string, filePath: string, content: string, description?: string): Promise<WriteResult> {
			const response = await this.request({
				command: 'code/write',
				persona_id: personaId,
				file_path: filePath,
				content,
				description: description ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to write file');
			return response.result as WriteResult;
		}

		async codeEdit(personaId: string, filePath: string, editMode: EditMode, description?: string): Promise<WriteResult> {
			const response = await this.request({
				command: 'code/edit',
				persona_id: personaId,
				file_path: filePath,
				edit_mode: editMode,
				description: description ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to edit file');
			return response.result as WriteResult;
		}

		async codeDelete(personaId: string, filePath: string, description?: string): Promise<WriteResult> {
			const response = await this.request({
				command: 'code/delete',
				persona_id: personaId,
				file_path: filePath,
				description: description ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to delete file');
			return response.result as WriteResult;
		}

		async codeDiff(personaId: string, filePath: string, editMode: EditMode): Promise<{ success: boolean; unified: string }> {
			const response = await this.request({
				command: 'code/diff',
				persona_id: personaId,
				file_path: filePath,
				edit_mode: editMode,
			});
			if (!response.success) throw new Error(response.error || 'Failed to compute diff');
			return response.result as { success: boolean; unified: string };
		}

		async codeUndo(personaId: string, changeId?: string, count?: number): Promise<UndoResult> {
			const response = await this.request({
				command: 'code/undo',
				persona_id: personaId,
				change_id: changeId ?? null,
				count: count ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to undo');
			return response.result as UndoResult;
		}

		async codeHistory(personaId: string, filePath?: string, limit?: number): Promise<HistoryResult> {
			const response = await this.request({
				command: 'code/history',
				persona_id: personaId,
				file_path: filePath ?? null,
				limit: limit ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to get history');
			return response.result as HistoryResult;
		}

		async codeSearch(personaId: string, pattern: string, fileGlob?: string, maxResults?: number): Promise<SearchResult> {
			const response = await this.request({
				command: 'code/search',
				persona_id: personaId,
				pattern,
				file_glob: fileGlob ?? null,
				max_results: maxResults ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Failed to search');
			return response.result as SearchResult;
		}

		async codeTree(personaId: string, path?: string, maxDepth?: number, includeHidden?: boolean): Promise<TreeResult> {
			const response = await this.request({
				command: 'code/tree',
				persona_id: personaId,
				path: path ?? null,
				max_depth: maxDepth ?? null,
				include_hidden: includeHidden ?? false,
			});
			if (!response.success) throw new Error(response.error || 'Failed to generate tree');
			return response.result as TreeResult;
		}

		// ────────────────────────────────────────────────────────────────
		// Git Operations
		// ────────────────────────────────────────────────────────────────

		async codeGitStatus(personaId: string): Promise<GitStatusInfo> {
			const response = await this.request({ command: 'code/git-status', persona_id: personaId });
			if (!response.success) throw new Error(response.error || 'Failed to get git status');
			return response.result as GitStatusInfo;
		}

		async codeGitDiff(personaId: string, staged?: boolean): Promise<{ success: boolean; diff: string }> {
			const response = await this.request({ command: 'code/git-diff', persona_id: personaId, staged: staged ?? false });
			if (!response.success) throw new Error(response.error || 'Failed to get git diff');
			return response.result as { success: boolean; diff: string };
		}

		async codeGitLog(personaId: string, count?: number): Promise<{ success: boolean; log: string }> {
			const response = await this.request({ command: 'code/git-log', persona_id: personaId, count: count ?? 10 });
			if (!response.success) throw new Error(response.error || 'Failed to get git log');
			return response.result as { success: boolean; log: string };
		}

		async codeGitAdd(personaId: string, paths: string[]): Promise<{ staged: string[] }> {
			const response = await this.request({ command: 'code/git-add', persona_id: personaId, paths });
			if (!response.success) throw new Error(response.error || 'Failed to stage files');
			return response.result as { staged: string[] };
		}

		async codeGitCommit(personaId: string, message: string): Promise<{ hash: string }> {
			const response = await this.request({ command: 'code/git-commit', persona_id: personaId, message });
			if (!response.success) throw new Error(response.error || 'Failed to create commit');
			return response.result as { hash: string };
		}

		async codeGitPush(personaId: string, remote?: string, branch?: string): Promise<{ output: string }> {
			const response = await this.request({ command: 'code/git-push', persona_id: personaId, remote: remote ?? '', branch: branch ?? '' });
			if (!response.success) throw new Error(response.error || 'Failed to push');
			return response.result as { output: string };
		}

		// ────────────────────────────────────────────────────────────────
		// Shell Operations
		// ────────────────────────────────────────────────────────────────

		async shellCreate(personaId: string, workspaceRoot: string): Promise<ShellSessionInfo> {
			const response = await this.request({ command: 'code/shell-create', persona_id: personaId, workspace_root: workspaceRoot });
			if (!response.success) throw new Error(response.error || 'Failed to create shell session');
			return response.result as ShellSessionInfo;
		}

		async shellExecute(personaId: string, cmd: string, options?: { timeoutMs?: number; wait?: boolean }): Promise<ShellExecuteResponse> {
			const response = await this.request({
				command: 'code/shell-execute',
				persona_id: personaId,
				cmd,
				timeout_ms: options?.timeoutMs ?? null,
				wait: options?.wait ?? false,
			});
			if (!response.success) throw new Error(response.error || 'Failed to execute command');
			return response.result as ShellExecuteResponse;
		}

		async shellPoll(personaId: string, executionId: string): Promise<ShellPollResponse> {
			const response = await this.request({ command: 'code/shell-poll', persona_id: personaId, execution_id: executionId });
			if (!response.success) throw new Error(response.error || 'Failed to poll execution');
			return response.result as ShellPollResponse;
		}

		async shellKill(personaId: string, executionId: string): Promise<void> {
			const response = await this.request({ command: 'code/shell-kill', persona_id: personaId, execution_id: executionId });
			if (!response.success) throw new Error(response.error || 'Failed to kill execution');
		}

		async shellCd(personaId: string, path: string): Promise<{ cwd: string }> {
			const response = await this.request({ command: 'code/shell-cd', persona_id: personaId, path });
			if (!response.success) throw new Error(response.error || 'Failed to change directory');
			return response.result as { cwd: string };
		}

		async shellStatus(personaId: string): Promise<ShellSessionInfo> {
			const response = await this.request({ command: 'code/shell-status', persona_id: personaId });
			if (!response.success) throw new Error(response.error || 'Failed to get shell status');
			return response.result as ShellSessionInfo;
		}

		async shellDestroy(personaId: string): Promise<void> {
			const response = await this.request({ command: 'code/shell-destroy', persona_id: personaId });
			if (!response.success) throw new Error(response.error || 'Failed to destroy shell');
		}

		async shellWatch(personaId: string, executionId: string): Promise<ShellWatchResponse> {
			const response = await this.request({ command: 'code/shell-watch', persona_id: personaId, execution_id: executionId });
			if (!response.success) throw new Error(response.error || 'Failed to watch execution');
			return response.result as ShellWatchResponse;
		}

		async shellSentinel(personaId: string, executionId: string, rules: SentinelRule[]): Promise<{ applied: boolean; ruleCount: number }> {
			const response = await this.request({ command: 'code/shell-sentinel', persona_id: personaId, execution_id: executionId, rules });
			if (!response.success) throw new Error(response.error || 'Failed to apply sentinel rules');
			return response.result as { applied: boolean; ruleCount: number };
		}
	};
}
