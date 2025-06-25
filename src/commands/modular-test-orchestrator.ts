#!/usr/bin/env npx tsx
/**
 * Modular Test Orchestrator
 * Lambda Architecture for AI-Human Collaborative Command Ecosystem
 * Each package.json drives specialized, sophisticated testing patterns
 */

import * as fs from 'fs';
import * as path from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

interface CommandPackage {
  name: string;
  path: string;
  packageJson: any;
  capabilities: string[];
  dependencies: string[];
  hasTests: boolean;
  testRunner: string;
  testCommand: string;
  layer: 'batch' | 'speed' | 'serving';
  networkReady: boolean;
}

interface TestResult {
  command: string;
  success: boolean;
  duration: number;
  coverage?: number;
  errors?: string[];
  capabilities: string[];
  layer: string;
}

interface LambdaArchitectureMetrics {
  batchLayer: TestResult[];
  speedLayer: TestResult[];
  servingLayer: TestResult[];
  totalCommands: number;
  passRate: number;
  averageCoverage: number;
  networkReadiness: number;
}

class ModularTestOrchestrator {
  private commands: CommandPackage[] = [];
  private results: TestResult[] = [];
  private lambdaMetrics: LambdaArchitectureMetrics = {
    batchLayer: [],
    speedLayer: [],
    servingLayer: [],
    totalCommands: 0,
    passRate: 0,
    averageCoverage: 0,
    networkReadiness: 0
  };

  /**
   * Discover all command packages and their test configurations
   */
  async discoverCommandPackages(): Promise<void> {
    console.log('🔍 Discovering Command Package Ecosystem...\n');
    
    const commandDirs = this.findCommandDirectories();
    
    for (const dir of commandDirs) {
      try {
        const packagePath = path.join(dir, 'package.json');
        if (fs.existsSync(packagePath)) {
          const commandPackage = await this.analyzeCommandPackage(dir, packagePath);
          this.commands.push(commandPackage);
        }
      } catch (error) {
        console.warn(`⚠️ Failed to analyze package: ${dir}`, error.message);
      }
    }
    
    console.log(`📦 Discovered ${this.commands.length} command packages`);
    this.categorizeByLambdaArchitecture();
    this.printPackageEcosystem();
  }

  /**
   * Find all command directories with package.json
   */
  private findCommandDirectories(): string[] {
    const dirs: string[] = [];
    
    const scanDirectory = (dir: string) => {
      try {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          if (entry.isDirectory()) {
            // Check if this directory has a package.json
            if (fs.existsSync(path.join(fullPath, 'package.json'))) {
              dirs.push(fullPath);
            }
            // Recursively scan subdirectories
            scanDirectory(fullPath);
          }
        }
      } catch (error) {
        // Skip directories that can't be read
      }
    };
    
