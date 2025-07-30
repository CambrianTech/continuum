#!/usr/bin/env npx tsx
/**
 * Smart System Startup - Intelligent JTAG system launcher
 * 
 * Checks if system is running, builds are up to date, and only executes necessary steps.
 * Used by both package.json scripts and jtag shell script for consistent behavior.
 */

import { exec, spawn } from 'child_process';
import { promisify } from 'util';
import { existsSync, statSync } from 'fs';
import { join } from 'path';

const execAsync = promisify(exec);

interface StartupOptions {
  force?: boolean;        // Force restart even if running
  skipBuild?: boolean;    // Skip build check
  background?: boolean;   // Run in background
  verbose?: boolean;      // Verbose logging
}

class SmartSystemStartup {
  private readonly ports = [9001, 9002];
  private readonly requiredDirs = ['dist'];
  private readonly buildIndicatorFiles = ['dist/index.js', 'dist/browser/index.js'];

  async checkPortInUse(port: number): Promise<string | null> {
    try {
      console.log(`🔍 Checking port ${port}...`);
      const { stdout } = await execAsync(`lsof -ti:${port} 2>/dev/null || echo ""`);
      const pid = stdout.trim();
      return pid || null;
    } catch {
      return null;
    }
  }

  async isSystemRunning(): Promise<boolean> {
    const portChecks = await Promise.all(
      this.ports.map(port => this.checkPortInUse(port))
    );
    
    const runningPorts = portChecks.filter(pid => pid !== null);
    const allRunning = runningPorts.length === this.ports.length;
    
    if (allRunning) {
      console.log(`✅ JTAG system running (ports ${this.ports.map((port, i) => `${port}: ${portChecks[i]}`).join(', ')})`);
    } else if (runningPorts.length > 0) {
      console.log(`⚠️  Partial system running (${runningPorts.length}/${this.ports.length} ports)`);
    } else {
      console.log(`❌ JTAG system not running`);
    }
    
    return allRunning;
  }

  async isBuildUpToDate(): Promise<boolean> {
    try {
      // Check if dist directory exists
      if (!existsSync('dist')) {
        console.log(`📦 Build required: dist directory missing`);
        return false;
      }

      // Check if key build files exist
      for (const file of this.buildIndicatorFiles) {
        if (!existsSync(file)) {
          console.log(`📦 Build required: ${file} missing`);
          return false;
        }
      }

      // Check if source files are newer than build files
      const packageJson = statSync('package.json').mtime;
      const tsConfig = statSync('tsconfig.json').mtime;
      const distIndex = statSync('dist/index.js').mtime;
      
      const sourceNewerThanBuild = packageJson > distIndex || tsConfig > distIndex;
      
      if (sourceNewerThanBuild) {
        console.log(`📦 Build required: source files newer than build`);
        return false;
      }

      console.log(`✅ Build is up to date`);
      return true;
    } catch (error) {
      console.log(`📦 Build check failed: ${error}`);
      return false;
    }
  }

  async runNpmStart(background: boolean = false): Promise<void> {
    console.log(`🚀 Starting JTAG system (${background ? 'background' : 'foreground'})...`);
    
    if (background) {
      // Use existing tmux-based system:start for reliable background execution
      console.log(`🔄 Using tmux for reliable background startup...`);
      await execAsync('npm run system:start');
      
      // Wait a bit for startup
      await new Promise(resolve => setTimeout(resolve, 3000));
    } else {
      // Run in foreground
      await execAsync('npm start');
    }
  }

  async waitForSystemReady(timeoutMs: number = 30000): Promise<boolean> {
    console.log(`⏳ Waiting for system to be ready (timeout: ${timeoutMs}ms)...`);
    
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      if (await this.isSystemRunning()) {
        console.log(`✅ System ready!`);
        return true;
      }
      
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    
    console.log(`❌ System not ready after ${timeoutMs}ms`);
    return false;
  }

  async ensureSystemRunning(options: StartupOptions = {}): Promise<boolean> {
    const { force = false, skipBuild = false, background = true, verbose = false } = options;
    
    if (verbose) {
      console.log(`🎯 Smart System Startup - Options:`, { force, skipBuild, background, verbose });
    }

    // Step 1: Check if force restart requested
    if (force) {
      console.log(`🔄 Force restart requested`);
      await this.runNpmStart(background);
      return await this.waitForSystemReady();
    }

    // Step 2: Check if system is already running
    if (await this.isSystemRunning()) {
      console.log(`✅ System already running - no action needed`);
      return true;
    }

    // Step 3: Check if build is needed (unless skipped)
    if (!skipBuild && !await this.isBuildUpToDate()) {
      console.log(`📦 Build out of date - npm start will handle rebuild`);
    }

    // Step 4: Start the system
    await this.runNpmStart(background);
    
    // Step 5: Wait for system to be ready
    return await this.waitForSystemReady();
  }
}

// CLI interface
async function main() {
  const args = process.argv.slice(2);
  
  const options: StartupOptions = {
    force: args.includes('--force') || args.includes('--restart'),
    skipBuild: args.includes('--skip-build'),
    background: !args.includes('--foreground'),
    verbose: args.includes('--verbose') || args.includes('-v')
  };

  const startup = new SmartSystemStartup();
  
  try {
    const success = await startup.ensureSystemRunning(options);
    process.exit(success ? 0 : 1);
  } catch (error) {
    console.error(`❌ Smart startup failed:`, error);
    process.exit(1);
  }
}

// Export for use in other scripts
export { SmartSystemStartup, StartupOptions };

// Run if called directly
if (require.main === module) {
  main();
}