/**
 * Dataset Configuration Types
 *
 * Shared configuration for AI dataset management commands.
 * Used for archiving Claude project conversation history for training.
 */

export type CompressionType = 'gzip' | 'bzip2' | 'xz' | 'none';

export type SourceType = 'claude-projects' | 'cursor-history' | 'vscode-chat' | 'continuum' | 'git' | 'custom';

export interface DatasetSourceConfig {
  /** Unique identifier for this source */
  id: string;

  /** Human-readable name */
  name: string;

  /** Source type (for known integrations) or 'custom' */
  type: SourceType;

  /** Absolute path or path with env vars (e.g., "$HOME/.claude/projects") */
  basePath: string;

  /** Whether this source is enabled for discovery */
  enabled: boolean;

  /** Pattern for discovering projects within this source (glob pattern) */
  discoveryPattern?: string;
}

export interface DatasetProjectConfig {
  /** Unique identifier for the project */
  id: string;

  /** Human-readable project name */
  name: string;

  /** Which source this project belongs to */
  sourceId: string;

  /** Relative path from source base path, or absolute path */
  path: string;

  /** Whether this project should be included in batch operations */
  enabled: boolean;

  /** Optional tags for filtering/categorization */
  tags?: string[];
}

export interface DatasetConfig {
  /** Config version for migration compatibility */
  version: string;

  /** Default output directory for created datasets */
  defaultOutputPath: string;

  /** Configured data sources (Claude, Cursor, custom, etc.) */
  sources: DatasetSourceConfig[];

  /** Configured projects to archive */
  projects: DatasetProjectConfig[];

  /** Default compression type */
  compression: CompressionType;

  /** Archive naming pattern (supports {project}, {timestamp}, {date}, {time}) */
  naming: string;
}

/**
 * Get default output path from environment/config or fallback to default
 * Checks for DATASETS_DIR from SecretManager (loaded from config.env)
 *
 * Directory Structure:
 * $DATASETS_DIR/
 * ├── raw/          Raw archives from data sources (tar.gz)
 * ├── parsed/       Converted training data (JSONL)
 * └── prepared/     Optimized for fine-tuning (deduped, balanced)
 */
export function getDefaultDatasetsPath(): string {
  // Try to get from SecretManager (loads from ~/.continuum/config.env)
  try {
    const { getSecret } = require('../../../system/secrets/SecretManager');
    const datasetsDir = getSecret('DATASETS_DIR', 'DatasetConfig');
    if (datasetsDir) {
      return `${datasetsDir}/raw`;  // Output to raw/ subdirectory
    }
  } catch (error) {
    // SecretManager not available (e.g., during initial load), fallback to process.env
    if (process.env.DATASETS_DIR) {
      return `${process.env.DATASETS_DIR}/raw`;
    }
  }

  // Fallback to default
  return '$HOME/.continuum/datasets/raw';
}

export const DEFAULT_DATASET_CONFIG: DatasetConfig = {
  version: '1.0.0',
  defaultOutputPath: getDefaultDatasetsPath(),
  sources: [
    {
      id: 'claude-projects',
      name: 'Claude Projects',
      type: 'claude-projects',
      basePath: '$HOME/.claude/projects',
      enabled: true,
      discoveryPattern: '-*'  // Directories starting with dash
    },
    {
      id: 'continuum-data',
      name: 'Continuum Data',
      type: 'continuum',
      basePath: '$HOME/.continuum',
      enabled: false
    }
  ],
  projects: [],
  compression: 'gzip',
  naming: '{project}-{timestamp}.tar.gz'
};

/**
 * Dataset archive metadata
 * Stored as manifest.json inside each archive
 */
export interface DatasetManifest {
  /** Dataset format version */
  version: string;

  /** Project identifier */
  projectId: string;

  /** Project name */
  projectName: string;

  /** ISO timestamp of archive creation */
  createdAt: string;

  /** Compression used */
  compression: CompressionType;

  /** Source directory that was archived */
  sourcePath: string;

  /** Tags for categorization */
  tags: string[];

  /** Uncompressed size in bytes */
  sizeBytes: number;

  /** Number of files archived */
  fileCount: number;
}

/**
 * Dataset archive info (for listing)
 */
export interface DatasetArchiveInfo {
  /** Archive filename */
  filename: string;

  /** Full path to archive */
  path: string;

  /** Archive size in bytes */
  sizeBytes: number;

  /** Archive creation timestamp */
  createdAt: Date;

  /** Parsed manifest (if available) */
  manifest?: DatasetManifest;
}
