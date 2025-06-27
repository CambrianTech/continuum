/**
 * Version Command - Get current system version information
 */

import { BaseCommand } from '../BaseCommand.js';

export interface VersionResult {
  success: boolean;
  version?: string;
  build?: string;
  buildTimestamp?: string;
  startupTime?: string;
  uptime?: string;
  environment?: string;
  nodeVersion?: string;
  platform?: string;
  error?: string;
}

export class VersionCommand extends BaseCommand {
  public readonly name = 'version';
  public readonly description = 'Get current system version information';
  
  protected log(message: string, level: string = 'info'): void {
    const timestamp = new Date().toISOString();
    console.log(`[${timestamp}] [version] ${level.toUpperCase()}: ${message}`);
  }
  
  async execute(params: any = {}): Promise<VersionResult> {
    try {
      // Get version from package.json
      const { readFileSync, statSync } = await import('fs');
      const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
      
      // Get build timestamp from package.json file modification time
      const packageStats = statSync('./package.json');
      const buildTimestamp = packageStats.mtime.toISOString();
      
      // Calculate uptime since process start
      const uptimeSeconds = process.uptime();
      const startupTime = new Date(Date.now() - (uptimeSeconds * 1000)).toISOString();
      
      // Format uptime
      const hours = Math.floor(uptimeSeconds / 3600);
      const minutes = Math.floor((uptimeSeconds % 3600) / 60);
      const seconds = Math.floor(uptimeSeconds % 60);
      const uptimeFormatted = `${hours}h ${minutes}m ${seconds}s`;
      
      const result: VersionResult = {
        success: true,
        version: packageData.version,
        build: 'TypeScript Daemon System',
        buildTimestamp,
        startupTime,
        uptime: uptimeFormatted,
        environment: process.env.NODE_ENV || 'development',
        nodeVersion: process.version,
        platform: `${process.platform} ${process.arch}`
      };
      
      this.log(`📦 Version info requested: v${result.version} (uptime: ${result.uptime})`);
      return result;
      
    } catch (error) {
      this.log(`❌ Failed to get version info: ${error.message}`, 'error');
      return {
        success: false,
        error: `Failed to get version: ${error.message}`
      };
    }
  }
}