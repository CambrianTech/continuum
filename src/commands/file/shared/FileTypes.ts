// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * File Command Types - Consolidated from src/types/shared/FileOperations.ts
 * 
 * Following middle-out architecture pattern:
 * - Shared types used across client, server, and remote contexts
 * - Centralized type definitions to prevent inconsistencies
 * - Unified interfaces for all file operations
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

/**
 * Directory resolution parameters for consistent path handling
 */
export interface DirectoryResolutionParams {
  sessionId?: UUID;
  artifactType?: ArtifactType;
  relativePath?: string;
  absolutePath?: string;
}

/**
 * Resolved path information
 */
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

/**
 * Session structure for file operations
 */
export interface SessionStructure {
  sessionId: UUID;
  paths: SessionPaths;
  type: 'development' | 'production' | 'test';
  owner: string;
}

/**
 * File validation result
 */
export interface FileValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

/**
 * File operation metadata
 */
export interface FileOperationMetadata {
  operation: string;
  timestamp: string;
  sessionId: UUID;
  artifactType: ArtifactType;
  size?: number;
  encoding?: BufferEncoding;
}

/**
 * File command context for consistent operation tracking
 */
export interface FileCommandContext {
  sessionId: UUID;
  requestId: string;
  timestamp: number;
  artifactType: ArtifactType;
  metadata?: Record<string, any>;
}