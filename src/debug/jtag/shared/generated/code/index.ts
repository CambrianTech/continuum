// Code Module Types - Generated from Rust (single source of truth)
// Re-run: cargo test --package continuum-core --lib export_bindings

// Core change graph types
export type { ChangeNode } from './ChangeNode';
export type { FileOperation } from './FileOperation';
export type { FileDiff } from './FileDiff';
export type { DiffHunk } from './DiffHunk';

// Edit modes (discriminated union)
export type { EditMode } from './EditMode';

// Operation results
export type { WriteResult } from './WriteResult';
export type { ReadResult } from './ReadResult';
export type { UndoResult } from './UndoResult';
export type { HistoryResult } from './HistoryResult';

// Search
export type { SearchMatch } from './SearchMatch';
export type { SearchResult } from './SearchResult';

// Tree
export type { TreeNode } from './TreeNode';
export type { TreeResult } from './TreeResult';

// Git
export type { GitStatusInfo } from './GitStatusInfo';
