/**
 * System-Wide Validation Command
 * 
 * Validates that all modules follow consistent directory structure,
 * package.json patterns, and implement their declared capabilities.
 */

import { BaseCommand } from '../../core/base-command/BaseCommand.js';
import { CommandResult, CommandContext } from '../../core/base-command/BaseCommand.js';
import { ModuleComplianceFramework } from '../../../testing/module-compliance/ModuleComplianceFramework.js';
import { SelfValidatingModule } from '../../../testing/self-validating/SelfValidatingModule.js';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface ValidateSystemParams {
  srcPath?: string;
  generateTests?: boolean;
  generateReport?: boolean;
  fixable?: boolean;
  verbose?: boolean;
}

export class ValidateSystemCommand extends BaseCommand {
  
  static async execute(params: ValidateSystemParams, _context: CommandContext): Promise<CommandResult> {
    const srcPath = params.srcPath || './src';
    
    try {
      console.log('🔍 Starting system-wide validation...');
      
      // Run comprehensive validation
      const complianceResults = await ModuleComplianceFramework.validateAllModules(srcPath);
      const selfValidationResults = await this.runSelfValidationForAllModules(srcPath);
      
      // Generate reports if requested
      if (params.generateReport) {
        await this.generateValidationReports(srcPath, complianceResults, selfValidationResults);
      }
      
      // Generate self-tests if requested
      if (params.generateTests) {
        await SelfValidatingModule.generateAllSelfTests(srcPath);
        console.log('✅ Generated self-validation tests for all modules');
      }
      
      // Analyze results
      const totalModules = complianceResults.length;
      const compliantModules = complianceResults.filter(r => r.isValid).length;
      const complianceRate = ((compliantModules / totalModules) * 100).toFixed(1);
      
      console.log(`\n📊 Validation Summary:`);
      console.log(`   Total modules: ${totalModules}`);
      console.log(`   Compliant: ${compliantModules}`);
      console.log(`   Non-compliant: ${totalModules - compliantModules}`);
      console.log(`   Compliance rate: ${complianceRate}%`);
      
      // Detailed results if verbose
      if (params.verbose) {
        await this.printDetailedResults(complianceResults, selfValidationResults);
      }
      
      // Check if system passes validation
      const systemIsValid = complianceRate === '100.0';
      
      if (systemIsValid) {
        console.log('\n✅ System validation PASSED - All modules are compliant');
        return {
          success: true,
          message: 'System validation PASSED - All modules are compliant',
          data: {
            totalModules,
            compliantModules,
            complianceRate: parseFloat(complianceRate),
            systemValid: true
          }
        };
      } else {
        console.log('\n❌ System validation FAILED - Some modules are non-compliant');
        
        // Show fixable issues
        if (params.fixable) {
          await this.suggestFixes(complianceResults);
        }
        
        return {
          success: false,
          message: `System validation failed: ${complianceRate}% compliance rate`,
          error: `System validation failed: ${complianceRate}% compliance rate`,
          data: {
            totalModules,
            compliantModules,
            complianceRate: parseFloat(complianceRate),
            systemValid: false,
            nonCompliantModules: complianceResults.filter(r => !r.isValid)
          }
        };
      }
      
    } catch (error) {
      return {
        success: false,
        error: `System validation failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }
  
  /**
   * Run self-validation for all modules
   */
  private static async runSelfValidationForAllModules(srcPath: string) {
    const modules = await this.discoverModules(srcPath);
    const results = [];
    
    for (const modulePath of modules) {
      try {
        const result = await SelfValidatingModule.validateSelf(modulePath);
        results.push(result);
      } catch (error) {
        console.warn(`Failed to self-validate ${modulePath}:`, error);
      }
    }
    
    return results;
  }
  
  /**
   * Generate comprehensive validation reports
   */
  private static async generateValidationReports(
    srcPath: string, 
    complianceResults: any[], 
    selfValidationResults: any[]
  ) {
    const reportsDir = path.join(srcPath, '..', 'reports');
    await fs.mkdir(reportsDir, { recursive: true });
    
    // Generate compliance report
    const complianceReport = await ModuleComplianceFramework.generateComplianceReport(srcPath);
    await fs.writeFile(path.join(reportsDir, 'module-compliance.md'), complianceReport);
    
    // Generate self-validation report
    const selfValidationReport = this.generateSelfValidationReport(selfValidationResults);
    await fs.writeFile(path.join(reportsDir, 'self-validation.md'), selfValidationReport);
    
    // Generate system structure report
    const structureReport = await this.generateSystemStructureReport(srcPath);
    await fs.writeFile(path.join(reportsDir, 'system-structure.md'), structureReport);
    
    console.log(`📄 Reports generated in: ${reportsDir}`);
  }
  
  /**
   * Generate self-validation report
   */
  private static generateSelfValidationReport(results: any[]): string {
    let report = '# Self-Validation Report\n\n';
    
    const compliant = results.filter(r => r.isCompliant);
    const nonCompliant = results.filter(r => !r.isCompliant);
    
    report += `## Summary\n`;
    report += `- Total modules: ${results.length}\n`;
    report += `- Self-compliant: ${compliant.length}\n`;
    report += `- Non-compliant: ${nonCompliant.length}\n`;
    report += `- Self-validation rate: ${((compliant.length / results.length) * 100).toFixed(1)}%\n\n`;
    
    if (nonCompliant.length > 0) {
      report += `## Modules Failing Self-Validation\n\n`;
      
      for (const module of nonCompliant) {
        report += `### ${module.moduleId} (${module.configType})\n`;
        report += `Path: \`${module.modulePath}\`\n`;
        report += `Tests: ${module.testsPassed}/${module.testsGenerated} passed\n\n`;
        
        // Failed capability tests
        const failedCapabilities = module.capabilityTests.filter((t: any) => !t.implemented);
        if (failedCapabilities.length > 0) {
          report += `**Failed Capability Tests:**\n`;
          for (const cap of failedCapabilities) {
            report += `- ❌ ${cap.capability}: ${cap.errors.join(', ')}\n`;
          }
          report += `\n`;
        }
        
        // Failed structure tests
        const failedStructure = module.structureTests.filter((t: any) => !t.met);
        if (failedStructure.length > 0) {
          report += `**Failed Structure Tests:**\n`;
          for (const struct of failedStructure) {
            report += `- ❌ ${struct.requirement}: ${struct.details}\n`;
          }
          report += `\n`;
        }
      }
    }
    
    return report;
  }
  
  /**
   * Generate system structure overview report
   */
  private static async generateSystemStructureReport(srcPath: string): Promise<string> {
    let report = '# System Structure Report\n\n';
    
    const modules = await this.discoverModules(srcPath);
    
    // Group modules by category
    const modulesByCategory: Record<string, any[]> = {};
    
    for (const modulePath of modules) {
      try {
        const packageData = JSON.parse(await fs.readFile(path.join(modulePath, 'package.json'), 'utf-8'));
        const config = packageData.continuum;
        
        if (config) {
          const category = config.category || 'Uncategorized';
          if (!modulesByCategory[category]) {
            modulesByCategory[category] = [];
          }
          modulesByCategory[category].push({
            path: modulePath,
            config,
            name: packageData.name
          });
        }
      } catch {
        // Skip modules without valid config
      }
    }
    
    report += `## Modules by Category\n\n`;
    
    for (const [category, modules] of Object.entries(modulesByCategory)) {
      report += `### ${category} (${modules.length} modules)\n\n`;
      
      for (const module of modules) {
        const relPath = path.relative(srcPath, module.path);
        report += `- **${module.config.command || module.config.daemon || module.config.widget || module.config.module}**\n`;
        report += `  - Path: \`${relPath}\`\n`;
        report += `  - Capabilities: ${module.config.capabilities.join(', ')}\n`;
        report += `  - Dependencies: ${module.config.dependencies.join(', ')}\n\n`;
      }
    }
    
    return report;
  }
  
  /**
   * Print detailed validation results
   */
  private static async printDetailedResults(complianceResults: any[], selfValidationResults: any[]) {
    console.log('\n📋 Detailed Validation Results:\n');
    
    for (const result of complianceResults) {
      const selfResult = selfValidationResults.find(s => s.modulePath === result.modulePath);
      
      const status = result.isValid ? '✅' : '❌';
      const selfStatus = selfResult?.isCompliant ? '✅' : '❌';
      
      console.log(`${status} ${selfStatus} ${result.moduleId} (${result.configType})`);
      console.log(`    Path: ${path.relative('./src', result.modulePath)}`);
      
      if (!result.isValid && result.errors.length > 0) {
        console.log(`    Compliance errors: ${result.errors.slice(0, 2).join(', ')}${result.errors.length > 2 ? '...' : ''}`);
      }
      
      if (selfResult && !selfResult.isCompliant) {
        console.log(`    Self-validation: ${selfResult.testsPassed}/${selfResult.testsGenerated} tests passed`);
      }
      
      console.log('');
    }
  }
  
  /**
   * Suggest automatic fixes for common issues
   */
  private static async suggestFixes(complianceResults: any[]) {
    console.log('\n🔧 Suggested Fixes:\n');
    
    const nonCompliant = complianceResults.filter(r => !r.isValid);
    
    for (const module of nonCompliant) {
      console.log(`📁 ${module.moduleId}:`);
      
      for (const error of module.errors) {
        if (error.includes('Missing README.md')) {
          console.log(`   - Create README.md with module documentation`);
        }
        if (error.includes('Missing test directory')) {
          console.log(`   - mkdir -p ${path.join(module.modulePath, 'test', 'unit')}`);
          console.log(`   - mkdir -p ${path.join(module.modulePath, 'test', 'integration')}`);
        }
        if (error.includes('Missing main')) {
          console.log(`   - Create main implementation file following naming convention`);
        }
      }
      
      console.log('');
    }
  }
  
  /**
   * Discover modules with package.json
   */
  private static async discoverModules(srcPath: string): Promise<string[]> {
    const modules: string[] = [];
    
    async function scanDirectory(dir: string) {
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          if (entry.isDirectory()) {
            try {
              await fs.access(path.join(fullPath, 'package.json'));
              modules.push(fullPath);
            } catch {
              await scanDirectory(fullPath);
            }
          }
        }
      } catch {
        // Skip inaccessible directories
      }
    }
    
    await scanDirectory(srcPath);
    return modules;
  }
  
  static getDefinition() {
    return {
      name: 'validate-system',
      description: 'Validates that all modules follow consistent structure and implement declared capabilities',
      category: 'Testing',
      parameters: {
        srcPath: {
          type: 'string',
          description: 'Path to source directory (default: ./src)',
          required: false
        },
        generateTests: {
          type: 'boolean',
          description: 'Generate self-validation tests for all modules',
          required: false
        },
        generateReport: {
          type: 'boolean',
          description: 'Generate comprehensive validation reports',
          required: false
        },
        fixable: {
          type: 'boolean',
          description: 'Show suggested fixes for common issues',
          required: false
        },
        verbose: {
          type: 'boolean',
          description: 'Show detailed validation results',
          required: false
        }
      },
      examples: [
        {
          command: 'validate-system',
          description: 'Basic system validation'
        },
        {
          command: 'validate-system --generateTests --generateReport --verbose',
          description: 'Full validation with test generation and reports'
        },
        {
          command: 'validate-system --fixable',
          description: 'Validation with suggested fixes'
        }
      ]
    };
  }
}