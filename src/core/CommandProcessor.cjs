/**
 * Command Processor
 * Handles AI protocol parsing and command execution
 */

const { spawn } = require('child_process');
const fs = require('fs');
const fetch = (...args) => import('node-fetch').then(({default: fetch}) => fetch(...args));

class CommandProcessor {
  constructor() {
    this.commands = new Map();
    this.setupDefaultCommands();
  }

  setupDefaultCommands() {
    this.commands.set('EXEC', this.executeShellCommand.bind(this));
    this.commands.set('FILE_READ', this.readFile.bind(this));
    this.commands.set('FILE_WRITE', this.writeFile.bind(this));
    this.commands.set('WEBFETCH', this.webFetch.bind(this));
    this.commands.set('PYTHON', this.executePython.bind(this));
  }

  parseAIProtocol(response) {
    const lines = response.split('\n');
    const commands = [];
    let statusMessage = null;
    let chatMessage = null;

    for (const line of lines) {
      const trimmed = line.trim();
      
      // Parse status messages
      if (trimmed.startsWith('[STATUS]')) {
        statusMessage = trimmed.replace('[STATUS]', '').trim();
        console.log(`📊 Protocol Status: ${statusMessage}`);
      }
      
      // Parse chat messages
      else if (trimmed.startsWith('[CHAT]')) {
        chatMessage = trimmed.replace('[CHAT]', '').trim();
      }
      
      // Parse commands
      else if (trimmed.startsWith('[CMD:')) {
        const match = trimmed.match(/\[CMD:(\w+)\]\s*(.+)/);
        if (match) {
          const [, command, params] = match;
          commands.push({ command, params: params.trim() });
          console.log(`📤 Protocol Command ${commands.length}: ${command} - ${params.substring(0, 50)}${params.length > 50 ? '...' : ''}`);
        }
      }
    }

    return {
      commands,
      statusMessage,
      chatMessage,
      hasCommands: commands.length > 0
    };
  }

  async processToolCommands(response) {
    console.log('🔍 Processing AI protocol...');
    const parsed = this.parseAIProtocol(response);
    
    console.log(`🎯 Total Commands Found: ${parsed.commands.length}`);
    
    if (parsed.commands.length === 0) {
      console.log('🔍 Scanning AI response for legacy tool commands...');
      return [];
    }

    const results = [];
    for (const cmd of parsed.commands) {
      console.log(`⚡ Executing dynamic command: ${cmd.command}`);
      try {
        const result = await this.executeCommand(cmd.command, cmd.params);
        results.push({
          tool: cmd.command,
          params: cmd.params,
          result: result
        });
      } catch (error) {
        console.error(`❌ Command ${cmd.command} failed: ${error.message}`);
        results.push({
          tool: cmd.command,
          params: cmd.params,
          result: `Error: ${error.message}`
        });
      }
    }

    console.log(`✅ Executed ${results.length} tool commands`);
    return results;
  }

  async executeCommand(command, params) {
    console.log(`🔧 EXECUTING COMMAND: ${command} with params: ${params.substring(0, 50)}${params.length > 50 ? '...' : ''}`);
    
    const handler = this.commands.get(command);
    if (handler) {
      return await handler(params);
    } else {
      throw new Error(`Unknown command: ${command}`);
    }
  }

  async executeShellCommand(command) {
    console.log(`⚡ Exec Command: ${command}`);
    
    return new Promise((resolve, reject) => {
      const process = spawn('bash', ['-c', command], {
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 30000
      });

      let stdout = '';
      let stderr = '';

      process.stdout.on('data', (data) => {
        stdout += data.toString();
      });

      process.stderr.on('data', (data) => {
        stderr += data.toString();
      });

      process.on('close', (code) => {
        const output = stdout || stderr || 'No output';
        console.log(`⚡ Exec Result: ${output.substring(0, 100)}${output.length > 100 ? '...' : ''}`);
        
        if (code !== 0 && stderr) {
          reject(new Error(`Command failed: ${command}\\n${stderr}`));
        } else {
          resolve(output);
        }
      });

      process.on('error', (error) => {
        console.log(`❌ Command execution failed: ${error.message}`);
        reject(error);
      });
    });
  }

  async readFile(filePath) {
    console.log(`📖 Reading file: ${filePath}`);
    try {
      const content = fs.readFileSync(filePath, 'utf-8');
      const length = content.length;
      console.log(`📖 File content length: ${length} chars`);
      return content.substring(0, 2000) + (length > 2000 ? '\\n... (truncated)' : '');
    } catch (error) {
      throw new Error(`Failed to read file ${filePath}: ${error.message}`);
    }
  }

  async writeFile(filePath, content = '') {
    console.log(`📝 Writing file: ${filePath} (${content.length} chars)`);
    try {
      fs.writeFileSync(filePath, content);
      return `File written successfully: ${filePath}`;
    } catch (error) {
      throw new Error(`Failed to write file ${filePath}: ${error.message}`);
    }
  }

  async webFetch(url) {
    console.log(`🌐 WebFetch URL: ${url}`);
    console.log(`🌐 Fetching content from: ${url}`);
    
    try {
      const response = await fetch(url, {
        headers: {
          'User-Agent': 'Continuum-AI/1.0 (AI Assistant)'
        },
        timeout: 15000
      });
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      
      const content = await response.text();
      const result = content.substring(0, 2000) + (content.length > 2000 ? '...' : '');
      console.log(`🌐 WebFetch Result: ${result.substring(0, 200)}${result.length > 200 ? '...' : ''}`);
      return result;
    } catch (error) {
      throw new Error(`Web fetch failed for ${url}: ${error.message}`);
    }
  }

  async executePython(code) {
    console.log(`🐍 Python Code: ${code.substring(0, 100)}${code.length > 100 ? '...' : ''}`);
    
    // Write Python code to temporary file
    const tempFile = `temp_script_${Date.now()}.py`;
    
    try {
      await this.writeFile(tempFile, code);
      
      // Try different Python commands
      const pythonCommands = ['python3', 'python'];
      let result = null;
      let lastError = null;
      
      for (const pythonCmd of pythonCommands) {
        try {
          result = await this.executeShellCommand(`${pythonCmd} ${tempFile}`);
          break;
        } catch (error) {
          lastError = error;
          console.log(`⚠️  ${pythonCmd} failed, trying next...`);
        }
      }
      
      if (!result) {
        throw lastError || new Error('No Python interpreter found');
      }
      
      return result;
    } finally {
      // Cleanup temp file
      try {
        const fs = require('fs');
        if (fs.existsSync(tempFile)) {
          fs.unlinkSync(tempFile);
        }
      } catch (cleanupError) {
        console.log('⚠️  Could not cleanup temp Python file');
      }
    }
  }
}

module.exports = CommandProcessor;