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

  async runGentleStart(background: boolean = false): Promise<void> {
    console.log(`🚀 Starting JTAG system gently (no force stop)...`);
    
    // Create the log directory path (relative to current working directory)
    const logDir = 'examples/test-bench/.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs';
    const logFile = `${logDir}/server-node-output.log`;
    
    // Ensure log directory exists
    const mkdirCommand = `mkdir -p ${logDir}`;
    
    // Gentle startup: build and start test-bench without killing existing systems
    const startupCommands = [
      mkdirCommand,
      'npm run build',
      'npm run version:bump', 
      'npm pack',
      'node scripts/update-test-bench.js',
      `cd examples/test-bench && (npm install && npm start) 2>&1 | tee ../../${logFile}`
    ].join(' && ');
    
    console.log(`🔄 Using tmux for gentle startup...`);
    console.log(`📝 Server output will be logged to: ${logFile}`);
    try {
      await execAsync(`tmux new-session -d -s jtag-gentle "${startupCommands}"`);
      console.log(`✅ Started in tmux session 'jtag-gentle'`);
    } catch (error) {
      console.log(`❌ Tmux command failed: ${error}`);
      throw error;
    }
    
    // Wait a bit for startup
    await new Promise(resolve => setTimeout(resolve, 5000));
    
    if (!background) {
      console.log(`💡 System started in tmux session 'jtag-gentle' - ports should be available soon`);
    }
  }

  async runNpmStart(background: boolean = false): Promise<void> {
    console.log(`🚀 Starting JTAG system (${background ? 'background' : 'foreground'})...`);
    
    // Use gentle start to avoid killing existing systems
    await this.runGentleStart(background);
  }

  async waitForSystemReady(timeoutMs: number = 60000): Promise<boolean> {
    console.log(`⏳ Waiting for system to be ready (timeout: ${timeoutMs}ms)...`);
    
    const startTime = Date.now();
    let lastPortCount = 0;
    
    while (Date.now() - startTime < timeoutMs) {
      const portChecks = await Promise.all(
        this.ports.map(port => this.checkPortInUse(port))
      );
      
      const runningPorts = portChecks.filter(pid => pid !== null);
      const currentPortCount = runningPorts.length;
      
      // Show progress if ports are starting up
      if (currentPortCount !== lastPortCount) {
        if (currentPortCount > 0) {
          console.log(`🔄 System starting... (${currentPortCount}/${this.ports.length} ports ready)`);
        }
        lastPortCount = currentPortCount;
      }
      
      // Check if fully ready
      if (currentPortCount === this.ports.length) {
        console.log(`✅ System ready! All ports (${this.ports.join(', ')}) are running`);
        return true;
      }
      
      // Wait 2 seconds between checks (system needs time to build/start)
      await new Promise(resolve => setTimeout(resolve, 2000));
    }
    
    console.log(`❌ System not ready after ${timeoutMs}ms`);
    return false;
  }

  async killExistingProcesses(): Promise<void> {
    console.log(`🛑 Killing existing JTAG processes...`);
    
    for (const port of this.ports) {
      const pid = await this.checkPortInUse(port);
      if (pid) {
        try {
          console.log(`🔫 Killing process ${pid} on port ${port}`);
          await execAsync(`kill -TERM ${pid}`);
          await new Promise(resolve => setTimeout(resolve, 1000)); // Wait for graceful shutdown
          
          // Force kill if still running
          const stillRunning = await this.checkPortInUse(port);
          if (stillRunning) {
            console.log(`🔫 Force killing process ${pid} on port ${port}`);
            await execAsync(`kill -KILL ${pid}`);
          }
        } catch (error) {
          console.warn(`⚠️  Failed to kill process ${pid}: ${error}`);
        }
      }
    }
    
    // Kill tmux sessions
    try {
      await execAsync('tmux kill-session -t jtag-gentle 2>/dev/null || true');
    } catch {
      // Ignore if session doesn't exist
    }
  }

  async getSystemStatus(): Promise<'running' | 'starting' | 'stopped'> {
    const portChecks = await Promise.all(
      this.ports.map(port => this.checkPortInUse(port))
    );
    
    const runningPorts = portChecks.filter(pid => pid !== null);
    
    if (runningPorts.length === this.ports.length) {
      return 'running';
    } else if (runningPorts.length > 0) {
      return 'starting';  
    } else {
      return 'stopped';
    }
  }

  async ensureSystemRunning(options: StartupOptions = {}): Promise<boolean> {
    const { force = false, skipBuild = false, background = true, verbose = false } = options;
    
    if (verbose) {
      console.log(`🎯 Smart System Startup - Options:`, { force, skipBuild, background, verbose });
    }

    const systemStatus = await this.getSystemStatus();
    
    // SCENARIO 1: System fully running
    if (systemStatus === 'running') {
      if (force) {
        console.log(`🔄 Force restart requested - killing existing system first`);
        await this.killExistingProcesses();
        await new Promise(resolve => setTimeout(resolve, 2000)); // Wait for cleanup
        console.log(`🚀 Starting new system after force kill`);
        await this.runNpmStart(background);
        return await this.waitForSystemReady();
      } else {
        console.log(`✅ System already running - connecting to existing system`);
        return true;
      }
    }
    
    // SCENARIO 2: System partially running (starting up)
    if (systemStatus === 'starting') {
      console.log(`⏳ System is starting up - waiting for completion`);
      const ready = await this.waitForSystemReady(30000); // Shorter timeout for already-starting system
      if (ready) {
        return true;
      } else {
        console.log(`❌ System failed to start completely - forcing restart`);
        await this.killExistingProcesses();
        await new Promise(resolve => setTimeout(resolve, 2000));
        await this.runNpmStart(background);
        return await this.waitForSystemReady();
      }
    }

    // SCENARIO 3: System not running 
    console.log(`🚀 No system detected - starting new system`);
    
    // Check if build is needed (unless skipped)
    if (!skipBuild && !await this.isBuildUpToDate()) {
      console.log(`📦 Build out of date - npm start will handle rebuild`);
    }

    await this.runNpmStart(background);
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

/**
 * Convenience function for tests and other scripts
 * Ensures system is running and returns true if successful
 */
export async function ensureJTAGSystemRunning(options: StartupOptions = {}): Promise<boolean> {
  const startup = new SmartSystemStartup();
  try {
    return await startup.ensureSystemRunning({ background: true, ...options });
  } catch (error) {
    console.error(`❌ Failed to ensure JTAG system running:`, error);
    return false;
  }
}

/**
 * Convenience function to stop the system
 */
export async function stopJTAGSystem(): Promise<boolean> {
  try {
    console.log(`🛑 Stopping JTAG system...`);
    await execAsync('npm run system:stop');
    console.log(`✅ JTAG system stopped`);
    return true;
  } catch (error) {
    console.error(`❌ Failed to stop JTAG system:`, error);
    return false;
  }
}

// Export for use in other scripts
export { SmartSystemStartup, StartupOptions };

// Run if called directly
if (require.main === module) {
  main();
}