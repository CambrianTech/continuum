/**
 * RestartCommand - Force restart the server with version bump
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

class RestartCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'restart',
      description: 'Force restart the server (kill existing instance)',
      icon: '🔄',
      parameters: {
        bump: {
          type: 'boolean',
          required: false,
          description: 'Bump version before restart',
          default: true
        },
        timeout: {
          type: 'number',
          required: false,
          description: 'Graceful shutdown timeout in seconds',
          default: 5
        }
      },
      examples: [
        'restart',
        'restart --no-bump',
        'restart --timeout 10'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const shouldBump = options.bump !== false;
    const timeout = options.timeout || 5;
    
    console.log('🔄 Force restarting Continuum server...');
    
    try {
      // Bump version first if requested
      if (shouldBump) {
        console.log('📈 Bumping version...');
        const newVersion = await this.bumpVersion();
        if (newVersion) {
          console.log(`✅ Version bumped to: ${newVersion}`);
        } else {
          console.log('⚠️ Version bump failed, continuing with restart...');
        }
      }
      
      // Kill existing processes
      const killedAny = await this.killExistingProcesses(timeout);
      
      if (killedAny) {
        console.log('✅ Existing Continuum processes terminated');
        // Wait a bit more to ensure port is freed
        await this.sleep(1000);
      } else {
        console.log('🔍 No existing Continuum processes found');
      }
      
      // Start fresh instance using spawn (like reload does)
      console.log('🚀 Starting fresh Continuum process...');
      const { spawn } = require('child_process');
      const path = require('path');
      
      // Get the continuum.cjs script path
      const scriptPath = path.join(process.cwd(), 'continuum.cjs');
      
      // Spawn new process 
      const newProcess = spawn('node', [scriptPath], {
        detached: true,
        stdio: 'ignore',
        cwd: process.cwd()
      });
      
      // Detach from parent
      newProcess.unref();
      
      console.log('✅ New Continuum process spawned');
      
      return this.createSuccessResult({ 
        versionBumped: shouldBump, 
        processesKilled: killedAny 
      }, 'Restart initiated');
      
    } catch (error) {
      console.error('❌ Restart failed:', error);
      return this.createErrorResult('Restart failed', error.message);
    }
  }
  
  static async bumpVersion() {
    try {
      return new Promise((resolve) => {
        const versionProcess = spawn('npm', ['version', 'patch', '--no-git-tag-version'], {
          stdio: 'pipe',
          cwd: process.cwd()
        });
        
        let output = '';
        versionProcess.stdout.on('data', (data) => {
          output += data.toString();
        });
        
        versionProcess.on('close', (code) => {
          if (code === 0) {
            const newVersion = output.trim().replace(/^v/, '');
            resolve(newVersion);
          } else {
            resolve(null);
          }
        });
        
        versionProcess.on('error', () => {
          resolve(null);
        });
      });
    } catch (error) {
      return null;
    }
  }
  
  static async killExistingProcesses(timeout) {
    const cwd = process.cwd();
    const pidPaths = [
      path.join(cwd, '.continuum', 'continuum.pid'),
      path.join(cwd, 'continuum.pid'),
      path.join(require('os').homedir(), '.continuum', 'continuum.pid')
    ];
    
    let killedAny = false;
    
    for (const pidPath of pidPaths) {
      if (fs.existsSync(pidPath)) {
        try {
          const pid = parseInt(fs.readFileSync(pidPath, 'utf8').trim());
          
          console.log(`🔍 Found PID ${pid} in ${pidPath}`);
          
          // Try to kill the process
          try {
            process.kill(pid, 'SIGTERM');
            console.log(`💀 Sent SIGTERM to process ${pid}`);
            
            // Wait for graceful shutdown
            await this.sleep(timeout * 1000);
            
            // Check if process is still running, force kill if needed
            try {
              process.kill(pid, 0); // Check if process exists
              console.log(`⚡ Process ${pid} still running, sending SIGKILL`);
              process.kill(pid, 'SIGKILL');
            } catch (error) {
              // Process already dead, which is what we want
            }
            
            killedAny = true;
          } catch (error) {
            if (error.code === 'ESRCH') {
              console.log(`💀 Process ${pid} was already dead`);
            } else {
              console.log(`⚠️  Could not kill process ${pid}: ${error.message}`);
            }
          }
          
          // Remove the PID file
          fs.unlinkSync(pidPath);
          console.log(`🗑️  Removed PID file: ${pidPath}`);
          
        } catch (error) {
          console.log(`⚠️  Could not read PID file ${pidPath}: ${error.message}`);
        }
      }
    }
    
    return killedAny;
  }
  
  static sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

module.exports = RestartCommand;