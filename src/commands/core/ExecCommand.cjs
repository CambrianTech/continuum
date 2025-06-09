/**
 * Execute Shell Command
 * Executes shell commands on the server
 */

const { spawn } = require('child_process');

class ExecCommand {
  static getDefinition() {
    return {
      name: 'EXEC',
      category: 'Core',
      icon: '⚡',
      description: 'Execute shell command',
      params: '<command_and_args>',
      examples: [
        '{"params": "ls -la"}',
        '{"params": "ps aux | grep node"}',
        '{"params": "git status"}',
        '{"params": "npm test"}'
      ],
      usage: 'Execute shell commands on the server. Use with caution - has full system access.'
    };
  }
  
  static async execute(params, continuum) {
    console.log('⚡ Exec command:', params);
    
    if (!params || params.trim() === '') {
      return {
        success: false,
        error: 'No command provided'
      };
    }
    
    try {
      const result = await ExecCommand.executeShellCommand(params);
      return {
        success: true,
        command: params,
        output: result.stdout,
        error: result.stderr,
        exitCode: result.exitCode,
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error.message,
        command: params,
        timestamp: new Date().toISOString()
      };
    }
  }
  
  static executeShellCommand(command) {
    return new Promise((resolve, reject) => {
      // Parse command and arguments
      const parts = command.trim().split(/\s+/);
      const cmd = parts[0];
      const args = parts.slice(1);
      
      console.log(`⚡ Executing: ${cmd} with args:`, args);
      
      const child = spawn(cmd, args, {
        stdio: 'pipe',
        shell: true
      });
      
      let stdout = '';
      let stderr = '';
      
      child.stdout.on('data', (data) => {
        stdout += data.toString();
      });
      
      child.stderr.on('data', (data) => {
        stderr += data.toString();
      });
      
      child.on('close', (code) => {
        console.log(`⚡ Command finished with exit code: ${code}`);
        resolve({
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          exitCode: code
        });
      });
      
      child.on('error', (error) => {
        console.error('⚡ Command error:', error);
        reject(error);
      });
      
      // Timeout after 30 seconds
      setTimeout(() => {
        child.kill('SIGTERM');
        reject(new Error('Command timeout after 30 seconds'));
      }, 30000);
    });
  }
}

module.exports = ExecCommand;