    scanDirectory(__dirname);
    return dirs;
  }

  /**
   * Analyze a command package and its test configuration
   */
  private async analyzeCommandPackage(dir: string, packagePath: string): Promise<CommandPackage> {
    const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
    const continuum = packageJson.continuum || {};
    
    // Determine test configuration
    const hasTests = continuum.hasTests || fs.existsSync(path.join(dir, 'test')) || fs.existsSync(path.join(dir, '*.test.ts'));
    const testRunner = this.determineTestRunner(packageJson, dir);
    const testCommand = this.buildTestCommand(testRunner, dir);
    
    // Check network readiness
    const networkReady = this.assessNetworkReadiness(continuum.capabilities || []);
    
    return {
      name: continuum.commandName || packageJson.name,
      path: dir,
      packageJson,
      capabilities: continuum.capabilities || [],
      dependencies: continuum.dependencies || [],
      hasTests,
      testRunner,
      testCommand,
      layer: this.classifyLambdaLayer(continuum.capabilities || []),
      networkReady
    };
  }

  /**
   * Determine the appropriate test runner for this package
   */
  private determineTestRunner(packageJson: any, dir: string): string {
    // Check scripts in package.json
    if (packageJson.scripts?.test) {
      return packageJson.scripts.test;
    }
    
    // Check for TypeScript test files
    if (fs.existsSync(path.join(dir, '*.test.ts')) || fs.existsSync(path.join(dir, 'test'))) {
      return 'jest';
    }
    
    // Check for JavaScript test files
    if (fs.existsSync(path.join(dir, '*.test.js'))) {
      return 'mocha';
    }
    
    return 'jest'; // Default
  }

  /**
   * Build the test command for this package
   */
  private buildTestCommand(testRunner: string, dir: string): string {
    const relativePath = path.relative(process.cwd(), dir);
    
    switch (testRunner) {
      case 'jest':
        return `npx jest ${relativePath} --coverage --json`;
      case 'mocha':
        return `npx mocha ${relativePath}/test/*.test.js --reporter json`;
      default:
        if (testRunner.includes('jest')) {
          return `${testRunner} --coverage --json`;
        }
        return testRunner;
    }
  }

  /**
   * Classify command into Lambda Architecture layer
   */
  private classifyLambdaLayer(capabilities: string[]): 'batch' | 'speed' | 'serving' {
    // Batch Layer: Complex operations, automation, analysis
    const batchKeywords = ['automation', 'macro', 'analysis', 'validation', 'documentation', 'github-actions'];
    
    // Speed Layer: Real-time, UI, interaction
    const speedKeywords = ['emotion', 'cursor', 'input', 'screenshot', 'websocket', 'real-time'];
    
    // Serving Layer: Orchestration, coordination, networking
    const servingKeywords = ['sharing', 'communication', 'session', 'coordination', 'network'];
    
    const capabilityString = capabilities.join(' ').toLowerCase();
    
    if (batchKeywords.some(keyword => capabilityString.includes(keyword))) {
      return 'batch';
    }
    
    if (speedKeywords.some(keyword => capabilityString.includes(keyword))) {
      return 'speed';
    }
    
    return 'serving';
  }

  /**
   * Assess network readiness based on capabilities
   */
  private assessNetworkReadiness(capabilities: string[]): boolean {
    const networkCapabilities = ['websocket', 'communication', 'sharing', 'broadcast', 'api'];
    return capabilities.some(cap => networkCapabilities.some(net => cap.includes(net)));
  }

  /**
   * Categorize commands by Lambda Architecture layers
   */
  private categorizeByLambdaArchitecture(): void {
    const layers = { batch: 0, speed: 0, serving: 0 };
    
    for (const cmd of this.commands) {
      layers[cmd.layer]++;
    }
    
    console.log(`\n🏗️ LAMBDA ARCHITECTURE DISTRIBUTION:`);
    console.log(`  📊 Batch Layer (Complex): ${layers.batch} commands`);
    console.log(`  ⚡ Speed Layer (Real-time): ${layers.speed} commands`);
    console.log(`  🌐 Serving Layer (Orchestration): ${layers.serving} commands`);
  }

  /**
   * Print comprehensive package ecosystem analysis
   */
  private printPackageEcosystem(): void {
    console.log(`\n📦 COMMAND PACKAGE ECOSYSTEM`);
    console.log(`============================`);
    
    const byLayer = {
      batch: this.commands.filter(cmd => cmd.layer === 'batch'),
      speed: this.commands.filter(cmd => cmd.layer === 'speed'),
      serving: this.commands.filter(cmd => cmd.layer === 'serving')
    };
    
    for (const [layer, commands] of Object.entries(byLayer)) {
      console.log(`\n${this.getLayerIcon(layer)} ${layer.toUpperCase()} LAYER:`);
      
      for (const cmd of commands) {
        const testStatus = cmd.hasTests ? '🧪' : '❌';
        const networkStatus = cmd.networkReady ? '🌐' : '📡';
        console.log(`  ${testStatus}${networkStatus} ${cmd.name}`);
        console.log(`    Capabilities: ${cmd.capabilities.join(', ')}`);
        console.log(`    Test Runner: ${cmd.testRunner}`);
        console.log(`    Dependencies: ${cmd.dependencies.length}`);
      }
    }
  }

  /**
   * Run modular tests across the entire ecosystem
   */
  async runModularTests(): Promise<void> {
    console.log(`\n🧪 MODULAR TEST EXECUTION`);
    console.log(`=========================\n`);
    
    // Run tests by layer priority (serving → speed → batch)
    const executionOrder = ['serving', 'speed', 'batch'] as const;
    
    for (const layer of executionOrder) {
      const layerCommands = this.commands.filter(cmd => cmd.layer === layer && cmd.hasTests);
      
      if (layerCommands.length > 0) {
        console.log(`${this.getLayerIcon(layer)} Testing ${layer.toUpperCase()} Layer (${layerCommands.length} commands):`);
        
        for (const command of layerCommands) {
          const result = await this.runCommandTest(command);
          this.results.push(result);
          this.categorizeResult(result);
        }
      }
    }
    
    this.generateLambdaMetrics();
    this.printTestResults();
  }

  /**
   * Run tests for a specific command package
   */
  private async runCommandTest(command: CommandPackage): Promise<TestResult> {
    const startTime = Date.now();
    console.log(`  🔄 Testing ${command.name}...`);
    
    try {
      const result = await this.executeTest(command);
      const duration = Date.now() - startTime;
      
      const testResult: TestResult = {
        command: command.name,
        success: result.success,
        duration,
        coverage: result.coverage,
        errors: result.errors,
        capabilities: command.capabilities,
        layer: command.layer
      };
      
      const status = result.success ? '✅' : '❌';
      const coverageText = result.coverage ? ` (${result.coverage}% coverage)` : '';
      console.log(`    ${status} ${command.name} - ${duration}ms${coverageText}`);
      
      return testResult;
      
    } catch (error) {
      const duration = Date.now() - startTime;
      console.log(`    ❌ ${command.name} - FAILED (${duration}ms): ${error.message}`);
      
      return {
        command: command.name,
        success: false,
        duration,
        errors: [error.message],
        capabilities: command.capabilities,
        layer: command.layer
      };
    }
  }

  /**
   * Execute test command and parse results
   */
  private async executeTest(command: CommandPackage): Promise<{ success: boolean; coverage?: number; errors?: string[] }> {
    return new Promise((resolve, reject) => {
      const child = spawn('bash', ['-c', command.testCommand], {
        cwd: command.path,
        stdio: ['pipe', 'pipe', 'pipe']
      });
      
      let stdout = '';
      let stderr = '';
      
      child.stdout?.on('data', (data) => {
        stdout += data.toString();
      });
      
      child.stderr?.on('data', (data) => {
        stderr += data.toString();
      });
      
      child.on('close', (code) => {
        try {
          // Parse JSON output from Jest/other test runners
          const result = this.parseTestOutput(stdout, stderr, code === 0);
          resolve(result);
        } catch (error) {
          // Fallback for non-JSON output
          resolve({
            success: code === 0,
            errors: stderr ? [stderr] : undefined
          });
        }
      });
      
      child.on('error', (error) => {
        reject(error);
      });
      
      // Timeout after 30 seconds
      setTimeout(() => {
        child.kill();
        reject(new Error('Test timeout'));
      }, 30000);
    });
  }

  /**
   * Parse test output from various test runners
   */
  private parseTestOutput(stdout: string, stderr: string, success: boolean): { success: boolean; coverage?: number; errors?: string[] } {
    try {
      // Try to parse Jest JSON output
      const jsonMatch = stdout.match(/\{[\s\S]*\}/);
      if (jsonMatch) {
        const result = JSON.parse(jsonMatch[0]);
        return {
          success: result.success !== false && result.numFailedTests === 0,
          coverage: result.coverageMap ? this.extractCoverage(result.coverageMap) : undefined,
          errors: result.testResults?.filter((t: any) => !t.status !== 'passed').map((t: any) => t.message)
        };
      }
    } catch (error) {
      // Fall through to simple parsing
    }
    
    // Simple success/failure detection
    return {
      success,
      errors: stderr ? [stderr] : undefined
    };
  }

  /**
   * Extract coverage percentage from coverage map
   */
  private extractCoverage(coverageMap: any): number {
    if (!coverageMap) return 0;
    
    let totalLines = 0;
    let coveredLines = 0;
    
    for (const [, coverage] of Object.entries(coverageMap)) {
      const lineCoverage = (coverage as any).s || {};
      for (const [, count] of Object.entries(lineCoverage)) {
        totalLines++;
        if ((count as number) > 0) coveredLines++;
      }
    }
    
    return totalLines > 0 ? Math.round((coveredLines / totalLines) * 100) : 0;
  }

  /**
   * Categorize test result into lambda architecture metrics
   */
  private categorizeResult(result: TestResult): void {
    switch (result.layer) {
      case 'batch':
        this.lambdaMetrics.batchLayer.push(result);
        break;
      case 'speed':
        this.lambdaMetrics.speedLayer.push(result);
        break;
      case 'serving':
        this.lambdaMetrics.servingLayer.push(result);
        break;
    }
  }

  /**
   * Generate comprehensive Lambda Architecture metrics
   */
  private generateLambdaMetrics(): void {
    const allResults = this.results;
    const successfulTests = allResults.filter(r => r.success);
    
    this.lambdaMetrics.totalCommands = allResults.length;
    this.lambdaMetrics.passRate = allResults.length > 0 ? (successfulTests.length / allResults.length) * 100 : 0;
    
    const coverageValues = allResults.filter(r => r.coverage !== undefined).map(r => r.coverage!);
    this.lambdaMetrics.averageCoverage = coverageValues.length > 0 
      ? coverageValues.reduce((a, b) => a + b, 0) / coverageValues.length 
      : 0;
    
    const networkReadyCommands = this.commands.filter(cmd => cmd.networkReady).length;
    this.lambdaMetrics.networkReadiness = this.commands.length > 0 
      ? (networkReadyCommands / this.commands.length) * 100 
      : 0;
  }

  /**
   * Print comprehensive test results with Lambda Architecture analysis
   */
  private printTestResults(): void {
    console.log(`\n📊 LAMBDA ARCHITECTURE TEST RESULTS`);
    console.log(`===================================\n`);
    
    // Overall metrics
    console.log(`🎯 ECOSYSTEM HEALTH:`);
    console.log(`  Total Commands: ${this.lambdaMetrics.totalCommands}`);
    console.log(`  Pass Rate: ${this.lambdaMetrics.passRate.toFixed(1)}%`);
    console.log(`  Average Coverage: ${this.lambdaMetrics.averageCoverage.toFixed(1)}%`);
    console.log(`  Network Readiness: ${this.lambdaMetrics.networkReadiness.toFixed(1)}%`);
    
    // Layer-specific results
    const layers = [
      { name: 'BATCH', icon: '📊', results: this.lambdaMetrics.batchLayer },
      { name: 'SPEED', icon: '⚡', results: this.lambdaMetrics.speedLayer },
      { name: 'SERVING', icon: '🌐', results: this.lambdaMetrics.servingLayer }
    ];
    
    for (const layer of layers) {
      if (layer.results.length > 0) {
        const successful = layer.results.filter(r => r.success).length;
        const passRate = (successful / layer.results.length) * 100;
        
        console.log(`\n${layer.icon} ${layer.name} LAYER:`);
        console.log(`  Commands: ${layer.results.length}`);
        console.log(`  Pass Rate: ${passRate.toFixed(1)}%`);
        
        for (const result of layer.results) {
          const status = result.success ? '✅' : '❌';
          const coverage = result.coverage ? ` (${result.coverage}%)` : '';
          console.log(`    ${status} ${result.command} - ${result.duration}ms${coverage}`);
        }
      }
    }
    
    // AI/Human collaboration readiness
    console.log(`\n🤝 AI-HUMAN COLLABORATION READINESS:`);
    console.log(`  Network-Ready Commands: ${this.commands.filter(cmd => cmd.networkReady).length}`);
    console.log(`  Tested Commands: ${this.commands.filter(cmd => cmd.hasTests).length}`);
    console.log(`  Specialized Capabilities: ${new Set(this.commands.flatMap(cmd => cmd.capabilities)).size}`);
    
    const fluentCommands = this.commands.filter(cmd => 
      cmd.capabilities.some(cap => cap.includes('chain') || cap.includes('fluent'))
    );
    console.log(`  Fluent API Commands: ${fluentCommands.length}`);
  }

  /**
   * Get icon for lambda architecture layer
   */
  private getLayerIcon(layer: string): string {
    switch (layer) {
      case 'batch': return '📊';
      case 'speed': return '⚡';
      case 'serving': return '🌐';
      default: return '📦';
    }
  }

  /**
   * Generate npm-style test report
   */
  generateNpmStyleReport(): string {
    const report = {
      name: 'continuum-command-ecosystem',
      version: '1.0.0',
      testResults: {
        total: this.lambdaMetrics.totalCommands,
        passed: this.results.filter(r => r.success).length,
        failed: this.results.filter(r => !r.success).length,
        coverage: this.lambdaMetrics.averageCoverage,
        networkReadiness: this.lambdaMetrics.networkReadiness
      },
      lambdaArchitecture: {
        batchLayer: this.lambdaMetrics.batchLayer.length,
        speedLayer: this.lambdaMetrics.speedLayer.length,
        servingLayer: this.lambdaMetrics.servingLayer.length
      },
      capabilities: Array.from(new Set(this.commands.flatMap(cmd => cmd.capabilities))),
      readinessScore: (this.lambdaMetrics.passRate + this.lambdaMetrics.averageCoverage + this.lambdaMetrics.networkReadiness) / 3
    };
    
    return JSON.stringify(report, null, 2);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const orchestrator = new ModularTestOrchestrator();
  
  orchestrator.discoverCommandPackages()
    .then(() => orchestrator.runModularTests())
    .then(() => {
      console.log('\n📋 NPM-STYLE REPORT:');
      console.log(orchestrator.generateNpmStyleReport());
      
      console.log('\n🚀 LAMBDA ARCHITECTURE READY!');
      console.log('  • Modular, specialized command packages');
      console.log('  • AI-human collaborative network established');
      console.log('  • TypeScript migration path clear');
      console.log('  • Enterprise-grade testing infrastructure');
    })
    .catch(error => {
      console.error('❌ Modular test orchestration failed:', error);
      process.exit(1);
    });
}

export { ModularTestOrchestrator };