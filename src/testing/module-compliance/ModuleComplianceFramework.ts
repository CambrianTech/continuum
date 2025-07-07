/**
 * Universal Module Compliance Testing Framework
 * 
 * Uses ContinuumConfig types as contracts for automated module validation.
 * Each module gets compliance tests based on its configuration type.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { 
  ContinuumConfig, 
  ContinuumWidgetConfig,
  ContinuumPackageUtils,
  PackageJson 
} from '../../types/ContinuumPackage';

export interface ModuleValidationResult {
  modulePath: string;
  moduleId: string;
  configType: 'command' | 'daemon' | 'widget' | 'module';
  isValid: boolean;
  errors: string[];
  warnings: string[];
  config: ContinuumConfig;
}

export interface ComplianceTestSuite {
  baseTests: ComplianceTest[];
  commandTests: ComplianceTest[];
  daemonTests: ComplianceTest[];
  widgetTests: ComplianceTest[];
  moduleTests: ComplianceTest[];
}

export interface ComplianceTest {
  name: string;
  description: string;
  validate: (modulePath: string, config: ContinuumConfig) => Promise<ComplianceTestResult>;
}

export interface ComplianceTestResult {
  passed: boolean;
  errors: string[];
  warnings: string[];
}

export class ModuleComplianceFramework {
  
  /**
   * Validate a single module against its configuration contract
   */
  static async validateModule(modulePath: string): Promise<ModuleValidationResult> {
    const errors: string[] = [];
    const warnings: string[] = [];
    
    try {
      // Load and validate package.json
      const packageJsonPath = path.join(modulePath, 'package.json');
      const packageData = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8')) as PackageJson;
      
      if (!packageData.continuum) {
        return {
          modulePath,
          moduleId: 'unknown',
          configType: 'module',
          isValid: false,
          errors: ['Missing continuum configuration in package.json'],
          warnings: [],
          config: {} as any
        };
      }
      
      const config = packageData.continuum;
      
      // Validate config structure
      if (!ContinuumPackageUtils.validateConfig(config)) {
        errors.push('Invalid continuum configuration structure');
      }
      
      const moduleType = ContinuumPackageUtils.getModuleType(config);
      const moduleId = ContinuumPackageUtils.getModuleId(config);
      
      // Run appropriate test suite based on module type
      const testSuite = this.getTestSuiteForType(moduleType);
      const testResults = await this.runTestSuite(modulePath, config, testSuite);
      
      errors.push(...testResults.errors);
      warnings.push(...testResults.warnings);
      
      return {
        modulePath,
        moduleId,
        configType: moduleType as any,
        isValid: errors.length === 0,
        errors,
        warnings,
        config
      };
      
    } catch (error) {
      return {
        modulePath,
        moduleId: 'unknown',
        configType: 'module',
        isValid: false,
        errors: [`Failed to validate module: ${error instanceof Error ? error.message : String(error)}`],
        warnings: [],
        config: {} as any
      };
    }
  }
  
  /**
   * Discover and validate all modules in the src directory
   */
  static async validateAllModules(srcPath: string): Promise<ModuleValidationResult[]> {
    const results: ModuleValidationResult[] = [];
    
    try {
      const modules = await this.discoverModules(srcPath);
      
      for (const modulePath of modules) {
        const result = await this.validateModule(modulePath);
        results.push(result);
      }
      
    } catch (error) {
      console.error('Failed to validate modules:', error);
    }
    
    return results;
  }
  
  /**
   * Discover all modules with package.json files
   */
  private static async discoverModules(srcPath: string): Promise<string[]> {
    const modules: string[] = [];
    
    async function scanDirectory(dir: string) {
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          if (entry.isDirectory()) {
            // Check if this directory has a package.json
            try {
              await fs.access(path.join(fullPath, 'package.json'));
              modules.push(fullPath);
            } catch {
              // No package.json, scan subdirectories
              await scanDirectory(fullPath);
            }
          }
        }
      } catch (error) {
        // Skip inaccessible directories
      }
    }
    
    await scanDirectory(srcPath);
    return modules;
  }
  
  /**
   * Get the appropriate test suite for a module type
   */
  private static getTestSuiteForType(moduleType: string): ComplianceTest[] {
    const allTests = this.getComplianceTestSuite();
    
    switch (moduleType) {
      case 'command':
        return [...allTests.baseTests, ...allTests.commandTests];
      case 'daemon':
        return [...allTests.baseTests, ...allTests.daemonTests];
      case 'widget':
        return [...allTests.baseTests, ...allTests.widgetTests];
      default:
        return [...allTests.baseTests, ...allTests.moduleTests];
    }
  }
  
  /**
   * Run a test suite against a module
   */
  private static async runTestSuite(
    modulePath: string, 
    config: ContinuumConfig, 
    tests: ComplianceTest[]
  ): Promise<{ errors: string[], warnings: string[] }> {
    const errors: string[] = [];
    const warnings: string[] = [];
    
    for (const test of tests) {
      try {
        const result = await test.validate(modulePath, config);
        
        if (!result.passed) {
          errors.push(`Test '${test.name}' failed`);
        }
        
        errors.push(...result.errors);
        warnings.push(...result.warnings);
        
      } catch (error) {
        errors.push(`Test '${test.name}' threw error: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    
    return { errors, warnings };
  }
  
  /**
   * Define the complete compliance test suite
   */
  private static getComplianceTestSuite(): ComplianceTestSuite {
    return {
      baseTests: [
        {
          name: 'package-json-structure',
          description: 'Validates package.json has required fields',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            try {
              const packageJsonPath = path.join(modulePath, 'package.json');
              const packageData = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8')) as PackageJson;
              
              // Required fields
              if (!packageData.name) errors.push('Missing name field');
              if (!packageData.version) errors.push('Missing version field');
              if (!packageData.description) errors.push('Missing description field');
              if (!packageData.main) errors.push('Missing main field');
              if (!packageData.type) warnings.push('Missing type field (recommended: "module")');
              
              // Continuum-specific validation
              if (!packageData.continuum) errors.push('Missing continuum configuration');
              
            } catch (error) {
              errors.push(`Cannot read package.json: ${error instanceof Error ? error.message : String(error)}`);
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        },
        
        {
          name: 'readme-documentation',
          description: 'Validates module has README.md documentation',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            try {
              await fs.access(path.join(modulePath, 'README.md'));
            } catch {
              warnings.push('Missing README.md documentation');
            }
            
            return { passed: true, errors, warnings };
          }
        },
        
        {
          name: 'test-directory-structure',
          description: 'Validates module has proper test directory structure',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            try {
              const testDir = path.join(modulePath, 'test');
              await fs.access(testDir);
              
              // Check for unit and integration test directories
              try {
                await fs.access(path.join(testDir, 'unit'));
              } catch {
                warnings.push('Missing test/unit directory');
              }
              
              try {
                await fs.access(path.join(testDir, 'integration'));
              } catch {
                warnings.push('Missing test/integration directory');
              }
              
            } catch {
              warnings.push('Missing test directory');
            }
            
            return { passed: true, errors, warnings };
          }
        },
        
        {
          name: 'dependency-validation',
          description: 'Validates declared dependencies exist',
          validate: async (_modulePath: string, config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            // This would check if declared dependencies in config.dependencies
            // actually exist in the project structure
            for (const dep of config.dependencies) {
              // TODO: Implement dependency resolution check
              // For now, just validate the format
              if (!dep || typeof dep !== 'string') {
                errors.push(`Invalid dependency: ${dep}`);
              }
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        }
      ],
      
      commandTests: [
        {
          name: 'command-implementation',
          description: 'Validates command has proper execute method',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            // const commandConfig = config as ContinuumCommandConfig;
            
            try {
              const mainFile = path.join(modulePath, path.basename(modulePath) + '.ts');
              await fs.access(mainFile);
              
              const content = await fs.readFile(mainFile, 'utf-8');
              
              // Check for execute method
              if (!content.includes('static async execute')) {
                errors.push('Command missing static async execute method');
              }
              
              // Check for getDefinition method
              if (!content.includes('static getDefinition')) {
                errors.push('Command missing static getDefinition method');
              }
              
            } catch {
              errors.push('Missing main command implementation file');
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        }
      ],
      
      daemonTests: [
        {
          name: 'daemon-lifecycle',
          description: 'Validates daemon has proper lifecycle methods',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            // const daemonConfig = config as ContinuumDaemonConfig;
            
            try {
              const mainFile = path.join(modulePath, path.basename(modulePath) + '.ts');
              await fs.access(mainFile);
              
              const content = await fs.readFile(mainFile, 'utf-8');
              
              // Check for required daemon methods
              if (!content.includes('async onStart')) {
                errors.push('Daemon missing onStart method');
              }
              
              if (!content.includes('async onStop')) {
                errors.push('Daemon missing onStop method');
              }
              
            } catch {
              errors.push('Missing main daemon implementation file');
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        }
      ],
      
      widgetTests: [
        {
          name: 'widget-ui-assets',
          description: 'Validates widget has UI assets declared in config',
          validate: async (modulePath: string, config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            const widgetConfig = config as ContinuumWidgetConfig;
            
            if (widgetConfig.ui?.template) {
              try {
                await fs.access(path.join(modulePath, widgetConfig.ui.template));
              } catch {
                errors.push(`Template file not found: ${widgetConfig.ui.template}`);
              }
            }
            
            if (widgetConfig.ui?.styles) {
              for (const styleFile of widgetConfig.ui.styles) {
                try {
                  await fs.access(path.join(modulePath, styleFile));
                } catch {
                  warnings.push(`Style file not found: ${styleFile}`);
                }
              }
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        }
      ],
      
      moduleTests: [
        {
          name: 'module-exports',
          description: 'Validates module has proper exports',
          validate: async (modulePath: string, _config: ContinuumConfig) => {
            const errors: string[] = [];
            const warnings: string[] = [];
            
            // const moduleConfig = config as ContinuumModuleConfig;
            
            try {
              const mainFile = path.join(modulePath, path.basename(modulePath) + '.ts');
              await fs.access(mainFile);
              
              const content = await fs.readFile(mainFile, 'utf-8');
              
              // Check for exports
              if (!content.includes('export')) {
                warnings.push('Module should export its functionality');
              }
              
            } catch {
              errors.push('Missing main module implementation file');
            }
            
            return { passed: errors.length === 0, errors, warnings };
          }
        }
      ]
    };
  }
  
  /**
   * Generate a compliance report for all modules
   */
  static async generateComplianceReport(srcPath: string): Promise<string> {
    const results = await this.validateAllModules(srcPath);
    
    let report = '# Continuum Module Compliance Report\n\n';
    
    const validModules = results.filter(r => r.isValid);
    const invalidModules = results.filter(r => !r.isValid);
    
    report += `## Summary\n`;
    report += `- Total modules: ${results.length}\n`;
    report += `- Valid modules: ${validModules.length}\n`;
    report += `- Invalid modules: ${invalidModules.length}\n`;
    report += `- Compliance rate: ${((validModules.length / results.length) * 100).toFixed(1)}%\n\n`;
    
    if (invalidModules.length > 0) {
      report += `## Issues Found\n\n`;
      
      for (const module of invalidModules) {
        report += `### ${module.moduleId} (${module.configType})\n`;
        report += `Path: \`${module.modulePath}\`\n\n`;
        
        if (module.errors.length > 0) {
          report += `**Errors:**\n`;
          for (const error of module.errors) {
            report += `- ${error}\n`;
          }
          report += `\n`;
        }
        
        if (module.warnings.length > 0) {
          report += `**Warnings:**\n`;
          for (const warning of module.warnings) {
            report += `- ${warning}\n`;
          }
          report += `\n`;
        }
      }
    }
    
    report += `## Valid Modules\n\n`;
    for (const module of validModules) {
      report += `- ✅ **${module.moduleId}** (${module.configType}) - \`${module.modulePath}\`\n`;
    }
    
    return report;
  }
}