/**
 * Command Module - Extends BaseModule with command-specific validation
 */

import { BaseModule, ValidationResult, MigrationResult, TestType, TestResult, TestDetail } from './BaseModule';
import { ContinuumCommandConfig } from './ContinuumPackage';

export class ContinuumCommand extends BaseModule {
  
  async validate(): Promise<ValidationResult> {
    // Call parent validation first
    const result = await super.validate();
    
    // Add command-specific validation
    if (!this.config || !('command' in this.config)) {
      result.errors.push('Not a valid command configuration');
      result.isValid = false;
      return result;
    }

    const commandConfig = this.config as ContinuumCommandConfig;

    // Command must have execute method
    const commandFileName = this.getCommandFileName();
    result.checks.hasImplementationFile = await this.checkFileExists(commandFileName);
    
    if (result.checks.hasImplementationFile) {
      result.checks.hasExecuteMethod = await this.checkFileContains(commandFileName, 'static async execute');
      result.checks.hasGetDefinitionMethod = await this.checkFileContains(commandFileName, 'static getDefinition');
      result.checks.extendsBaseCommand = await this.checkFileContains(commandFileName, 'extends BaseCommand');
      
      if (!result.checks.extendsBaseCommand) {
        result.errors.push('Command must extend BaseCommand');
      }
    } else {
      result.errors.push(`Missing command implementation file: ${commandFileName}`);
      result.checks.hasExecuteMethod = false;
      result.checks.hasGetDefinitionMethod = false;
      result.checks.extendsBaseCommand = false;
    }

    // Check if command name matches directory
    const expectedCommandName = this.getExpectedCommandName();
    result.checks.correctCommandName = commandConfig.command === expectedCommandName;
    if (!result.checks.correctCommandName) {
      result.warnings.push(`Command name '${commandConfig.command}' doesn't match expected '${expectedCommandName}'`);
    }

    // Update isValid based on all checks
    const failedChecks = Object.values(result.checks).filter(check => !check).length;
    result.isValid = result.errors.length === 0 && failedChecks === 0;

    return result;
  }

  async migrate(): Promise<MigrationResult> {
    // Call parent migration first - base class handles version tracking, standard directories
    const result = await super.migrate();
    
    // Add command-specific migration using base class protected methods
    await this.migrateCommandImplementation(result);
    await this.migrateCommandTests(result);

    return result;
  }

