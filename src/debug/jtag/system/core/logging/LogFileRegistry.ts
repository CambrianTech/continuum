/**
 * LogFileRegistry - Dynamic log file discovery and metadata
 *
 * Purpose:
 * - Discover all Logger.ts-managed files
 * - Provide metadata (size, modified time, line count estimate)
 * - Group by category (system, persona, session, external)
 * - Cache results to avoid repeated filesystem scans
 *
 * Usage:
 *   const registry = new LogFileRegistry();
 *   const logs = await registry.discover();
 *   const personaLogs = await registry.filter({ category: 'persona' });
 *   const helperLogs = await registry.filter({ personaId: 'helper-ai' });
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { SystemPaths } from '../config/SystemPaths';

export interface LogMetadata {
  filePath: string;
  category: 'system' | 'persona' | 'session' | 'external';
  component?: string;         // e.g., 'AIProviderDaemon'
  personaId?: string;         // For persona logs
  personaName?: string;       // e.g., 'Helper AI'
  logType?: string;           // e.g., 'adapters', 'cognition', 'tools'
  sizeBytes: number;
  lastModified: Date;
  lineCountEstimate: number;  // Rough estimate (bytes / 100)
  isActive: boolean;          // Currently being written to
}

export interface FilterCriteria {
  category?: string;
  component?: string;
  personaId?: string;
  logType?: string;
  minSize?: number;
  maxSize?: number;
  modifiedAfter?: Date;
}

export class LogFileRegistry {
  private cache: LogMetadata[] | null = null;
  private cacheTimestamp: number = 0;
  private readonly CACHE_TTL_MS = 5000;  // 5 seconds
  private externalLogs: Map<string, { component: string }> = new Map();

  /**
   * Discover all log files (scans filesystem)
   * Uses cache if available and fresh
   */
  async discover(): Promise<LogMetadata[]> {
    // Check cache
    if (this.cache && Date.now() - this.cacheTimestamp < this.CACHE_TTL_MS) {
      return this.cache;
    }

    const logs: LogMetadata[] = [];

    // Discover system logs
    logs.push(...await this.discoverSystemLogs());

    // Discover persona logs
    logs.push(...await this.discoverPersonaLogs());

    // Discover session logs
    logs.push(...await this.discoverSessionLogs());

    // Add external logs
    logs.push(...await this.discoverExternalLogs());

    // Update cache
    this.cache = logs;
    this.cacheTimestamp = Date.now();

    return logs;
  }

  /**
   * Get metadata for specific log file
   */
  async getMetadata(filePath: string): Promise<LogMetadata | null> {
    const logs = await this.discover();
    return logs.find(log => log.filePath === filePath) || null;
  }

  /**
   * Filter logs by criteria
   */
  async filter(criteria: FilterCriteria): Promise<LogMetadata[]> {
    const logs = await this.discover();

    return logs.filter(log => {
      if (criteria.category && log.category !== criteria.category) return false;
      if (criteria.component && log.component !== criteria.component) return false;
      if (criteria.personaId && log.personaId !== criteria.personaId) return false;
      if (criteria.logType && log.logType !== criteria.logType) return false;
      if (criteria.minSize !== undefined && log.sizeBytes < criteria.minSize) return false;
      if (criteria.maxSize !== undefined && log.sizeBytes > criteria.maxSize) return false;
      if (criteria.modifiedAfter && log.lastModified < criteria.modifiedAfter) return false;
      return true;
    });
  }

  /**
   * Register external log (e.g., npm-start.log)
   */
  registerExternal(filePath: string, component: string): void {
    this.externalLogs.set(filePath, { component });
    this.invalidateCache();
  }

  /**
   * Invalidate cache (force re-scan on next discover())
   */
  invalidateCache(): void {
    this.cache = null;
  }

  /**
   * Discover system logs (.continuum/jtag/logs/system/)
   */
  private async discoverSystemLogs(): Promise<LogMetadata[]> {
    const systemLogsDir = SystemPaths.logs.system;
    return this.scanDirectory(systemLogsDir, 'system');
  }

  /**
   * Discover persona logs (.continuum/personas/{id}/logs/)
   */
  private async discoverPersonaLogs(): Promise<LogMetadata[]> {
    const logs: LogMetadata[] = [];
    const personasBaseDir = path.join(SystemPaths.root, 'personas');

    try {
      const personaDirs = await fs.readdir(personasBaseDir, { withFileTypes: true });

      for (const dir of personaDirs) {
        if (!dir.isDirectory()) continue;

        const personaId = dir.name;
        const logsDir = path.join(personasBaseDir, personaId, 'logs');

        try {
          const personaLogs = await this.scanDirectory(logsDir, 'persona', {
            personaId,
            personaName: this.formatPersonaName(personaId)
          });
          logs.push(...personaLogs);
        } catch (error) {
          // Persona logs directory might not exist yet
          continue;
        }
      }
    } catch (error) {
      // Personas directory might not exist yet
    }

    return logs;
  }

  /**
   * Discover session logs (.continuum/sessions/{type}/{run}/logs/)
   */
  private async discoverSessionLogs(): Promise<LogMetadata[]> {
    const logs: LogMetadata[] = [];
    const sessionsBaseDir = path.join(SystemPaths.root, 'sessions');

    try {
      const sessionTypes = await fs.readdir(sessionsBaseDir, { withFileTypes: true });

      for (const typeDir of sessionTypes) {
        if (!typeDir.isDirectory()) continue;

        const typeBasePath = path.join(sessionsBaseDir, typeDir.name);
        const runDirs = await fs.readdir(typeBasePath, { withFileTypes: true });

        for (const runDir of runDirs) {
          if (!runDir.isDirectory()) continue;

          const logsDir = path.join(typeBasePath, runDir.name, 'logs');

          try {
            const sessionLogs = await this.scanDirectory(logsDir, 'session');
            logs.push(...sessionLogs);
          } catch (error) {
            // Logs directory might not exist
            continue;
          }
        }
      }
    } catch (error) {
      // Sessions directory might not exist yet
    }

    return logs;
  }

  /**
   * Discover external logs (registered via registerExternal())
   */
  private async discoverExternalLogs(): Promise<LogMetadata[]> {
    const logs: LogMetadata[] = [];

    for (const [filePath, { component }] of this.externalLogs) {
      try {
        const stats = await fs.stat(filePath);
        logs.push({
          filePath,
          category: 'external',
          component,
          sizeBytes: stats.size,
          lastModified: stats.mtime,
          lineCountEstimate: Math.ceil(stats.size / 100),
          isActive: true
        });
      } catch (error) {
        // File doesn't exist (yet)
        continue;
      }
    }

    return logs;
  }

  /**
   * Scan directory for log files
   */
  private async scanDirectory(
    dirPath: string,
    category: 'system' | 'persona' | 'session' | 'external',
    extraMetadata?: { personaId?: string; personaName?: string }
  ): Promise<LogMetadata[]> {
    const logs: LogMetadata[] = [];

    try {
      const files = await fs.readdir(dirPath, { withFileTypes: true });

      for (const file of files) {
        if (!file.isFile() || !file.name.endsWith('.log')) continue;

        const filePath = path.join(dirPath, file.name);
        const stats = await fs.stat(filePath);

        // Extract component and logType from filename
        const basename = path.basename(file.name, '.log');
        const component = this.extractComponent(basename, category);
        const logType = category === 'persona' ? basename : undefined;

        // Check if file is active (modified in last 5 minutes)
        const isActive = Date.now() - stats.mtime.getTime() < 5 * 60 * 1000;

        logs.push({
          filePath,
          category,
          component,
          logType,
          personaId: extraMetadata?.personaId,
          personaName: extraMetadata?.personaName,
          sizeBytes: stats.size,
          lastModified: stats.mtime,
          lineCountEstimate: Math.ceil(stats.size / 100),
          isActive
        });
      }
    } catch (error) {
      // Directory doesn't exist or no permissions
    }

    return logs;
  }

  /**
   * Extract component name from log filename
   */
  private extractComponent(basename: string, category: string): string | undefined {
    // System logs: filename IS the component (e.g., 'adapters', 'sql', 'coordination')
    if (category === 'system') {
      return basename;
    }

    // Persona logs: logType is the filename (e.g., 'cognition', 'tools', 'adapters')
    // Component would be 'PersonaCognition', 'PersonaToolExecutor', etc.
    if (category === 'persona') {
      const typeToComponent: Record<string, string> = {
        'adapters': 'AIProviderDaemon',
        'body': 'PersonaBody',
        'cns': 'PersonaCNS',
        'cognition': 'PersonaCognition',
        'genome': 'PersonaGenome',
        'hippocampus': 'PersonaHippocampus',
        'mind': 'PersonaMind',
        'personalogger': 'PersonaLogger',
        'soul': 'PersonaSoul',
        'tools': 'PersonaToolExecutor',
        'training': 'TrainingDataAccumulator',
        'user': 'PersonaUser'
      };
      return typeToComponent[basename];
    }

    // Session logs: extract from filename (e.g., 'server-console-log' → 'ServerConsole')
    if (category === 'session') {
      const parts = basename.split('-');
      return parts.slice(0, -1).map(p => p.charAt(0).toUpperCase() + p.slice(1)).join('');
    }

    return undefined;
  }

  /**
   * Format persona ID to human-readable name
   */
  private formatPersonaName(personaId: string): string {
    // Convert 'helper' → 'Helper AI', 'codereview' → 'CodeReview AI'
    const formatted = personaId
      .split(/[-_]/)
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');

    // Add 'AI' suffix if not already present
    return formatted.endsWith(' AI') ? formatted : `${formatted} AI`;
  }
}
