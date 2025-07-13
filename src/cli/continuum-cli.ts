#!/usr/bin/env tsx
/**
 * Continuum CLI - Ultra-thin pipe that knows NOTHING about commands
 * 
 * All command interface logic is inside the command modules themselves.
 * CLI just forwards raw arguments to the command discovery system.
 */

class ThinContinuumCLI {
  private baseUrl = 'http://localhost:9000';

  /**
   * Universal argument parsing - no special cases, everything is a command
   */
  parseArgs(args: string[]): { command: string; rawArgs: string[] } {
    // Note: args.length === 0 is handled by run() method before this is called
    
    const firstArg = args[0];
    
    // Map --flags to commands uniformly
    if (firstArg.startsWith('--')) {
      const command = firstArg.substring(2); // --help → help, --version → version, --screenshot → screenshot
      const rawArgs = args.slice(1);
      return { command, rawArgs };
    }
    
    // Regular command
    const command = firstArg;
    const rawArgs = args.slice(1);
    
    return { command, rawArgs };
  }

  /**
   * No command translation - commands work exactly as they are
   */
  async adaptCommand(command: string, rawArgs: string[]): Promise<{ command: string; rawArgs: string[] }> {
    // No translation, no fuzzy matching, no aliases
    // Commands must be called exactly as they exist
    return { command, rawArgs };
  }

  /**
   * Minimal payload - just forward raw arguments
   */
  buildRequestPayload(rawArgs: string[]): Record<string, any> {
    // The command module itself handles all argument interpretation
    return { args: rawArgs };
  }

  /**
   * Execute command via API with dynamic adaptation and ideal UX
   */
  async executeCommand(command: string, rawArgs: string[]): Promise<void> {
    try {
      // Dynamic adaptation - let command system handle UX improvements
      const adapted = await this.adaptCommand(command, rawArgs);
      const payload = this.buildRequestPayload(adapted.rawArgs);
      
      console.log(`🌐 Executing command: ${adapted.command}`);
      
      const response = await fetch(`${this.baseUrl}/api/commands/${adapted.command}`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload)
      });
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      
      const result = await response.json();
      
