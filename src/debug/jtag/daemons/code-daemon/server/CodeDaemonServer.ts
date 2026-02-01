/**
 * CodeDaemon Server - Workspace Operations via Rust IPC
 *
 * Server-side implementation that replaces CodeDaemon static methods.
 * All file operations go through continuum-core Rust backend.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { CodeDaemon } from '../shared/CodeDaemon';
import type {
  WorkspaceEditMode,
} from '../shared/CodeDaemonTypes';
import { Logger } from '../../../system/core/logging/Logger';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';

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

  const rustClient = new RustCoreIPCClient('/tmp/continuum-core.sock');
  try {
    await rustClient.connect();
    log.info('Connected to continuum-core for workspace operations');
  } catch (err) {
    log.warn('continuum-core not available — workspace operations will fail until Rust server starts');
  }

  CodeDaemon.createWorkspace = async (personaId: string, workspaceRoot: string, readRoots?: string[]) => {
    await rustClient.codeCreateWorkspace(personaId, workspaceRoot, readRoots);
  };

  CodeDaemon.workspaceRead = async (personaId: string, filePath: string, startLine?: number, endLine?: number) => {
    return await rustClient.codeRead(personaId, filePath, startLine, endLine);
  };

  CodeDaemon.workspaceWrite = async (personaId: string, filePath: string, content: string, description?: string) => {
    return await rustClient.codeWrite(personaId, filePath, content, description);
  };

  CodeDaemon.workspaceEdit = async (personaId: string, filePath: string, editMode: WorkspaceEditMode, description?: string) => {
    return await rustClient.codeEdit(personaId, filePath, editMode, description);
  };

  CodeDaemon.workspaceDelete = async (personaId: string, filePath: string, description?: string) => {
    return await rustClient.codeDelete(personaId, filePath, description);
  };

  CodeDaemon.workspaceDiff = async (personaId: string, filePath: string, editMode: WorkspaceEditMode) => {
    return await rustClient.codeDiff(personaId, filePath, editMode);
  };

  CodeDaemon.workspaceUndo = async (personaId: string, changeId?: string, count?: number) => {
    return await rustClient.codeUndo(personaId, changeId, count);
  };

  CodeDaemon.workspaceHistory = async (personaId: string, filePath?: string, limit?: number) => {
    return await rustClient.codeHistory(personaId, filePath, limit);
  };

  CodeDaemon.workspaceSearch = async (personaId: string, pattern: string, fileGlob?: string, maxResults?: number) => {
    return await rustClient.codeSearch(personaId, pattern, fileGlob, maxResults);
  };

  CodeDaemon.workspaceTree = async (personaId: string, treePath?: string, maxDepth?: number, includeHidden?: boolean) => {
    return await rustClient.codeTree(personaId, treePath, maxDepth, includeHidden);
  };

  CodeDaemon.workspaceGitStatus = async (personaId: string) => {
    return await rustClient.codeGitStatus(personaId);
  };

  CodeDaemon.workspaceGitDiff = async (personaId: string, staged?: boolean) => {
    return await rustClient.codeGitDiff(personaId, staged);
  };

  log.info('Initialized successfully (workspace operations via Rust IPC)');
}
