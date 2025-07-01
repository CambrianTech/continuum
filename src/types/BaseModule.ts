/**
 * Base Module - Foundation class for all Continuum modules
 * 
 * Every module extends this and implements validate() through inheritance chain
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { ContinuumConfig, ContinuumPackageUtils, PackageJson } from './ContinuumPackage.js';

export interface ValidationResult {
  isValid: boolean;
  errors: string[];
  warnings: string[];
  checks: Record<string, boolean>;
}

export enum TestType {
  Unit = 'unit',
  Integration = 'integration',
  Validation = 'validation',
  Migration = 'migration',
  All = 'all'
}

export interface TestResult {
  testType: TestType;
  passed: boolean;
  total: number;
  successful: number;
  failed: number;
  errors: string[];
  details: TestDetail[];
}

export interface TestDetail {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
}

export interface MigrationResult {
  migrated: boolean;
  changes: string[];
  errors: string[];
  versionFrom?: string;
  versionTo?: string;
  migrationSteps?: MigrationStep[];
}

export interface MigrationStep {
  step: string;
  description: string;
  completed: boolean;
  error?: string;
}

export interface BaseMigrationStruct {
  versionFrom: string;
  versionTo: string;
  migrationName: string;
  description: string;
  requiresBackup?: boolean;
  rollbackSupported?: boolean;
}

// Specialized validation results for different module types
export interface CommandValidationResult extends ValidationResult {
  commandChecks?: {
    hasExecuteMethod?: boolean;
    hasGetDefinitionMethod?: boolean;
    extendsBaseCommand?: boolean;
  };
}

export interface DaemonValidationResult extends ValidationResult {
  daemonChecks?: {
    hasOnStartMethod?: boolean;
    hasOnStopMethod?: boolean;
    extendsBaseDaemon?: boolean;
    hasHealthEndpoint?: boolean;
  };
}

export interface WidgetValidationResult extends ValidationResult {
  widgetChecks?: {
    hasUIAssets?: boolean;
    hasTemplateFile?: boolean;
    hasStyleFiles?: boolean;
    extendsBaseWidget?: boolean;
  };
}

// Specialized migration results for different module types
export interface CommandMigrationResult extends MigrationResult {
  commandMigrations?: string[];
}

export interface DaemonMigrationResult extends MigrationResult {
  daemonMigrations?: string[];
}

export interface WidgetMigrationResult extends MigrationResult {
  widgetMigrations?: string[];
}

export class BaseModule {
  protected modulePath: string;
  protected config?: ContinuumConfig;
  protected packageJson?: PackageJson;

  constructor(modulePath: string) {
    this.modulePath = modulePath;
  }

  /**
   * Unified test method - handles all test types through inheritance
   */
  async test(testType: TestType = TestType.All): Promise<TestResult[]> {
    const results: TestResult[] = [];

    try {
      if (testType === TestType.All || testType === TestType.Validation) {
        results.push(await this.runValidationTests());
      }

      if (testType === TestType.All || testType === TestType.Unit) {
        results.push(await this.runUnitTests());
      }

      if (testType === TestType.All || testType === TestType.Integration) {
        results.push(await this.runIntegrationTests());
      }

      if (testType === TestType.All || testType === TestType.Migration) {
        results.push(await this.runMigrationTests());
      }

    } catch (error) {
      results.push({
        testType,
        passed: false,
        total: 1,
        successful: 0,
        failed: 1,
        errors: [error instanceof Error ? error.message : String(error)],
        details: []
      });
    }

    return results;
  }

  /**
   * Base validation tests - converted to TestResult format
   */
  protected async runValidationTests(): Promise<TestResult> {
    const startTime = Date.now();
    const validation = await this.validate();
    const duration = Date.now() - startTime;

    const details: TestDetail[] = Object.entries(validation.checks).map(([name, passed]) => {
      const detail: TestDetail = {
        name: `validation-${name}`,
        passed,
        duration: duration / Object.keys(validation.checks).length
      };
      if (!passed) {
        detail.error = `Check failed: ${name}`;
      }
      return detail;
    });

    return {
      testType: TestType.Validation,
      passed: validation.isValid,
      total: Object.keys(validation.checks).length,
      successful: Object.values(validation.checks).filter(Boolean).length,
      failed: Object.values(validation.checks).filter(check => !check).length,
      errors: validation.errors,
      details
    };
  }

  /**
   * Base unit tests - subclasses override to add their specific tests
   */
  protected async runUnitTests(): Promise<TestResult> {
    // Base module unit tests - check basic structure
    const tests = [
      { name: 'package-json-exists', test: () => this.checkFileExists('package.json') },
      { name: 'has-continuum-config', test: () => !!this.packageJson?.continuum },
      { name: 'module-id-valid', test: () => this.getModuleId() !== 'unknown' }
    ];

    const details: TestDetail[] = [];
    let successful = 0;

    for (const { name, test } of tests) {
      const startTime = Date.now();
      try {
        const passed = await test();
        const detail: TestDetail = {
          name,
          passed,
          duration: Date.now() - startTime
        };
        if (!passed) {
          detail.error = `Test failed: ${name}`;
        }
        details.push(detail);
        if (passed) successful++;
      } catch (error) {
        details.push({
          name,
          passed: false,
          duration: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return {
      testType: TestType.Unit,
      passed: successful === tests.length,
      total: tests.length,
      successful,
      failed: tests.length - successful,
      errors: details.filter(d => !d.passed).map(d => d.error || 'Unknown error'),
      details
    };
  }

  /**
   * Base integration tests - subclasses override to add their specific tests
   */
  protected async runIntegrationTests(): Promise<TestResult> {
    // Base integration tests - check module can work with system
    const tests = [
      { name: 'can-load-config', test: () => this.canLoadConfiguration() },
      { name: 'dependencies-resolvable', test: () => this.canResolveDependencies() }
    ];

    const details: TestDetail[] = [];
    let successful = 0;

    for (const { name, test } of tests) {
      const startTime = Date.now();
      try {
        const passed = await test();
        details.push({
          name,
          passed,
          duration: Date.now() - startTime
        });
        if (passed) successful++;
      } catch (error) {
        details.push({
          name,
          passed: false,
          duration: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return {
      testType: TestType.Integration,
      passed: successful === tests.length,
      total: tests.length,
      successful,
      failed: tests.length - successful,
      errors: details.filter(d => !d.passed).map(d => d.error || 'Unknown error'),
      details
    };
  }

  /**
   * Migration tests - test migration scenarios
   */
  protected async runMigrationTests(): Promise<TestResult> {
    const tests = [
      { name: 'can-backup', test: () => this.canCreateBackup() },
      { name: 'migration-plan-valid', test: () => this.hasMigrationPlan() }
    ];

    const details: TestDetail[] = [];
    let successful = 0;

    for (const { name, test } of tests) {
      const startTime = Date.now();
      try {
        const passed = await test();
        details.push({
          name,
          passed,
          duration: Date.now() - startTime
        });
        if (passed) successful++;
      } catch (error) {
        details.push({
          name,
          passed: false,
          duration: Date.now() - startTime,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return {
      testType: TestType.Migration,
      passed: successful === tests.length,
      total: tests.length,
      successful,
      failed: tests.length - successful,
      errors: details.filter(d => !d.passed).map(d => d.error || 'Unknown error'),
      details
    };
  }

  // Helper methods for tests
  protected async canLoadConfiguration(): Promise<boolean> {
    try {
      await this.loadPackageJson();
      return !!this.packageJson?.continuum;
    } catch {
      return false;
    }
  }

  protected async canResolveDependencies(): Promise<boolean> {
    if (!this.config?.dependencies) return true;
    // For now, just check they're strings - real dependency resolution comes later
    return this.config.dependencies.every(dep => typeof dep === 'string');
  }

  protected async canCreateBackup(): Promise<boolean> {
    // Check if we can read the module for backup
    try {
      await this.loadPackageJson();
      return true;
    } catch {
      return false;
    }
  }

  protected async hasMigrationPlan(): Promise<boolean> {
    // Base modules always have a migration plan (even if empty)
    return true;
  }

  /**
   * Base validation - all modules must have these
   */
  async validate(): Promise<ValidationResult> {
    const errors: string[] = [];
    const warnings: string[] = [];
    const checks: Record<string, boolean> = {};

    try {
      // Load and validate package.json
      await this.loadPackageJson();
      
      // Base structural requirements
      checks.hasPackageJson = await this.checkFileExists('package.json');
      checks.hasValidName = !!(this.packageJson?.name);
      checks.hasValidVersion = !!(this.packageJson?.version);
      checks.hasDescription = !!(this.packageJson?.description);
      checks.hasContinuumConfig = !!(this.packageJson?.continuum);
      
      if (this.packageJson?.continuum) {
        this.config = this.packageJson.continuum;
        checks.hasValidConfig = ContinuumPackageUtils.validateConfig(this.config);
      } else {
        checks.hasValidConfig = false;
        errors.push('Missing continuum configuration');
      }

      // Standard directory structure
      checks.hasTestDirectory = await this.checkDirectoryExists('test');
      checks.hasUnitTests = await this.checkDirectoryExists('test/unit');
      checks.hasIntegrationTests = await this.checkDirectoryExists('test/integration');
      
      // Optional but recommended
      if (!await this.checkFileExists('README.md')) {
        warnings.push('Missing README.md documentation');
      }

    } catch (error) {
      errors.push(`Base validation failed: ${error instanceof Error ? error.message : String(error)}`);
    }

    const failedChecks = Object.values(checks).filter(check => !check).length;
    const isValid = errors.length === 0 && failedChecks === 0;

    return { isValid, errors, warnings, checks };
  }


  /**
   * Smart migration - delegates to ModuleMigration class
   */
  async migrate(): Promise<MigrationResult> {
    const { ModuleMigration } = await import('./ModuleMigration.js');
    const migration = new ModuleMigration(this.modulePath, this.packageJson);
    return await migration.migrate();
  }

  /**
   * Static method for system-wide mass migration - delegates to SystemMigration
   */
  static async migrateAllModules(rootPath: string = './src'): Promise<MigrationResult[]> {
    const { SystemMigration } = await import('./SystemMigration.js');
    return await SystemMigration.migrateAllModules(rootPath);
  }

  /**
   * Static method for category migration - delegates to SystemMigration
   */
  static async migrateByCategory(category: 'commands' | 'daemons' | 'ui', rootPath: string = './src'): Promise<MigrationResult[]> {
    const { SystemMigration } = await import('./SystemMigration.js');
    return await SystemMigration.migrateByCategory(category, rootPath);
  }

  protected async fixDirectories(result: MigrationResult): Promise<void> {
    const dirs = ['test', 'test/unit', 'test/integration'];
    
    for (const dir of dirs) {
      if (!await this.checkDirectoryExists(dir)) {
        await fs.mkdir(path.join(this.modulePath, dir), { recursive: true });
        result.changes.push(`Created ${dir} directory`);
        result.migrated = true;
      }
    }
  }

  protected async fixFiles(result: MigrationResult): Promise<void> {
    // Fix README.md
    if (!await this.checkFileExists('README.md')) {
      await this.generateReadme();
      result.changes.push('Generated README.md');
      result.migrated = true;
    }
  }

  protected async fixPackageJson(result: MigrationResult): Promise<void> {
    if (!this.packageJson) return;

    let updated = false;

    // Add test scripts if missing
    if (!this.packageJson.scripts?.test) {
      if (!this.packageJson.scripts) this.packageJson.scripts = {};
      this.packageJson.scripts.test = 'node --test test/';
      this.packageJson.scripts['test:unit'] = 'node --test test/unit/';
      this.packageJson.scripts['test:integration'] = 'node --test test/integration/';
      updated = true;
    }

    if (updated) {
      await this.savePackageJson();
      result.changes.push('Added test scripts to package.json');
      result.migrated = true;
    }
  }

  /**
   * Generate missing files (README.md, etc.)
   */
  protected async generateMissingFiles(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('generate-files', 'Generate missing files');
    
    try {
      // Generate README.md if missing
      if (!await this.checkFileExists('README.md')) {
        await this.generateReadme();
        result.changes.push('Generated README.md');
        result.migrated = true;
      }

      step.completed = true;
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`File generation failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  /**
   * Protected migration methods that subclasses can use
   */
  protected async migrateStandardDirectories(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('standard-directories', 'Create standard directory structure');
    
    try {
      if (!await this.checkDirectoryExists('test')) {
        await fs.mkdir(path.join(this.modulePath, 'test'), { recursive: true });
        result.changes.push('Created test directory');
      }

      if (!await this.checkDirectoryExists('test/unit')) {
        await fs.mkdir(path.join(this.modulePath, 'test/unit'), { recursive: true });
        result.changes.push('Created test/unit directory');
      }

      if (!await this.checkDirectoryExists('test/integration')) {
        await fs.mkdir(path.join(this.modulePath, 'test/integration'), { recursive: true });
        result.changes.push('Created test/integration directory');
      }

      step.completed = true;
      result.migrated = result.migrated || result.changes.length > 0;
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`Directory migration failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  protected async migratePackageJsonStructure(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('package-json', 'Update package.json structure');
    
    try {
      await this.loadPackageJson();
      let updated = false;

      if (this.packageJson) {
        // Ensure basic fields exist
        if (!this.packageJson.description) {
          this.packageJson.description = `${this.getModuleId()} module`;
          updated = true;
        }

        if (!this.packageJson.version) {
          this.packageJson.version = '1.0.0';
          updated = true;
        }

        if (updated) {
          await this.savePackageJson();
          result.changes.push('Updated package.json structure');
          result.migrated = true;
        }
      }

      step.completed = true;
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`Package.json migration failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  protected async migrateBasicConfiguration(result: MigrationResult): Promise<void> {
    const step = this.createMigrationStep('basic-config', 'Ensure basic configuration is present');
    
    try {
      if (!this.packageJson?.continuum) {
        result.errors.push('Missing continuum configuration - manual intervention required');
        step.error = 'Missing continuum configuration';
      } else {
        step.completed = true;
      }
    } catch (error) {
      step.error = error instanceof Error ? error.message : String(error);
      result.errors.push(`Configuration migration failed: ${step.error}`);
    }
    
    if (result.migrationSteps) result.migrationSteps.push(step);
  }

  protected createMigrationStep(step: string, description: string): MigrationStep {
    return { step, description, completed: false };
  }

  protected getCurrentVersion(): string {
    return this.packageJson?.version || '0.0.0';
  }

  protected getTargetVersion(): string {
    return this.packageJson?.version || '1.0.0';
  }

  protected async savePackageJson(): Promise<void> {
    if (this.packageJson) {
      const packagePath = path.join(this.modulePath, 'package.json');
      await fs.writeFile(packagePath, JSON.stringify(this.packageJson, null, 2));
    }
  }

  /**
   * Generate missing module structure without overwriting existing files
   */
  async generateStructure(): Promise<MigrationResult> {
    const result: MigrationResult = {
      migrated: false,
      changes: [],
      errors: [],
      versionFrom: this.getCurrentVersion(),
      versionTo: this.getTargetVersion(),
      migrationSteps: []
    };

    try {
      // Generate README.md if missing
      if (!await this.checkFileExists('README.md')) {
        await this.generateReadme();
        result.changes.push('Generated README.md');
        result.migrated = true;
      }

      // Generate test directories if missing
      await this.generateTestStructure(result);

      // Generate package.json improvements if needed
      await this.generatePackageJsonImprovements(result);

    } catch (error) {
      result.errors.push(`Structure generation failed: ${error instanceof Error ? error.message : String(error)}`);
    }

    return result;
  }

  protected async generateReadme(): Promise<void> {
    const moduleId = this.getModuleId();
    const description = this.packageJson?.description || `${moduleId} module`;
    
    const template = `# ${moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')}

${description}

## Usage
\`\`\`bash
# Add usage examples here
\`\`\`

## Configuration
\`\`\`json
${this.packageJson?.continuum ? JSON.stringify(this.packageJson.continuum, null, 2) : '// Add continuum configuration'}
\`\`\`

## Testing
\`\`\`bash
npm test
\`\`\`

## Auto-generated file - customize as needed
`;

    await fs.writeFile(path.join(this.modulePath, 'README.md'), template);
  }

  protected async generateTestStructure(result: MigrationResult): Promise<void> {
    const testDirs = ['test', 'test/unit', 'test/integration'];
    
    for (const dir of testDirs) {
      if (!await this.checkDirectoryExists(dir)) {
        await fs.mkdir(path.join(this.modulePath, dir), { recursive: true });
        result.changes.push(`Created ${dir} directory`);
        result.migrated = true;
      }
    }
  }

  protected async generatePackageJsonImprovements(result: MigrationResult): Promise<void> {
    if (!this.packageJson) return;

    let updated = false;

    // Add test scripts if missing
    if (!this.packageJson.scripts) {
      this.packageJson.scripts = {};
    }

    if (!this.packageJson.scripts.test) {
      this.packageJson.scripts.test = 'node --test test/';
      this.packageJson.scripts['test:unit'] = 'node --test test/unit/';
      this.packageJson.scripts['test:integration'] = 'node --test test/integration/';
      updated = true;
    }

    if (updated) {
      await this.savePackageJson();
      result.changes.push('Added test scripts to package.json');
      result.migrated = true;
    }
  }

  /**
   * Get module type for specialized validation
   */
  getModuleType(): string {
    if (!this.config) return 'unknown';
    return ContinuumPackageUtils.getModuleType(this.config);
  }

  /**
   * Get module identifier
   */
  getModuleId(): string {
    if (!this.config) return 'unknown';
    return ContinuumPackageUtils.getModuleId(this.config);
  }

  /**
   * Protected helper methods for subclasses
   */
  protected async loadPackageJson(): Promise<void> {
    const packagePath = path.join(this.modulePath, 'package.json');
    const content = await fs.readFile(packagePath, 'utf-8');
    this.packageJson = JSON.parse(content);
  }

  protected async checkFileExists(fileName: string): Promise<boolean> {
    try {
      await fs.access(path.join(this.modulePath, fileName));
      return true;
    } catch {
      return false;
    }
  }

  protected async checkDirectoryExists(dirName: string): Promise<boolean> {
    try {
      const stat = await fs.stat(path.join(this.modulePath, dirName));
      return stat.isDirectory();
    } catch {
      return false;
    }
  }

  protected async checkFileContains(fileName: string, content: string): Promise<boolean> {
    try {
      const fileContent = await fs.readFile(path.join(this.modulePath, fileName), 'utf-8');
      return fileContent.includes(content);
    } catch {
      return false;
    }
  }

  protected combineValidationResults(baseResult: ValidationResult, ...additionalResults: ValidationResult[]): ValidationResult {
    const combined: ValidationResult = {
      isValid: baseResult.isValid,
      errors: [...baseResult.errors],
      warnings: [...baseResult.warnings],
      checks: { ...baseResult.checks }
    };

    for (const result of additionalResults) {
      combined.isValid = combined.isValid && result.isValid;
      combined.errors.push(...result.errors);
      combined.warnings.push(...result.warnings);
      Object.assign(combined.checks, result.checks);
    }

    return combined;
  }

  protected combineMigrationResults(baseResult: MigrationResult, ...additionalResults: MigrationResult[]): MigrationResult {
    const combined: MigrationResult = {
      migrated: baseResult.migrated,
      changes: [...baseResult.changes],
      errors: [...baseResult.errors]
    };

    for (const result of additionalResults) {
      combined.migrated = combined.migrated || result.migrated;
      combined.changes.push(...result.changes);
      combined.errors.push(...result.errors);
    }

    return combined;
  }
}