/**
 * Self-Validating Module Framework
 * 
 * Each module's configuration becomes its test contract.
 * Tests are auto-generated from the module's own continuum config.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { 
  ContinuumConfig, 
  ContinuumCommandConfig, 
  ContinuumDaemonConfig, 
  ContinuumWidgetConfig,
  ContinuumPackageUtils,
  PackageJson 
} from '../../types/ContinuumPackage.js';

export interface SelfValidationResult {
  modulePath: string;
  moduleId: string;
  configType: string;
  testsGenerated: number;
  testsPassed: number;
  testsFailed: number;
  isCompliant: boolean;
  validationErrors: ConfigValidationError[];
  capabilityTests: CapabilityTestResult[];
  structureTests: StructureTestResult[];
}

export interface ConfigValidationError {
  type: 'missing-file' | 'missing-method' | 'invalid-structure' | 'capability-mismatch';
  severity: 'error' | 'warning';
  message: string;
  expectedBy: string; // Which config property requires this
  actualPath?: string;
}

export interface CapabilityTestResult {
  capability: string;
  required: boolean;
  implemented: boolean;
  evidence: string[];
  errors: string[];
}

export interface StructureTestResult {
  requirement: string;
  met: boolean;
  details: string;
}

export class SelfValidatingModule {
  
  /**
   * Validate a module against its own configuration contract
   */
  static async validateSelf(modulePath: string): Promise<SelfValidationResult> {
    const packageJsonPath = path.join(modulePath, 'package.json');
    
    try {
      const packageData = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8')) as PackageJson;
      const config = packageData.continuum;
      
      if (!config) {
        throw new Error('No continuum configuration found');
      }
      
      const moduleId = ContinuumPackageUtils.getModuleId(config);
      const configType = ContinuumPackageUtils.getModuleType(config);
      
      // Generate and run tests based on the module's own config
      const validationErrors: ConfigValidationError[] = [];
      const capabilityTests = await this.validateCapabilities(modulePath, config);
      const structureTests = await this.validateStructure(modulePath, config);
      
      // Count test results
      const testsGenerated = capabilityTests.length + structureTests.length;
      const testsPassed = capabilityTests.filter(t => t.implemented).length + 
                         structureTests.filter(t => t.met).length;
      const testsFailed = testsGenerated - testsPassed;
      
      // Determine compliance
      const hasErrors = validationErrors.some(e => e.severity === 'error') || 
                       capabilityTests.some(t => t.required && !t.implemented) ||
                       structureTests.some(t => !t.met);
      
      return {
        modulePath,
        moduleId,
        configType,
        testsGenerated,
        testsPassed,
        testsFailed,
        isCompliant: !hasErrors,
        validationErrors,
        capabilityTests,
        structureTests
      };
      
    } catch (error) {
      return {
        modulePath,
        moduleId: 'unknown',
        configType: 'unknown',
        testsGenerated: 0,
        testsPassed: 0,
        testsFailed: 1,
        isCompliant: false,
        validationErrors: [{
          type: 'invalid-structure',
          severity: 'error',
          message: `Failed to load module: ${error instanceof Error ? error.message : String(error)}`,
          expectedBy: 'package.json'
        }],
        capabilityTests: [],
        structureTests: []
      };
    }
  }
  
  /**
   * Validate that the module implements its declared capabilities
   */
  private static async validateCapabilities(
    modulePath: string, 
    config: ContinuumConfig
  ): Promise<CapabilityTestResult[]> {
    const results: CapabilityTestResult[] = [];
    
    for (const capability of config.capabilities) {
      const result = await this.testCapability(modulePath, config, capability as string);
      results.push(result);
    }
    
    return results;
  }
  
  /**
   * Test if a specific capability is actually implemented
   */
  private static async testCapability(
    modulePath: string, 
    config: ContinuumConfig, 
    capability: string
  ): Promise<CapabilityTestResult> {
    const evidence: string[] = [];
    const errors: string[] = [];
    let implemented = false;
    
    try {
      // Get the main implementation file
      const mainFile = await this.getMainImplementationFile(modulePath, config);
      const content = await fs.readFile(mainFile, 'utf-8');
      
      // Test capability implementation based on capability type
      switch (capability) {
        case 'file-reading':
          implemented = content.includes('readFile') || content.includes('fs.read');
          if (implemented) evidence.push('Found file reading operations');
          else errors.push('Declares file-reading but no file read operations found');
          break;
          
        case 'file-writing':
          implemented = content.includes('writeFile') || content.includes('fs.write');
          if (implemented) evidence.push('Found file writing operations');
          else errors.push('Declares file-writing but no file write operations found');
          break;
          
        case 'session-management':
          implemented = content.includes('session') && (content.includes('create') || content.includes('manage'));
          if (implemented) evidence.push('Found session management code');
          else errors.push('Declares session-management but no session code found');
          break;
          
        case 'daemon-management':
          implemented = content.includes('daemon') && (content.includes('start') || content.includes('stop'));
          if (implemented) evidence.push('Found daemon management code');
          else errors.push('Declares daemon-management but no daemon control found');
          break;
          
        case 'browser-orchestration':
          implemented = content.includes('browser') || content.includes('chrome') || content.includes('puppeteer');
          if (implemented) evidence.push('Found browser control code');
          else errors.push('Declares browser-orchestration but no browser code found');
          break;
          
        case 'websocket-management':
          implemented = content.includes('WebSocket') || content.includes('ws');
          if (implemented) evidence.push('Found WebSocket code');
          else errors.push('Declares websocket-management but no WebSocket code found');
          break;
          
        default:
          // Generic capability check - look for the capability name in code
          implemented = content.toLowerCase().includes(capability.toLowerCase().replace('-', ''));
          if (implemented) evidence.push(`Found references to ${capability}`);
          else errors.push(`Declares ${capability} but no related code found`);
      }
      
    } catch (error) {
      errors.push(`Could not test capability: ${error instanceof Error ? error.message : String(error)}`);
    }
    
    return {
      capability,
      required: true, // All declared capabilities are required
      implemented,
      evidence,
      errors
    };
  }
  
  /**
   * Validate the module's structure based on its type
   */
  private static async validateStructure(
    modulePath: string, 
    config: ContinuumConfig
  ): Promise<StructureTestResult[]> {
    const results: StructureTestResult[] = [];
    const moduleType = ContinuumPackageUtils.getModuleType(config);
    
    // Base structure requirements for all modules
    results.push(await this.testFileExists(modulePath, 'package.json', 'Package definition'));
    results.push(await this.testFileExists(modulePath, 'README.md', 'Documentation'));
    results.push(await this.testDirectoryExists(modulePath, 'test', 'Test directory'));
    results.push(await this.testDirectoryExists(modulePath, 'test/unit', 'Unit tests'));
    results.push(await this.testDirectoryExists(modulePath, 'test/integration', 'Integration tests'));
    
    // Type-specific structure requirements
    switch (moduleType) {
      case 'command':
        const commandConfig = config as ContinuumCommandConfig;
        const commandFile = `${commandConfig.command.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('')}Command.ts`;
        results.push(await this.testFileExists(modulePath, commandFile, 'Command implementation'));
        results.push(await this.testMethodExists(modulePath, commandFile, 'static async execute', 'Execute method'));
        results.push(await this.testMethodExists(modulePath, commandFile, 'static getDefinition', 'Definition method'));
        break;
        
      case 'daemon':
        const daemonConfig = config as ContinuumDaemonConfig;
        const daemonFile = `${daemonConfig.daemon.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('')}Daemon.ts`;
        results.push(await this.testFileExists(modulePath, daemonFile, 'Daemon implementation'));
        results.push(await this.testMethodExists(modulePath, daemonFile, 'async onStart', 'Start method'));
        results.push(await this.testMethodExists(modulePath, daemonFile, 'async onStop', 'Stop method'));
        break;
        
      case 'widget':
        const widgetConfig = config as ContinuumWidgetConfig;
        const widgetFile = `${widgetConfig.widget.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('')}Widget.ts`;
        results.push(await this.testFileExists(modulePath, widgetFile, 'Widget implementation'));
        
        // Test UI assets if declared
        if (widgetConfig.ui?.template) {
          results.push(await this.testFileExists(modulePath, widgetConfig.ui.template, 'Widget template'));
        }
        if (widgetConfig.ui?.styles) {
          for (const styleFile of widgetConfig.ui.styles) {
            results.push(await this.testFileExists(modulePath, styleFile, `Style file: ${styleFile}`));
          }
        }
        break;
    }
    
    return results;
  }
  
  /**
   * Test if a file exists
   */
  private static async testFileExists(
    modulePath: string, 
    fileName: string, 
    description: string
  ): Promise<StructureTestResult> {
    try {
      await fs.access(path.join(modulePath, fileName));
      return {
        requirement: `${description} (${fileName})`,
        met: true,
        details: `File exists: ${fileName}`
      };
    } catch {
      return {
        requirement: `${description} (${fileName})`,
        met: false,
        details: `Missing file: ${fileName}`
      };
    }
  }
  
  /**
   * Test if a directory exists
   */
  private static async testDirectoryExists(
    modulePath: string, 
    dirName: string, 
    description: string
  ): Promise<StructureTestResult> {
    try {
      const stat = await fs.stat(path.join(modulePath, dirName));
      const isDirectory = stat.isDirectory();
      return {
        requirement: `${description} (${dirName}/)`,
        met: isDirectory,
        details: isDirectory ? `Directory exists: ${dirName}` : `Path exists but is not a directory: ${dirName}`
      };
    } catch {
      return {
        requirement: `${description} (${dirName}/)`,
        met: false,
        details: `Missing directory: ${dirName}`
      };
    }
  }
  
  /**
   * Test if a method exists in a file
   */
  private static async testMethodExists(
    modulePath: string, 
    fileName: string, 
    methodSignature: string, 
    description: string
  ): Promise<StructureTestResult> {
    try {
      const content = await fs.readFile(path.join(modulePath, fileName), 'utf-8');
      const hasMethod = content.includes(methodSignature);
      return {
        requirement: `${description} (${methodSignature})`,
        met: hasMethod,
        details: hasMethod ? `Method found: ${methodSignature}` : `Missing method: ${methodSignature}`
      };
    } catch {
      return {
        requirement: `${description} (${methodSignature})`,
        met: false,
        details: `Cannot check method - file missing: ${fileName}`
      };
    }
  }
  
  /**
   * Get the main implementation file for a module
   */
  private static async getMainImplementationFile(modulePath: string, config: ContinuumConfig): Promise<string> {
    const moduleType = ContinuumPackageUtils.getModuleType(config);
    const moduleId = ContinuumPackageUtils.getModuleId(config);
    
    // Convert module ID to PascalCase filename
    const pascalName = moduleId.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join('');
    
    const possibleFiles = [
      `${pascalName}${moduleType.charAt(0).toUpperCase() + moduleType.slice(1)}.ts`,
      `${pascalName}.ts`,
      path.basename(modulePath) + '.ts',
      'index.ts'
    ];
    
    for (const fileName of possibleFiles) {
      try {
        await fs.access(path.join(modulePath, fileName));
        return path.join(modulePath, fileName);
      } catch {
        // Try next file
      }
    }
    
    throw new Error(`No main implementation file found. Tried: ${possibleFiles.join(', ')}`);
  }
  
  /**
   * Generate a test file that validates this module
   */
  static async generateSelfTest(modulePath: string): Promise<string> {
    const validation = await this.validateSelf(modulePath);
    
    return `
/**
 * Auto-generated self-validation test for ${validation.moduleId}
 * 
 * This test validates that the module implements what it declares
 * in its continuum configuration.
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { SelfValidatingModule } from '../../types/testing/SelfValidatingModule.js';

test('${validation.moduleId} self-validation', async () => {
  const result = await SelfValidatingModule.validateSelf('${modulePath}');
  
  // Module should be compliant with its own configuration
  assert.equal(result.isCompliant, true, 
    \`Module is not compliant: \${result.validationErrors.map(e => e.message).join(', ')}\`);
  
  // All declared capabilities should be implemented
  for (const capTest of result.capabilityTests) {
    assert.equal(capTest.implemented, true, 
      \`Capability '\${capTest.capability}' not implemented: \${capTest.errors.join(', ')}\`);
  }
  
  // Required structure should exist
  for (const structTest of result.structureTests) {
    assert.equal(structTest.met, true, 
      \`Structure requirement not met: \${structTest.requirement} - \${structTest.details}\`);
  }
  
  console.log(\`✅ \${result.moduleId}: \${result.testsPassed}/\${result.testsGenerated} tests passed\`);
});
`;
  }
  
  /**
   * Generate self-validation tests for all modules
   */
  static async generateAllSelfTests(srcPath: string): Promise<void> {
    const modules = await this.discoverModules(srcPath);
    
    for (const modulePath of modules) {
      try {
        const testContent = await this.generateSelfTest(modulePath);
        const testFile = path.join(modulePath, 'test', 'self-validation.test.ts');
        
        // Ensure test directory exists
        await fs.mkdir(path.join(modulePath, 'test'), { recursive: true });
        
        await fs.writeFile(testFile, testContent);
        console.log(`Generated self-test: ${testFile}`);
        
      } catch (error) {
        console.error(`Failed to generate test for ${modulePath}:`, error);
      }
    }
  }
  
  /**
   * Discover modules (same as ModuleComplianceFramework)
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
}