      // Check if command returned help/definition format - make it user-friendly
      if (this.isHelpResponse(result, adapted.command)) {
        await this.formatHelpResponse(result, adapted.command, rawArgs);
      } else if (result.data && typeof result.data === 'string') {
        console.log(result.data);
      } else {
        console.log(JSON.stringify(result, null, 2));
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      // If command failed with 500 (likely missing parameters), auto-show help
      if (errorMessage.includes('HTTP 500')) {
        console.log(`💡 Command '${command}' needs parameters. Showing help:\n`);
        
        try {
          // Auto-call help command by spawning new CLI process
          const { spawn } = await import('child_process');
          const helpProcess = spawn('npx', ['tsx', 'src/cli/continuum-cli.ts', 'help', command], { 
            stdio: 'inherit',
            cwd: process.cwd()
          });
          
          await new Promise((resolve) => {
            helpProcess.on('close', resolve);
          });
          return;
        } catch {
          // Fallback to simple guidance if help command fails
        }
        
        console.log(`📖 Usage: continuum ${command} [options]`);
        console.log(`💡 Try: continuum help ${command} for detailed help`);
        return;
      }
      
      // Other errors
      console.error(`❌ Command failed: ${errorMessage}`);
      process.exit(1);
    }
  }


  /**
   * Check if response is help/definition format
   */
  private isHelpResponse(result: any, command: string): boolean {
    return command === 'help' || 
           (result.commands && Array.isArray(result.commands)) ||
           (result.commandsByCategory && typeof result.commandsByCategory === 'object');
  }

  /**
   * Format help response by reading command definitions dynamically
   */
  private async formatHelpResponse(result: any, _command: string, rawArgs: string[]): Promise<void> {
    if (rawArgs.length > 0) {
      // Specific command help requested - read that command's definition
      const targetCommand = rawArgs[0];
      await this.showCommandDefinition(targetCommand);
    } else {
      // General help - show all commands with their definitions
      this.showGeneralHelp(result);
    }
  }

  /**
   * Show command definition by reading it from the command module
   */
  private async showCommandDefinition(commandName: string): Promise<void> {
    try {
      // Try to get definition via API first
      const defResponse = await fetch(`${this.baseUrl}/api/commands/${commandName}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ args: ['--definition'] })
      });

      if (defResponse.ok) {
        const defResult = await defResponse.json();
        
        if (defResult.definition) {
          this.displayCommandDefinition(commandName, defResult.definition);
          return;
        }
      }

      // Fallback: Try to read definition by convention
      await this.displayCommandHelp(commandName);
      
    } catch (error) {
      console.log(`\n📖 ${commandName.toUpperCase()}\n`);
      console.log(`Command available - try: continuum ${commandName} --help\n`);
    }
  }

  /**
   * Display command definition in user-friendly format
   */
  private displayCommandDefinition(commandName: string, definition: any): void {
    console.log(`\n📖 ${definition.name?.toUpperCase() || commandName.toUpperCase()}`);
    console.log(`${definition.icon || '⚡'} ${definition.description || 'Command available'}\n`);
    
    console.log(`Usage: continuum ${commandName} [options]\n`);
    
    if (definition.parameters && Object.keys(definition.parameters).length > 0) {
      console.log('📋 Parameters:');
      Object.entries(definition.parameters).forEach(([param, config]: [string, any]) => {
        const type = config.type || 'string';
        const desc = config.description || 'No description';
        console.log(`  --${param}  (${type})  ${desc}`);
      });
      console.log('');
    }
    
    if (definition.examples && definition.examples.length > 0) {
      console.log('💡 Examples:');
      definition.examples.forEach((example: any) => {
        console.log(`  # ${example.description}`);
        console.log(`  continuum ${commandName} ${this.formatExampleParams(example.command)}`);
        console.log('');
      });
    }
    
    console.log(`🔍 For more details: continuum ${commandName} --help\n`);
  }

  /**
   * Format example parameters from JSON to CLI format
   */
  private formatExampleParams(command: string): string {
    try {
      const params = JSON.parse(command);
      return Object.entries(params)
        .map(([key, value]) => `--${key}="${value}"`)
        .join(' ');
    } catch {
      return command;
    }
  }

  /**
   * Display general help with command discovery
   */
  private showGeneralHelp(result: any): void {
    console.log('\n🌐 CONTINUUM COMMANDS\n');
    
    if (result.commandsByCategory) {
      Object.entries(result.commandsByCategory).forEach(([category, commands]: [string, any]) => {
        console.log(`📋 ${category.toUpperCase()}:`);
        (commands as string[]).forEach(cmd => {
          console.log(`  continuum ${cmd}       # or continuum --${cmd}`);
        });
        console.log('');
      });
    }
    
    console.log('💡 Get command help: continuum help [command] or continuum --help [command]');
    console.log('🔧 Universal syntax: continuum [command] or continuum --[command]');
    console.log('📝 Commands must be called exactly as listed - no aliases or translations\n');
  }

  /**
   * Fallback help display
   */
  private async displayCommandHelp(commandName: string): Promise<void> {
    console.log(`\n📖 ${commandName.toUpperCase()}\n`);
    console.log(`Usage: continuum ${commandName} [options]\n`);
    console.log(`💡 Command available in the system`);
    console.log(`   Try: continuum ${commandName} --help`);
    console.log(`   Or: continuum --${commandName} [options]`);
    console.log(`   Or: continuum help\n`);
  }

  /**
   * Check if daemon is running
   */
  async isDaemonRunning(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/api/health`);
      return response.ok;
    } catch {
      return false;
    }
  }

  /**
   * Main CLI entry point - ultra-thin
   */
  async run(): Promise<void> {
    const args = process.argv.slice(2);
    
    // Show CLI help for bare help commands or empty args
    if (args.length === 0 || (args.length === 1 && (args[0] === '--help' || args[0] === '-h'))) {
      this.showHelp();
      return;
    }
    
    const { command, rawArgs } = this.parseArgs(args);
    
    // Check if daemon is running (except for start/stop commands)
    if (!['start', 'stop', 'restart'].includes(command)) {
      const isRunning = await this.isDaemonRunning();
      if (!isRunning) {
        console.error('❌ Daemon not running. Please run: ./continuum start');
        process.exit(1);
      }
    }
    
    await this.executeCommand(command, rawArgs);
  }

  /**
   * Ultra-minimal help - CLI knows nothing about command interfaces
   */
  showHelp(): void {
    console.log(`
Continuum CLI - Ultra-thin command pipe

Usage:
  continuum [command] [arguments...]

The CLI forwards all arguments directly to the command modules.
Each command defines its own interface and argument handling.

For command-specific help:
  continuum [command] --help
  
💡 CLI knows nothing about command parameters - all logic is in command modules.
`);
  }
}

// Run CLI if called directly
const isMainModule = import.meta.url === `file://${process.argv[1]}`;
if (isMainModule) {
  const cli = new ThinContinuumCLI();
  cli.run().catch((error) => {
    console.error('❌ CLI Error:', error);
    process.exit(1);
  });
}

export { ThinContinuumCLI };