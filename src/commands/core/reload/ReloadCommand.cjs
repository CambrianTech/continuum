/**
 * Reload Command
 * Reloads the Continuum system via API
 */

class ReloadCommand {
  static getDefinition() {
    return {
      name: 'RELOAD',
      category: 'Core',
      description: 'Reload the Continuum system (restart server)',
      icon: '🔄',
      params: 'No parameters required',
      usage: 'Gracefully restarts the Continuum server with version bump',
      examples: [
        '{}',
        '"params": "force"'
      ]
    };
  }

  static async execute(params = '', continuum = null, encoding = 'utf-8') {
    try {
      console.log('🔄 API Reload command triggered');
      
      const force = params.trim().toLowerCase() === 'force';
      
      if (force) {
        console.log('⚡ Force reload requested');
      } else {
        console.log('🔄 Graceful reload requested');
      }

      // Schedule the reload to happen after response is sent
      setTimeout(async () => {
        try {
          console.log('🔄 Executing server reload...');
          
          // Import spawn dynamically to restart the process
          const { spawn } = require('child_process');
          const path = require('path');
          
          // Get the current working directory and script path
          const scriptPath = path.join(process.cwd(), 'continuum.cjs');
          const args = ['--restart'];
          
          console.log('🚀 Spawning new Continuum process...');
          
          // Spawn new process with restart flag
          const newProcess = spawn('node', [scriptPath, ...args], {
            detached: true,
            stdio: 'ignore',
            cwd: process.cwd()
          });
          
          // Detach from parent
          newProcess.unref();
          
          console.log('✅ New process spawned, shutting down current instance...');
          
          // Gracefully shut down current process
          process.exit(0);
          
        } catch (error) {
          console.error('❌ Reload failed:', error);
        }
      }, 1000); // Give time for response to be sent
      
      return {
        success: true,
        message: 'Continuum reload initiated - server will restart in 1 second',
        action: force ? 'force_reload' : 'graceful_reload',
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('❌ Reload command failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = ReloadCommand;