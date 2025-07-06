/**
 * Daemon Module - Extends BaseModule with daemon-specific validation
 */

import { BaseModule, ValidationResult, MigrationResult } from './BaseModule';
import { ContinuumDaemonConfig } from './ContinuumPackage';

export class ContinuumDaemon extends BaseModule {
  
  async validate(): Promise<ValidationResult> {
    // Call parent validation first
    const baseResult = await super.validate();
    
    // Add daemon-specific validation
    const daemonResult = await this.validateDaemonSpecific();
    
    return this.combineValidationResults(baseResult, daemonResult);
  }

  async migrate(): Promise<MigrationResult> {
    // Call parent migration first  
    const baseResult = await super.migrate();
    
    // Add daemon-specific migration
    const daemonResult = await this.migrateDaemonSpecific();
    
    return this.combineMigrationResults(baseResult, daemonResult);
  }

  private async validateDaemonSpecific(): Promise<ValidationResult> {
    const errors: string[] = [];
    const warnings: string[] = [];
    const checks: Record<string, boolean> = {};

    if (!this.config || !('daemon' in this.config)) {
      errors.push('Not a valid daemon configuration');
      return { isValid: false, errors, warnings, checks };
    }

    const daemonConfig = this.config as ContinuumDaemonConfig;

    // Daemon must have lifecycle methods
    const daemonFileName = this.getDaemonFileName();
    checks.hasImplementationFile = await this.checkFileExists(daemonFileName);
    
    if (checks.hasImplementationFile) {
      checks.hasOnStartMethod = await this.checkFileContains(daemonFileName, 'async onStart');
      checks.hasOnStopMethod = await this.checkFileContains(daemonFileName, 'async onStop');
      checks.extendsBaseDaemon = await this.checkFileContains(daemonFileName, 'extends BaseDaemon');
    } else {
      errors.push(`Missing daemon implementation file: ${daemonFileName}`);
      checks.hasOnStartMethod = false;
      checks.hasOnStopMethod = false;
      checks.extendsBaseDaemon = false;
    }

    // Validate daemon name matches directory
    const expectedDaemonName = this.getExpectedDaemonName();
    checks.correctDaemonName = daemonConfig.daemon === expectedDaemonName;
    if (!checks.correctDaemonName) {
      warnings.push(`Daemon name '${daemonConfig.daemon}' doesn't match expected '${expectedDaemonName}'`);
    }

    // Check startup order is reasonable
    if (daemonConfig.startupOrder !== undefined) {
      checks.validStartupOrder = daemonConfig.startupOrder >= 0 && daemonConfig.startupOrder <= 1000;
      if (!checks.validStartupOrder) {
        warnings.push(`Startup order ${daemonConfig.startupOrder} should be between 0-1000`);
      }
    } else {
      checks.validStartupOrder = true; // Optional field
    }

    // Check health check configuration
    if (daemonConfig.healthCheck) {
      checks.validHealthCheck = (
        typeof daemonConfig.healthCheck.enabled === 'boolean' &&
        typeof daemonConfig.healthCheck.intervalMs === 'number' &&
        typeof daemonConfig.healthCheck.timeoutMs === 'number'
      );
    } else {
      checks.validHealthCheck = true; // Optional field
    }

    const failedChecks = Object.values(checks).filter(check => !check).length;
    const isValid = errors.length === 0 && failedChecks === 0;

    return { isValid, errors, warnings, checks };
  }

  private async migrateDaemonSpecific(): Promise<MigrationResult> {
    const changes: string[] = [];
    const errors: string[] = [];

    try {
      // Create daemon implementation file if missing
      const daemonFileName = this.getDaemonFileName();
      if (!await this.checkFileExists(daemonFileName)) {
        await this.createDaemonTemplate(daemonFileName);
        changes.push(`Created daemon implementation: ${daemonFileName}`);
      }

      // Create daemon test files if missing
      const testFileName = `test/unit/${this.getDaemonClassName()}.test.ts`;
      if (!await this.checkFileExists(testFileName)) {
        await this.createDaemonTestTemplate(testFileName);
        changes.push(`Created daemon test: ${testFileName}`);
      }

    } catch (error) {
      errors.push(`Daemon migration failed: ${error instanceof Error ? error.message : String(error)}`);
    }

    return { migrated: changes.length > 0, changes, errors };
  }

  private getDaemonFileName(): string {
    return `${this.getDaemonClassName()}.ts`;
  }

  private getDaemonClassName(): string {
    const expectedName = this.getExpectedDaemonName();
    return expectedName.split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join('') + 'Daemon';
  }

  private getExpectedDaemonName(): string {
    // Extract daemon name from directory path
    const parts = this.modulePath.split('/');
    return parts[parts.length - 1];
  }

  private async createDaemonTemplate(fileName: string): Promise<void> {
    const className = this.getDaemonClassName();
    const daemonName = this.getExpectedDaemonName();
    
    const template = `/**
 * ${className} - Generated daemon template
 */

import { BaseDaemon } from '../base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../base/BaseDaemon.js';

export class ${className} extends BaseDaemon {
  
  async onStart(): Promise<void> {
    this.log('🚀 ${className} starting...', 'info');
    
    // Initialize daemon-specific resources here
    
    this.log('✅ ${className} started successfully', 'info');
  }
  
  async onStop(): Promise<void> {
    this.log('🛑 ${className} stopping...', 'info');
    
    // Clean up daemon-specific resources here
    
    this.log('✅ ${className} stopped successfully', 'info');
  }
  
  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(\`📨 Received message: \${message.type}\`, 'debug');
    
    try {
      switch (message.type) {
        case 'ping':
          return this.createResponse(message, { pong: true });
          
        case 'status':
          return this.createResponse(message, { 
            status: 'running',
            daemon: '${daemonName}'
          });
          
        default:
          return this.createErrorResponse(message, \`Unknown message type: \${message.type}\`);
      }
    } catch (error) {
      return this.createErrorResponse(message, error instanceof Error ? error.message : String(error));
    }
  }
  
  getDaemonName(): string {
    return '${daemonName}';
  }
}`;

    const fs = await import('fs/promises');
    const path = await import('path');
    await fs.writeFile(path.join(this.modulePath, fileName), template);
  }

  private async createDaemonTestTemplate(fileName: string): Promise<void> {
    const className = this.getDaemonClassName();
    
    const template = `/**
 * Unit tests for ${className}
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { ${className} } from '../../${this.getDaemonFileName().replace('.ts', '.js')}';

test('${className} can start and stop', async () => {
  const daemon = new ${className}();
  
  // Test startup
  await daemon.onStart();
  assert.equal(daemon.getDaemonName(), '${this.getExpectedDaemonName()}');
  
  // Test shutdown
  await daemon.onStop();
});

test('${className} handles basic messages', async () => {
  const daemon = new ${className}();
  
  const pingMessage = {
    id: 'test-1',
    from: 'test',
    to: '${this.getExpectedDaemonName()}',
    type: 'ping',
    data: {},
    timestamp: new Date(),
    priority: 'normal' as const
  };
  
  const response = await daemon['handleMessage'](pingMessage);
  assert.equal(response.success, true);
  assert.equal(response.data.pong, true);
});`;

    const fs = await import('fs/promises');
    const path = await import('path');
    await fs.mkdir(path.dirname(path.join(this.modulePath, fileName)), { recursive: true });
    await fs.writeFile(path.join(this.modulePath, fileName), template);
  }
}