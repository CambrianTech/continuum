// ISSUES: 4 open, last updated 2025-07-14 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * 🚨 CRITICAL:
 * - [ ] Issue #1: Create unified FileOperationParams interface to replace inconsistent FileWriteParams and fileSave options
 * - [ ] Issue #2: Create standardized FileOperationResult interface for consistent command results
 * 
 * 🔧 IMPROVEMENTS:
 * - [ ] Issue #3: Create ArtifactType enum to prevent string inconsistencies across screenshot, log, recording, etc.
 * - [ ] Issue #4: Create DirectoryResolutionParams interface for consistent path handling
 * 
 * Shared File Operation Types
 * 
 * These types should be used across all file operations to ensure consistency
 * between browser client, server commands, and file system operations.
 */

import type { UUID } from 'crypto';

/**
 * Artifact types for file operations
 * Centralizes all artifact type definitions to prevent inconsistencies
 */
export enum ArtifactType {
  SCREENSHOT = 'screenshot',
  LOG = 'log', 
  RECORDING = 'recording',
  FILE = 'file',
  DEVTOOLS = 'devtools',
  METADATA = 'metadata'
}

/**
 * Unified parameters for all file operations
 * Replaces inconsistent FileWriteParams and fileSave options
 */
export interface FileOperationParams {
  content: string | Buffer | Uint8Array;
  filename: string;
  artifactType?: ArtifactType;
  encoding?: BufferEncoding;
  sessionId?: UUID;
  marshalId?: string; // For command chaining correlation
}

/**
 * Standardized result format for all file operations
 * Ensures consistent response structure across all file commands
 */
export interface FileOperationResult {
  filename: string;
  filepath: string;
  size: number;
  artifactType: ArtifactType;
  sessionId: UUID;
  timestamp: string;
  success: boolean;
  error?: string;
}

// TODO: Issue #4 - Consistent directory path resolution
export interface DirectoryResolutionParams {
  sessionId?: UUID;
  artifactType?: ArtifactType;
  relativePath?: string;
  absolutePath?: string;
}

export interface ResolvedPath {
  absolute: string;
  relative: string;
  sessionBased: boolean;
}

/**
 * Session-based file paths following the established structure:
 * .continuum/sessions/user/shared/{sessionId}/screenshots/
 * .continuum/sessions/user/shared/{sessionId}/logs/
 * etc.
 */
export interface SessionPaths {
  base: string;
  logs: string;
  screenshots: string;
  recordings: string;
  files: string;
  devtools: string;
}

export interface SessionStructure {
  sessionId: UUID;
  paths: SessionPaths;
  type: 'development' | 'production' | 'test';
  owner: string;
}