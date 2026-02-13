/**
 * CodeDaemon Server - Workspace Operations via Rust IPC
 *
 * Server-side implementation that replaces CodeDaemon static methods.
 * All file operations go through continuum-core Rust backend.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { CodeDaemon } from '../shared/CodeDaemon';
import type {
  WorkspaceEditMode,
} from '../shared/CodeDaemonTypes';
import { Logger } from '../../../system/core/logging/Logger';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';

/**
 * Initialize CodeDaemon for server usage.
 * Connects to continuum-core Rust backend for all workspace operations.
 */
export async function initializeCodeDaemon(jtagContext: JTAGContext): Promise<void> {
  const log = Logger.create('CodeDaemonServer', 'daemons/CodeDaemonServer');
  log.info('Initializing CodeDaemon...');

  // ========================================================================
  // Workspace-Scoped Operations (Rust IPC backed)
  // ========================================================================

  const rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  try {
    await rustClient.connect();
    log.info('Connected to continuum-core for workspace operations');
  } catch (err) {
    log.warn('continuum-core not available — workspace operations will fail until Rust server starts');
  }

  CodeDaemon.createWorkspace = async (personaId: UUID, workspaceRoot: string, readRoots?: string[]) => {
    await rustClient.codeCreateWorkspace(personaId, workspaceRoot, readRoots);
  };

  CodeDaemon.workspaceRead = async (personaId: UUID, filePath: string, startLine?: number, endLine?: number) => {
    return await rustClient.codeRead(personaId, filePath, startLine, endLine);
  };

  CodeDaemon.workspaceWrite = async (personaId: UUID, filePath: string, content: string, description?: string) => {
    return await rustClient.codeWrite(personaId, filePath, content, description);
  };

  CodeDaemon.workspaceEdit = async (personaId: UUID, filePath: string, editMode: WorkspaceEditMode, description?: string) => {
    return await rustClient.codeEdit(personaId, filePath, editMode, description);
  };

  CodeDaemon.workspaceDelete = async (personaId: UUID, filePath: string, description?: string) => {
    return await rustClient.codeDelete(personaId, filePath, description);
  };

  CodeDaemon.workspaceDiff = async (personaId: UUID, filePath: string, editMode: WorkspaceEditMode) => {
    return await rustClient.codeDiff(personaId, filePath, editMode);
  };

  CodeDaemon.workspaceUndo = async (personaId: UUID, changeId?: string, count?: number) => {
    return await rustClient.codeUndo(personaId, changeId, count);
  };

  CodeDaemon.workspaceHistory = async (personaId: UUID, filePath?: string, limit?: number) => {
    return await rustClient.codeHistory(personaId, filePath, limit);
  };

  CodeDaemon.workspaceSearch = async (personaId: UUID, pattern: string, fileGlob?: string, maxResults?: number) => {
    return await rustClient.codeSearch(personaId, pattern, fileGlob, maxResults);
  };

  CodeDaemon.workspaceTree = async (personaId: UUID, treePath?: string, maxDepth?: number, includeHidden?: boolean) => {
    return await rustClient.codeTree(personaId, treePath, maxDepth, includeHidden);
  };

  CodeDaemon.workspaceGitStatus = async (personaId: UUID) => {
    return await rustClient.codeGitStatus(personaId);
  };

  CodeDaemon.workspaceGitDiff = async (personaId: UUID, staged?: boolean) => {
    return await rustClient.codeGitDiff(personaId, staged);
  };

  CodeDaemon.workspaceGitLog = async (personaId: UUID, count?: number) => {
    return await rustClient.codeGitLog(personaId, count);
  };

  CodeDaemon.workspaceGitAdd = async (personaId: UUID, paths: string[]) => {
    return await rustClient.codeGitAdd(personaId, paths);
  };

  CodeDaemon.workspaceGitCommit = async (personaId: UUID, message: string) => {
    return await rustClient.codeGitCommit(personaId, message);
  };

  CodeDaemon.workspaceGitPush = async (personaId: UUID, remote?: string, branch?: string) => {
    return await rustClient.codeGitPush(personaId, remote, branch);
  };

  // ========================================================================
  // Shell Session Operations (Handle + Poll pattern)
  // ========================================================================

  CodeDaemon.shellCreate = async (personaId: UUID, workspaceRoot: string) => {
    return await rustClient.shellCreate(personaId, workspaceRoot);
  };

  CodeDaemon.shellExecute = async (personaId: UUID, cmd: string, options?: { timeoutMs?: number; wait?: boolean }) => {
    return await rustClient.shellExecute(personaId, cmd, options);
  };

  CodeDaemon.shellPoll = async (personaId: UUID, executionId: string) => {
    return await rustClient.shellPoll(personaId, executionId);
  };

  CodeDaemon.shellKill = async (personaId: UUID, executionId: string) => {
    await rustClient.shellKill(personaId, executionId);
  };

  CodeDaemon.shellCd = async (personaId: UUID, path: string) => {
    return await rustClient.shellCd(personaId, path);
  };

  CodeDaemon.shellStatus = async (personaId: UUID) => {
    return await rustClient.shellStatus(personaId);
  };

  CodeDaemon.shellDestroy = async (personaId: UUID) => {
    await rustClient.shellDestroy(personaId);
  };

  // ========================================================================
  // Shell Watch + Sentinel (Event-driven output streaming)
  // ========================================================================

  CodeDaemon.shellWatch = async (personaId: UUID, executionId: string) => {
    return await rustClient.shellWatch(personaId, executionId);
  };

  CodeDaemon.shellSentinel = async (personaId: UUID, executionId: string, rules) => {
    return await rustClient.shellSentinel(personaId, executionId, rules);
  };

  log.info('Initialized successfully (workspace + shell + watch/sentinel operations via Rust IPC)');
}
