/**
 * Dataset List Server Command
 *
 * Lists available dataset archives.
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as os from 'os';
import type { DatasetListParams, DatasetListResult } from '../shared/DatasetListTypes';
import { createDatasetListResult } from '../shared/DatasetListTypes';
import type { DatasetConfig, DatasetArchiveInfo } from '../../shared/DatasetConfig';
import { DEFAULT_DATASET_CONFIG } from '../../shared/DatasetConfig';
import { generateUUID } from '../../../../../system/core/types/CrossPlatformUUID';

export class DatasetListServerCommand {
  /**
   * Execute dataset list
   */
  async execute(params: DatasetListParams): Promise<DatasetListResult> {
    const sessionId = generateUUID();

    // Load configuration
    const config = await this.loadConfig();

    // Resolve search path
    const searchPath = params.path || this.resolveEnvPath(config.defaultOutputPath);

    // Find archives
    const archives = await this.findArchives(searchPath, params.detailed || false);

    // Calculate total size
    const totalSizeBytes = archives.reduce((sum, a) => sum + a.sizeBytes, 0);

    return createDatasetListResult(params.context, sessionId, {
      success: true,
      message: `Found ${archives.length} dataset archive(s)`,
      archives,
      totalSizeBytes
    });
  }

  /**
   * Load dataset configuration
   */
  private async loadConfig(): Promise<DatasetConfig> {
    const configPath = path.join(os.homedir(), '.continuum', 'config', 'datasets.json');

    try {
      const content = await fs.readFile(configPath, 'utf-8');
      return JSON.parse(content);
    } catch (error) {
      return DEFAULT_DATASET_CONFIG;
    }
  }

  /**
   * Find all dataset archives in path
   */
  private async findArchives(searchPath: string, detailed: boolean): Promise<DatasetArchiveInfo[]> {
    const archives: DatasetArchiveInfo[] = [];

    try {
      const entries = await fs.readdir(searchPath, { withFileTypes: true });

      for (const entry of entries) {
        if (entry.isFile() && this.isArchive(entry.name)) {
          const archivePath = path.join(searchPath, entry.name);
          const stats = await fs.stat(archivePath);

          const info: DatasetArchiveInfo = {
            filename: entry.name,
            path: archivePath,
            sizeBytes: stats.size,
            createdAt: stats.birthtime
          };

          // Extract manifest if detailed
          if (detailed) {
            info.manifest = await this.extractManifest(archivePath);
          }

          archives.push(info);
        }
      }
    } catch (error) {
      // Directory doesn't exist or not accessible
      console.warn(`Could not read directory ${searchPath}:`, error);
    }

    // Sort by creation date (newest first)
    archives.sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime());

    return archives;
  }

  /**
   * Check if filename is an archive
   */
  private isArchive(filename: string): boolean {
    return filename.endsWith('.tar.gz') ||
           filename.endsWith('.tar.bz2') ||
           filename.endsWith('.tar.xz') ||
           filename.endsWith('.tar');
  }

  /**
   * Extract manifest from archive (if it exists)
   */
  private async extractManifest(archivePath: string): Promise<any | undefined> {
    // TODO: Extract manifest.json from archive using tar
    // For now, return undefined
    return undefined;
  }

  /**
   * Resolve environment variables in path
   */
  private resolveEnvPath(pathStr: string): string {
    return pathStr
      .replace(/\$HOME/g, os.homedir())
      .replace(/\$(\w+)/g, (_, varName) => process.env[varName] || '');
  }
}
