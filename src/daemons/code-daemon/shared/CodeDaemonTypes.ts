/**
 * CodeDaemon Types - Shared type definitions
 *
 * Workspace-scoped types re-exported from ts-rs generated (Rust is source of truth).
 * Aliased with Workspace* prefix for domain clarity in CodeDaemon API.
 */

export type { EditMode as WorkspaceEditMode } from '../../../shared/generated/code/EditMode';
export type { WriteResult as WorkspaceWriteResult } from '../../../shared/generated/code/WriteResult';
export type { ReadResult as WorkspaceReadResult } from '../../../shared/generated/code/ReadResult';
export type { SearchResult as WorkspaceSearchResult } from '../../../shared/generated/code/SearchResult';
export type { SearchMatch as WorkspaceSearchMatch } from '../../../shared/generated/code/SearchMatch';
export type { TreeNode as WorkspaceTreeNode } from '../../../shared/generated/code/TreeNode';
export type { TreeResult as WorkspaceTreeResult } from '../../../shared/generated/code/TreeResult';
export type { UndoResult as WorkspaceUndoResult } from '../../../shared/generated/code/UndoResult';
export type { ChangeNode as WorkspaceChangeNode } from '../../../shared/generated/code/ChangeNode';
export type { HistoryResult as WorkspaceHistoryResult } from '../../../shared/generated/code/HistoryResult';
export type { GitStatusInfo as WorkspaceGitStatusInfo } from '../../../shared/generated/code/GitStatusInfo';

// Shell session types
export type { ShellExecutionStatus as WorkspaceShellExecutionStatus } from '../../../shared/generated/code/ShellExecutionStatus';
export type { ShellExecuteResponse as WorkspaceShellExecuteResponse } from '../../../shared/generated/code/ShellExecuteResponse';
export type { ShellPollResponse as WorkspaceShellPollResponse } from '../../../shared/generated/code/ShellPollResponse';
export type { ShellSessionInfo as WorkspaceShellSessionInfo } from '../../../shared/generated/code/ShellSessionInfo';
export type { ShellHistoryEntry as WorkspaceShellHistoryEntry } from '../../../shared/generated/code/ShellHistoryEntry';

// Shell watch + sentinel types
export type { OutputClassification as WorkspaceOutputClassification } from '../../../shared/generated/code/OutputClassification';
export type { SentinelAction as WorkspaceSentinelAction } from '../../../shared/generated/code/SentinelAction';
export type { SentinelRule as WorkspaceSentinelRule } from '../../../shared/generated/code/SentinelRule';
export type { ClassifiedLine as WorkspaceClassifiedLine } from '../../../shared/generated/code/ClassifiedLine';
export type { ShellWatchResponse as WorkspaceShellWatchResponse } from '../../../shared/generated/code/ShellWatchResponse';
