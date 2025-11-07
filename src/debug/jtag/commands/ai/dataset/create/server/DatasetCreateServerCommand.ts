/**
 * Dataset Create Server Command
 *
 * Creates compressed archives of AI conversation history for training datasets.
 */

import { spawn } from 'child_process';
import { promises as fs } from 'fs';
import * as path from 'path';
import * as os from 'os';
import type { DatasetCreateParams, DatasetCreateResult } from '../shared/DatasetCreateTypes';
import { createDatasetCreateResult } from '../shared/DatasetCreateTypes';
import type { DatasetConfig, DatasetProjectConfig, DatasetManifest } from '../../shared/DatasetConfig';
import { DEFAULT_DATASET_CONFIG } from '../../shared/DatasetConfig';
import { generateUUID } from '../../../../../system/core/types/CrossPlatformUUID';

export class DatasetCreateServerCommand {
  /**
   * Execute dataset creation
   */
  async execute(params: DatasetCreateParams): Promise<DatasetCreateResult> {
    const startTime = Date.now();
    const sessionId = generateUUID();

    // Load configuration
    const config = await this.loadConfig();

    // Resolve output path
    const outputPath = params.outputPath || this.resolveEnvPath(config.defaultOutputPath);
    await fs.mkdir(outputPath, { recursive: true });

    // Determine which projects to archive
    const projectsToArchive = this.selectProjects(config, params);

    if (projectsToArchive.length === 0) {
      return createDatasetCreateResult(params.context, sessionId, {
        success: false,
        message: 'No projects found to archive',
        archives: [],
        totalSizeBytes: 0,
        durationMs: Date.now() - startTime
      });
    }

    // Create archives
    const archives: DatasetCreateResult['archives'] = [];
    let totalSizeBytes = 0;

    for (const project of projectsToArchive) {
      try {
        const archive = await this.createArchive(config, project, outputPath, params);
        archives.push(archive);
        totalSizeBytes += archive.sizeBytes;

        console.log(`✅ Created ${archive.filename} (${this.formatBytes(archive.sizeBytes)})`);
      } catch (error) {
        console.error(`❌ Failed to archive project ${project.name}:`, error);
      }
    }

    return createDatasetCreateResult(params.context, sessionId, {
      success: archives.length > 0,
      message: `Created ${archives.length} dataset archive(s)`,
      archives,
      totalSizeBytes,
      durationMs: Date.now() - startTime
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
      // Return default config if file doesn't exist
      console.log('📝 Using default dataset configuration');
      return DEFAULT_DATASET_CONFIG;
    }
  }

  /**
   * Select projects to archive based on params
   */
  private selectProjects(config: DatasetConfig, params: DatasetCreateParams): DatasetProjectConfig[] {
    let projects = config.projects.filter(p => p.enabled);

    // Filter by project ID
    if (params.project) {
      projects = projects.filter(p => p.id === params.project);
    }

    // Filter by source ID
    if (params.source) {
      projects = projects.filter(p => p.sourceId === params.source);
    }

    return projects;
  }

  /**
   * Create archive for a single project
   */
  private async createArchive(
    config: DatasetConfig,
    project: DatasetProjectConfig,
    outputPath: string,
    params: DatasetCreateParams
  ): Promise<DatasetCreateResult['archives'][0]> {
    // Resolve source path
    const source = config.sources.find(s => s.id === project.sourceId);
    if (!source) {
      throw new Error(`Source ${project.sourceId} not found`);
    }

    const basePath = this.resolveEnvPath(source.basePath);
    const sourcePath = path.join(basePath, project.path);

    // Check if source exists
    await fs.access(sourcePath);

    // Generate filename
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
    const filename = config.naming
      .replace('{project}', project.id)
      .replace('{timestamp}', timestamp)
      .replace('{date}', timestamp.split('T')[0])
      .replace('{time}', timestamp.split('T')[1]);

    const archivePath = path.join(outputPath, filename);

    // Get compression flag
    const compression = params.compression || config.compression;
    const compFlag = this.getCompressionFlag(compression);

    // Create manifest if requested
    if (params.includeManifest !== false) {
      const manifest = await this.createManifest(project, sourcePath, compression);
      const manifestPath = path.join(sourcePath, 'manifest.json');
      await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2));
    }

    // Create archive using tar
    await this.runTar(sourcePath, archivePath, compFlag);

    // Clean up manifest
    if (params.includeManifest !== false) {
      const manifestPath = path.join(sourcePath, 'manifest.json');
      await fs.unlink(manifestPath).catch(() => {});
    }

    // Get archive size
    const stats = await fs.stat(archivePath);

    return {
      projectId: project.id,
      projectName: project.name,
      filename,
      path: archivePath,
      sizeBytes: stats.size,
      compressionType: compression
    };
  }

  /**
   * Create manifest for archive
   */
  private async createManifest(
    project: DatasetProjectConfig,
    sourcePath: string,
    compression: string
  ): Promise<DatasetManifest> {
    // Count files
    const fileCount = await this.countFiles(sourcePath);

    // Get directory size
    const sizeBytes = await this.getDirectorySize(sourcePath);

    return {
      version: '1.0.0',
      projectId: project.id,
      projectName: project.name,
      createdAt: new Date().toISOString(),
      compression: compression as any,
      sourcePath,
      tags: project.tags || [],
      sizeBytes,
      fileCount
    };
  }

  /**
   * Run tar command
   */
  private async runTar(sourcePath: string, outputPath: string, compressionFlag: string): Promise<void> {
    const sourceDir = path.dirname(sourcePath);
    const sourceName = path.basename(sourcePath);

    return new Promise((resolve, reject) => {
      const args = [compressionFlag, '-cf', outputPath, `./${sourceName}`];
      const tar = spawn('tar', args, {
        cwd: sourceDir,
        stdio: 'pipe'
      });

      let stderr = '';
      tar.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      tar.on('close', (code) => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`tar failed with code ${code}: ${stderr}`));
        }
      });

      tar.on('error', reject);
    });
  }

  /**
   * Get compression flag for tar
   */
  private getCompressionFlag(compression: string): string {
    switch (compression) {
      case 'gzip': return '-z';
      case 'bzip2': return '-j';
      case 'xz': return '-J';
      case 'none': return '';
      default: return '-z';
    }
  }

  /**
   * Count files in directory recursively
   */
  private async countFiles(dirPath: string): Promise<number> {
    let count = 0;
    const entries = await fs.readdir(dirPath, { withFileTypes: true });

    for (const entry of entries) {
      if (entry.isDirectory()) {
        count += await this.countFiles(path.join(dirPath, entry.name));
      } else {
        count++;
      }
    }

    return count;
  }

  /**
   * Get total directory size recursively
   */
  private async getDirectorySize(dirPath: string): Promise<number> {
    let size = 0;
    const entries = await fs.readdir(dirPath, { withFileTypes: true });

    for (const entry of entries) {
      const entryPath = path.join(dirPath, entry.name);
      if (entry.isDirectory()) {
        size += await this.getDirectorySize(entryPath);
      } else {
        const stats = await fs.stat(entryPath);
        size += stats.size;
      }
    }

    return size;
  }

  /**
   * Resolve environment variables in path
   */
  private resolveEnvPath(pathStr: string): string {
    return pathStr
      .replace(/\$HOME/g, os.homedir())
      .replace(/\$(\w+)/g, (_, varName) => process.env[varName] || '');
  }

  /**
   * Format bytes to human-readable string
   */
  private formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
  }
}
