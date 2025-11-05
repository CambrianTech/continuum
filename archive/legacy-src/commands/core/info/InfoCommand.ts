/**
 * Info Command - System Information and Status Display
 * 
 * Provides comprehensive system information including:
 * - System details (Node.js version, platform, memory usage)
 * - Continuum version and status
 * - Daemon status and health
 * - Command availability
 * 
 * Converted to modern TypeScript with middle-out architecture
 */

import { BaseCommand, CommandDefinition, ContinuumContext, CommandResult } from '../base-command/BaseCommand';
import { InfoParams, InfoResult, SystemInfo, ContinuumInfo, DaemonInfo } from './types';
import * as os from 'os';
import * as process from 'process';

export class InfoCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'info',
      category: 'core',
      icon: 'ℹ️',
      description: 'Display system information and Continuum status',
      parameters: {
        verbose: { type: 'boolean' as const, description: 'Enable verbose output with detailed information', required: false },
        format: { type: 'string' as const, description: 'Output format (json|text)', required: false },
        includeSystem: { type: 'boolean' as const, description: 'Include system information', required: false },
        includeDaemons: { type: 'boolean' as const, description: 'Include daemon status information', required: false }
      },
      examples: [
        { description: 'Basic system information', command: 'info' },
        { description: 'Verbose system information', command: 'info --verbose' },
        { description: 'JSON output format', command: 'info --format=json' },
        { description: 'Include daemon status', command: 'info --includeDaemons' }
      ],
      usage: 'Display system information and status'
    };
  }

  static async execute(params: InfoParams, context: ContinuumContext): Promise<CommandResult> {
    try {
      // Gather system information
      const systemInfo: SystemInfo = {
        name: 'continuum',
        version: process.env.npm_package_version || '0.0.0',
        nodeVersion: process.version,
        platform: os.platform(),
        arch: os.arch(),
        uptime: os.uptime(),
        memoryUsage: process.memoryUsage(),
        timestamp: new Date().toISOString()
      };
      
      // Placeholder daemon information - TODO: Implement real daemon discovery
      const daemons: DaemonInfo[] = [
        { name: 'session-manager', status: 'running' },
        { name: 'command-processor', status: 'running' },
        { name: 'browser-manager', status: 'running' }
      ];
      
      const continuumInfo: ContinuumInfo = {
        version: systemInfo.version,
        sessionId: context.sessionId || 'unknown',
        daemons: params.includeDaemons ? daemons : [],
        commands: 30, // TODO: Get actual command count
        uptime: process.uptime()
      };
      
      const result: InfoResult = {
        success: true,
        message: 'System information retrieved successfully',
        system: params.includeSystem !== false ? systemInfo : {} as SystemInfo,
        continuum: continuumInfo
      };
      
      if (params.verbose) {
        console.log('ℹ️ Running InfoCommand in verbose mode');
        console.log('ℹ️ Format:', params.format || 'text');
        console.log('ℹ️ Include System:', params.includeSystem !== false);
        console.log('ℹ️ Include Daemons:', params.includeDaemons || false);
      }
      
      return this.createSuccessResult(
        result.message,
        result
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Info command failed: ${errorMessage}`);
    }
  }
}