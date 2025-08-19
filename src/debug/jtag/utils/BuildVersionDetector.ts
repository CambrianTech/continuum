#!/usr/bin/env tsx
/**
 * Build Version Detection System
 * 
 * Ensures the running JTAG system matches the current source code state.
 * Detects when rebuilds are needed due to TypeScript/TypeScript changes.
 * Integrates with auto-spawn pattern to provide seamless development.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { globSync } from 'glob';
import { execSync } from 'child_process';
import { SystemReadySignaler } from '../scripts/signal-system-ready';
import { diagnostics } from './DiagnosticsLogger';

export interface VersionMismatchResult {
  needsRebuild: boolean;
  reason: string;
  details: {
    sourceHash: string;
    runningHash: string;
    newestSource: number;
    runningSystem: number;
    buildStatus: 'current' | 'outdated' | 'missing' | 'unknown';
  };
}

export interface BuildNeedAnalysis {
  typescript: boolean;
  generated: boolean;
  system: boolean;
  reason: string[];
  severity: 'none' | 'minor' | 'major' | 'critical';
}

export class BuildVersionDetector {
  private versionFile = '.continuum/jtag/system/version.json';
  private sourceHashFile = '.continuum/jtag/system/source-hash.json';

  /**
   * Main entry point: Detect if running system matches current source code
   */
  async detectVersionMismatch(): Promise<VersionMismatchResult> {
    const operationId = `version-detect-${Date.now()}`;
    const context = diagnostics.startOperation(operationId, 'Version Mismatch Detection', 30000);
    
    try {
      console.log('🔍 BUILD VERSION DETECTION: Analyzing source vs running system...');
      
      // Get current source code state with error handling
      diagnostics.addDetail(operationId, 'phase', 'source_analysis');
      const currentSourceHash = await this.safeCalculateSourceHash(operationId);
      const currentSourceTime = this.safeGetNewestSourceTime(operationId);
      
      diagnostics.addDetail(operationId, 'sourceHash', currentSourceHash.substring(0, 12));
      diagnostics.addDetail(operationId, 'sourceTime', new Date(currentSourceTime).toISOString());
      
      // Get running system state with error handling
      diagnostics.addDetail(operationId, 'phase', 'system_analysis');
      const runningSystemHash = await this.safeGetRunningSystemHash(operationId);
      const runningSystemTime = await this.safeGetRunningSystemTime(operationId);
      
      diagnostics.addDetail(operationId, 'runningHash', runningSystemHash?.substring(0, 12) || 'none');
      diagnostics.addDetail(operationId, 'runningTime', new Date(runningSystemTime).toISOString());
      
      console.log(`📊 Source hash: ${currentSourceHash.substring(0, 8)}... (modified: ${new Date(currentSourceTime).toISOString()})`);
      console.log(`📊 System hash: ${runningSystemHash ? runningSystemHash.substring(0, 8) + '...' : 'not found'} (running: ${new Date(runningSystemTime).toISOString()})`);
      
      // Determine if rebuild is needed with error tracking
      diagnostics.addDetail(operationId, 'phase', 'rebuild_analysis');
      const needsRebuild = await this.safeAnalyzeRebuildNeed(operationId, currentSourceHash, runningSystemHash, currentSourceTime, runningSystemTime);
      
      let buildStatus: 'current' | 'outdated' | 'missing' | 'unknown' = 'unknown';
      let reason = '';
      
      if (!runningSystemHash) {
        buildStatus = 'missing';
        reason = 'No running system detected - first build needed';
        diagnostics.addDetail(operationId, 'decision', 'missing_system');
      } else if (currentSourceHash !== runningSystemHash) {
        buildStatus = 'outdated';
        reason = 'Source code changed since last build';
        diagnostics.addDetail(operationId, 'decision', 'hash_mismatch');
      } else if (currentSourceTime > runningSystemTime + 5000) { // 5 second tolerance
        buildStatus = 'outdated';
        reason = 'Source files modified after system startup';
        diagnostics.addDetail(operationId, 'decision', 'time_mismatch');
      } else {
        buildStatus = 'current';
        reason = 'System is up to date with current source code';
        diagnostics.addDetail(operationId, 'decision', 'current');
      }
      
      diagnostics.addDetail(operationId, 'needsRebuild', needsRebuild);
      diagnostics.addDetail(operationId, 'buildStatus', buildStatus);
      
      if (!needsRebuild) {
        console.log('✅ BUILD VERSION: System matches current source code');
      } else {
        console.log(`🔄 BUILD VERSION: Rebuild needed - ${reason}`);
      }
      
      const result = {
        needsRebuild,
        reason,
        details: {
          sourceHash: currentSourceHash,
          runningHash: runningSystemHash || '',
          newestSource: currentSourceTime,
          runningSystem: runningSystemTime,
          buildStatus
        }
      };
      
      diagnostics.completeOperation(operationId);
      return result;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : undefined;
      
      diagnostics.addError(operationId, `Version detection failed: ${errorMessage}`);
      if (errorStack) {
        diagnostics.addDetail(operationId, 'errorStack', errorStack);
      }
      
      diagnostics.failOperation(operationId, errorMessage);
      
      console.error('❌ BUILD VERSION DETECTION ERROR:', errorMessage);
      console.error('🔍 Check diagnostic logs for detailed error information');
      
      return {
        needsRebuild: true,
        reason: `Version detection failed: ${errorMessage}`,
        details: {
          sourceHash: '',
          runningHash: '',
          newestSource: 0,
          runningSystem: 0,
          buildStatus: 'unknown'
        }
      };
    }
  }

  /**
   * Calculate hash of all relevant source files
   */
  async calculateSourceHash(): Promise<string> {
    const sourcePatterns = [
      '**/*.ts',
      '**/*.tsx', 
      'tsconfig.json',
      'package.json',
      '!node_modules/**/*',
      '!dist/**/*',
      '!examples/test-bench/node_modules/**/*',
      '!.continuum/**/*'
    ];
    
    const files: string[] = [];
    
    // Get all source files with timestamps
    for (const pattern of sourcePatterns) {
      const matched = globSync(pattern);
      files.push(...matched);
    }
    
    // Sort for consistent hashing
    files.sort();
    
    // Create hash from file contents and modification times
    const hasher = crypto.createHash('sha256');
    
    for (const file of files) {
      try {
        const stats = fs.statSync(file);
        const content = fs.readFileSync(file, 'utf8');
        
        // Include both content and modification time in hash
        hasher.update(`${file}:${stats.mtime.getTime()}:${content}`);
      } catch (error) {
        // File might have been deleted, skip
        continue;
      }
    }
    
    return hasher.digest('hex');
  }

  /**
   * Get timestamp of newest source file
   */
  private getNewestSourceTime(): number {
    try {
      const sourceFiles = globSync('**/*.{ts,tsx}', { 
        ignore: ['node_modules/**/*', 'dist/**/*', '.continuum/**/*'] 
      });
      
      if (sourceFiles.length === 0) return 0;
      
      return Math.max(...sourceFiles.map(file => {
        try {
          return fs.statSync(file).mtime.getTime();
        } catch {
          return 0;
        }
      }));
    } catch {
      return 0;
    }
  }

  /**
   * Get hash of currently running system (from version file)
   */
  private async getRunningSystemHash(): Promise<string | null> {
    try {
      // First check if system is actually running
      const signaler = new SystemReadySignaler();
      const signal = await signaler.generateReadySignal();
      
      if (!signal.bootstrapComplete || signal.systemHealth === 'error') {
        return null; // No running system
      }
      
      // Try to read stored source hash from running system
      if (fs.existsSync(this.sourceHashFile)) {
        const hashData = JSON.parse(fs.readFileSync(this.sourceHashFile, 'utf8'));
        return hashData.sourceHash || null;
      }
      
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Get startup time of currently running system
   */
  private async getRunningSystemTime(): Promise<number> {
    try {
      // Check system startup from signal file
      const signalFile = '.continuum/jtag/signals/system-ready.json';
      if (fs.existsSync(signalFile)) {
        const stats = fs.statSync(signalFile);
        return stats.mtime.getTime();
      }
      
      // Fallback: check log files
      const logFile = '.continuum/jtag/system/logs/npm-start.log';
      if (fs.existsSync(logFile)) {
        const stats = fs.statSync(logFile);
        return stats.mtime.getTime();
      }
      
      return 0;
    } catch {
      return 0;
    }
  }

  /**
   * Analyze specific build needs beyond version hash
   */
  async analyzeBuildNeeds(): Promise<BuildNeedAnalysis> {
    const needs = {
      typescript: false,
      generated: false,
      system: false,
      reason: [] as string[],
      severity: 'none' as 'none' | 'minor' | 'major' | 'critical'
    };

    // Check TypeScript compilation
    const tsFiles = this.getNewestFileTime('**/*.ts');
    const jsFiles = this.getNewestFileTime('dist/**/*.js');
    const tsConfig = this.getFileModTime('tsconfig.json');
    
    if (jsFiles === 0) {
      needs.typescript = true;
      needs.reason.push('No compiled JavaScript files found');
      needs.severity = 'critical';
    } else if (tsFiles > jsFiles || tsConfig > jsFiles) {
      needs.typescript = true;
      needs.reason.push('TypeScript source files newer than compiled output');
      needs.severity = 'major';
    }

    // Check generated files
    const sourceFiles = this.getNewestFileTime('{daemons,commands,system}/**/*.ts');
    const generatedBrowser = this.getFileModTime('browser/generated.ts');
    const generatedServer = this.getFileModTime('server/generated.ts');
    
    if (generatedBrowser === 0 || generatedServer === 0) {
      needs.generated = true;
      needs.reason.push('Generated files missing');
      needs.severity = 'critical';
    } else if (sourceFiles > Math.min(generatedBrowser, generatedServer)) {
      needs.generated = true;
      needs.reason.push('Source files newer than generated files');
      if (needs.severity === 'none') needs.severity = 'major';
    }

    // Check system deployment
    const examplesPackage = this.getFileModTime('examples/test-bench/package.json');
    const mainPackage = this.getFileModTime('package.json');
    
    if (mainPackage > examplesPackage && examplesPackage > 0) {
      needs.system = true;
      needs.reason.push('Main package.json newer than test-bench deployment');
      if (needs.severity === 'none') needs.severity = 'minor';
    }

    return needs;
  }

  /**
   * Store source hash for running system (called after successful build)
   */
  async storeSystemVersion(sourceHash: string): Promise<void> {
    try {
      const versionData = {
        sourceHash,
        buildTime: new Date().toISOString(),
        buildTimestamp: Date.now(),
        packageVersion: this.getPackageVersion()
      };
      
      // Ensure directory exists
      fs.mkdirSync(path.dirname(this.sourceHashFile), { recursive: true });
      
      // Store hash file
      fs.writeFileSync(this.sourceHashFile, JSON.stringify(versionData, null, 2));
      
      console.log(`✅ BUILD VERSION: Stored system version ${sourceHash.substring(0, 8)}...`);
    } catch (error) {
      console.warn('⚠️ Failed to store system version:', error.message);
    }
  }

  /**
   * Integration point: Should we rebuild before running tests?
   */
  async shouldRebuildForTesting(): Promise<{ rebuild: boolean; reason: string }> {
    const versionCheck = await this.detectVersionMismatch();
    const buildCheck = await this.analyzeBuildNeeds();
    
    // Critical build issues always require rebuild
    if (buildCheck.severity === 'critical') {
      return {
        rebuild: true,
        reason: `Critical build issue: ${buildCheck.reason.join(', ')}`
      };
    }
    
    // Version mismatch requires rebuild for reliable testing
    if (versionCheck.needsRebuild) {
      return {
        rebuild: true,
        reason: versionCheck.reason
      };
    }
    
    return {
      rebuild: false,
      reason: 'System is current with source code'
    };
  }

  // Helper methods
  private getFileModTime(filePath: string): number {
    try {
      return fs.statSync(filePath).mtime.getTime();
    } catch {
      return 0;
    }
  }

  private getNewestFileTime(pattern: string): number {
    try {
      const files = globSync(pattern);
      if (files.length === 0) return 0;
      return Math.max(...files.map(file => this.getFileModTime(file)));
    } catch {
      return 0;
    }
  }

  private getPackageVersion(): string {
    try {
      const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
      return pkg.version || 'unknown';
    } catch {
      return 'unknown';
    }
  }

  // Safe wrapper methods with timeout protection and error handling

  private async safeCalculateSourceHash(operationId: string): Promise<string> {
    try {
      return await Promise.race([
        this.calculateSourceHash(),
        new Promise<string>((_, reject) => {
          setTimeout(() => reject(new Error('Source hash calculation timeout')), 15000);
        })
      ]);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      diagnostics.addError(operationId, `Source hash calculation failed: ${errorMessage}`);
      throw new Error(`Unable to calculate source hash: ${errorMessage}`);
    }
  }

  private safeGetNewestSourceTime(operationId: string): number {
    try {
      const time = this.getNewestSourceTime();
      if (time === 0) {
        diagnostics.addWarning(operationId, 'No source files found for timestamp calculation');
      }
      return time;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      diagnostics.addError(operationId, `Source time calculation failed: ${errorMessage}`);
      return Date.now(); // Fallback to current time
    }
  }

  private async safeGetRunningSystemHash(operationId: string): Promise<string | null> {
    try {
      return await Promise.race([
        this.getRunningSystemHash(),
        new Promise<string | null>((_, reject) => {
          setTimeout(() => reject(new Error('Running system hash timeout')), 10000);
        })
      ]);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      diagnostics.addWarning(operationId, `Running system hash check failed: ${errorMessage}`);
      return null; // This is expected if no system is running
    }
  }

  private async safeGetRunningSystemTime(operationId: string): Promise<number> {
    try {
      return await Promise.race([
        this.getRunningSystemTime(),
        new Promise<number>((_, reject) => {
          setTimeout(() => reject(new Error('Running system time timeout')), 5000);
        })
      ]);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      diagnostics.addWarning(operationId, `Running system time check failed: ${errorMessage}`);
      return 0; // This indicates no running system detected
    }
  }

  private async safeAnalyzeRebuildNeed(
    operationId: string,
    sourceHash: string, 
    runningHash: string | null,
    sourceTime: number,
    runningTime: number
  ): Promise<boolean> {
    try {
      return await Promise.race([
        this.analyzeRebuildNeed(sourceHash, runningHash, sourceTime, runningTime),
        new Promise<boolean>((_, reject) => {
          setTimeout(() => reject(new Error('Rebuild analysis timeout')), 10000);
        })
      ]);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      diagnostics.addError(operationId, `Rebuild analysis failed: ${errorMessage}`);
      // Default to rebuild needed when analysis fails
      return true;
    }
  }

  private async analyzeRebuildNeed(
    sourceHash: string, 
    runningHash: string | null,
    sourceTime: number,
    runningTime: number
  ): Promise<boolean> {
    // No running system hash = definitely need build
    if (!runningHash) {
      return true;
    }
    
    // Source hash changed = need rebuild
    if (sourceHash !== runningHash) {
      return true;
    }
    
    // Source files modified after system start = need rebuild
    if (sourceTime > runningTime + 5000) { // 5 second tolerance for filesystem timing
      return true;
    }
    
    // Check for other build indicators
    const buildAnalysis = await this.analyzeBuildNeeds();
    if (buildAnalysis.severity === 'critical' || buildAnalysis.severity === 'major') {
      return true;
    }
    
    return false;
  }
}

// CLI interface for testing
async function main() {
  const detector = new BuildVersionDetector();
  
  if (process.argv.includes('--check-version')) {
    const result = await detector.detectVersionMismatch();
    console.log('\n🎯 VERSION MISMATCH ANALYSIS:');
    console.log(`   Needs rebuild: ${result.needsRebuild ? '✅ YES' : '❌ NO'}`);
    console.log(`   Reason: ${result.reason}`);
    console.log(`   Source hash: ${result.details.sourceHash.substring(0, 12)}...`);
    console.log(`   System hash: ${result.details.runningHash ? result.details.runningHash.substring(0, 12) + '...' : 'not found'}`);
    console.log(`   Build status: ${result.details.buildStatus}`);
    process.exit(result.needsRebuild ? 1 : 0);
  }
  
  if (process.argv.includes('--check-build')) {
    const result = await detector.shouldRebuildForTesting();
    console.log('\n🎯 BUILD NEED ANALYSIS:');
    console.log(`   Should rebuild: ${result.rebuild ? '✅ YES' : '❌ NO'}`);
    console.log(`   Reason: ${result.reason}`);
    process.exit(result.rebuild ? 1 : 0);
  }
  
  if (process.argv.includes('--store-version')) {
    const sourceHash = await detector['calculateSourceHash']();
    await detector.storeSystemVersion(sourceHash);
    process.exit(0);
  }
  
  // Default: show both analyses
  const versionResult = await detector.detectVersionMismatch();
  const buildResult = await detector.shouldRebuildForTesting();
  
  console.log('\n🎯 COMPREHENSIVE BUILD ANALYSIS:');
  console.log(`   Version mismatch: ${versionResult.needsRebuild ? '⚠️ YES' : '✅ NO'}`);
  console.log(`   Should rebuild for testing: ${buildResult.rebuild ? '⚠️ YES' : '✅ NO'}`);
  console.log(`   Overall reason: ${buildResult.reason}`);
}

if (require.main === module) {
  main().catch(console.error);
}