  private async migrateCommandImplementation(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('command-implementation', 'Create command implementation file');
    
    try {
      const commandFileName = this.getCommandFileName();
      if (!await this.checkFileExists(commandFileName)) {
        await this.createCommandTemplate(commandFileName);
        result.changes.push(`Created command implementation: ${commandFileName}`);
        result.migrated = true;
      }
      step.completed = true;
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`Command implementation migration failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  private async migrateCommandTests(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('command-tests', 'Create command test files');
    
    try {
      const testFileName = `test/unit/${this.getCommandClassName()}.test.ts`;
      if (!await this.checkFileExists(testFileName)) {
        await this.createCommandTestTemplate(testFileName);
        result.changes.push(`Created command test: ${testFileName}`);
        result.migrated = true;
      }
      step.completed = true;
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`Command test migration failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  /**
   * Command-specific unit tests - inherits base tests + adds command tests
   */
  protected async runUnitTests(): Promise<TestResult> {
    // Get base module unit tests first (super.runUnitTests())
    const baseResult = await super.runUnitTests();
    
    // Add command-specific unit tests
    const commandTests = [
      { name: 'has-execute-method', test: () => this.hasExecuteMethod() },
      { name: 'has-definition-method', test: () => this.hasDefinitionMethod() },
      { name: 'extends-base-command', test: () => this.extendsBaseCommand() },
      { name: 'command-name-matches', test: () => this.commandNameMatches() }
    ];

    let successful = baseResult.successful;
    const details = [...baseResult.details];

    for (const { name, test } of commandTests) {
      const startTime = Date.now();
      try {
        const passed = await test();
        const detail: TestDetail = {
          name: `command-${name}`,
          passed,
          duration: Date.now() - startTime
        };
        if (!passed) {
          detail.error = `Command test failed: ${name}`;
        }
        details.push(detail);
        if (passed) successful++;
      } catch (error) {
        details.push({
          name: `command-${name}`,
          passed: false,
          duration: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    const total = baseResult.total + commandTests.length;
    const failed = total - successful;

    return {
      testType: TestType.Unit,
      passed: failed === 0,
      total,
      successful,
      failed,
      errors: [...baseResult.errors, ...details.filter(d => !d.passed).map(d => d.error || 'Unknown error')],
      details
    };
  }

  /**
   * Command-specific integration tests - inherits base + adds command integration
   */
  protected async runIntegrationTests(): Promise<TestResult> {
    // Get base integration tests first
    const baseResult = await super.runIntegrationTests();
    
    // Add command-specific integration tests
    const integrationTests = [
      { name: 'can-execute-command', test: () => this.canExecuteCommand() },
      { name: 'command-registration-works', test: () => this.canRegisterCommand() },
      { name: 'parameter-validation-works', test: () => this.canValidateParameters() }
    ];

    let successful = baseResult.successful;
    const details = [...baseResult.details];

    for (const { name, test } of integrationTests) {
      const startTime = Date.now();
      try {
        const passed = await test();
        details.push({
          name: `command-integration-${name}`,
          passed,
          duration: Date.now() - startTime
        });
        if (passed) successful++;
      } catch (error) {
        details.push({
          name: `command-integration-${name}`,
          passed: false,
          duration: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    const total = baseResult.total + integrationTests.length;
    const failed = total - successful;

    return {
      testType: TestType.Integration,
      passed: failed === 0,
      total,
      successful,
      failed,
      errors: [...baseResult.errors, ...details.filter(d => !d.passed).map(d => d.error || 'Unknown error')],
      details
    };
  }

  // Command-specific test helper methods
  private async hasExecuteMethod(): Promise<boolean> {
    const fileName = this.getCommandFileName();
    return this.checkFileContains(fileName, 'static async execute');
  }

  private async hasDefinitionMethod(): Promise<boolean> {
    const fileName = this.getCommandFileName();
    return this.checkFileContains(fileName, 'static getDefinition');
  }

  private async extendsBaseCommand(): Promise<boolean> {
    const fileName = this.getCommandFileName();
    return this.checkFileContains(fileName, 'extends BaseCommand');
  }

  private async commandNameMatches(): Promise<boolean> {
    if (!this.config || !('command' in this.config)) return false;
    const config = this.config as ContinuumCommandConfig;
    const expectedName = this.getExpectedCommandName();
    return config.command === expectedName;
  }

  private async canExecuteCommand(): Promise<boolean> {
    // Test if command can be executed (basic smoke test)
    try {
      const fileName = this.getCommandFileName();
      return await this.checkFileExists(fileName);
    } catch {
      return false;
    }
  }

  private async canRegisterCommand(): Promise<boolean> {
    // Test if command can be registered in system
    try {
      return await this.hasDefinitionMethod();
    } catch {
      return false;
    }
  }

  private async canValidateParameters(): Promise<boolean> {
    // Test if command has parameter validation
    const fileName = this.getCommandFileName();
    return this.checkFileContains(fileName, 'Params') || this.checkFileContains(fileName, 'parameters');
  }


  private getCommandFileName(): string {
    return `${this.getCommandClassName()}.ts`;
  }

  private getCommandClassName(): string {
    const expectedName = this.getExpectedCommandName();
    return expectedName.split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join('') + 'Command';
  }

  private getExpectedCommandName(): string {
    // Extract command name from directory path
    const parts = this.modulePath.split('/');
    return parts[parts.length - 1];
  }

  private async createCommandTemplate(fileName: string): Promise<void> {
    const className = this.getCommandClassName();
    const commandName = this.getExpectedCommandName();
    
    const template = `/**
 * ${className} - Generated command template
 */

import { BaseCommand } from '../../core/base-command/BaseCommand.js';
import { CommandResult, CommandContext } from '../../core/base-command/BaseCommand.js';

export interface ${className}Params {
  // Define command parameters here
}

export class ${className} extends BaseCommand {
  
  static async execute(params: ${className}Params, context: CommandContext): Promise<CommandResult> {
    try {
      // Implement command logic here
      
      return {
        success: true,
        message: '${commandName} command executed successfully',
        data: params
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  static getDefinition() {
    return {
      name: '${commandName}',
      description: '${className} implementation',
      category: 'Generated',
      parameters: {
        // Define parameters here
      },
      examples: [
        {
          command: '${commandName}',
          description: 'Basic ${commandName} usage'
        }
      ]
    };
  }
}`;

    const fs = await import('fs/promises');
    const path = await import('path');
    await fs.writeFile(path.join(this.modulePath, fileName), template);
  }

  private async createCommandTestTemplate(fileName: string): Promise<void> {
    const className = this.getCommandClassName();
    
    const template = `/**
 * Unit tests for ${className}
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { ${className} } from '../../${this.getCommandFileName().replace('.ts', '.js')}';

test('${className} can execute basic operation', async () => {
  const result = await ${className}.execute({}, { sessionId: 'test' });
  
  assert.equal(typeof result.success, 'boolean');
  assert.equal(typeof result.message, 'string');
});

test('${className} has valid definition', () => {
  const definition = ${className}.getDefinition();
  
  assert.equal(typeof definition.name, 'string');
  assert.equal(typeof definition.description, 'string');
  assert.equal(typeof definition.category, 'string');
});`;

    const fs = await import('fs/promises');
    const path = await import('path');
    await fs.mkdir(path.dirname(path.join(this.modulePath, fileName)), { recursive: true });
    await fs.writeFile(path.join(this.modulePath, fileName), template);
  }
}