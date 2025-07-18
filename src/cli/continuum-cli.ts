#!/usr/bin/env tsx
// ISSUES: 1 open, last updated 2025-07-15 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// ISSUE #1: CLI is not actually thin - has excessive debug output, health checks, and complex formatting logic
/**
 * Continuum CLI - Ultra-thin pipe that knows NOTHING about commands
 * 
 * All command interface logic is inside the command modules themselves.
 * CLI just forwards raw arguments to the command discovery system via HTTP API.
 * 
 * ✅ CLEANED UP: Removed daemon management logic (2025-07-13)
 * ✅ CLEANED UP: Made CLI ultra-thin HTTP client only (2025-07-13)
 * ✅ CLEANED UP: Added fail-fast validation for command names (2025-07-13)
 * ✅ MODULARIZED: Added pluggable formatter system for command output (2025-07-15)
 */

import { FormatterRegistry } from './formatters/FormatterRegistry';

class ThinContinuumCLI {
  // Ultra-thin CLI connects to existing daemon, never creates daemons
  private formatterRegistry: FormatterRegistry;

  constructor() {
    this.formatterRegistry = new FormatterRegistry();
  }

  /**
   * Universal argument parsing - no special cases, everything is a command
   */
  parseArgs(args: string[]): { command: string; rawArgs: string[] } {
    // Note: args.length === 0 is handled by run() method before this is called
    console.log(`🔬 JTAG CLI: parseArgs called with:`, args);
    
    const firstArg = args[0];
    
    // Map --flags to commands uniformly
    if (firstArg.startsWith('--')) {
      const command = firstArg.substring(2); // --help → help, --version → version, --screenshot → screenshot
      const rawArgs = args.slice(1);
      console.log(`🔬 JTAG CLI: Parsed flag - command: ${command}, rawArgs:`, rawArgs);
      return { command, rawArgs };
    }
    
    // Regular command
    const command = firstArg;
    const rawArgs = args.slice(1);
    console.log(`🔬 JTAG CLI: Parsed regular - command: ${command}, rawArgs:`, rawArgs);
    
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
    const payload = { args: rawArgs };
    console.log(`🔬 JTAG CLI: buildRequestPayload created:`, payload);
    return payload;
  }

  /**
   * Execute command via HTTP API to existing daemon - ultra-thin pipe
   */
  async executeCommand(command: string, rawArgs: string[]): Promise<void> {
    try {
      // FAIL FAST: Validate command name before proceeding
      if (!command || command === 'undefined' || typeof command !== 'string' || command.trim() === '') {
        const error = `❌ JTAG CLI: Invalid command name: "${command}" (type: ${typeof command})`;
        console.error(error);
        console.error(`🔬 JTAG CLI: Raw args:`, rawArgs);
        throw new Error(error);
      }
      
      console.log(`🌐 Executing command: ${command}`);
      console.log(`🔬 JTAG CLI: Using HTTP API to existing daemon (ultra-thin)`);
      
      // Ultra-thin CLI - connect to existing daemon via HTTP
      const baseUrl = 'http://localhost:9000';
      const payload = this.buildRequestPayload(rawArgs);
      
      console.log(`🔬 JTAG CLI: Sending HTTP request to: ${baseUrl}/api/commands/${command}`);
      console.log(`🔬 JTAG CLI: Payload:`, JSON.stringify(payload));
      
      const response = await fetch(`${baseUrl}/api/commands/${command}`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload)
      });
      
      if (!response.ok) {
        const error = `HTTP ${response.status}: ${response.statusText}`;
        console.error(`❌ JTAG CLI: HTTP request failed: ${error}`);
        throw new Error(error);
      }
      
      // Handle response body safely
      const responseText = await response.text();
      console.log(`🔬 JTAG CLI: Raw response text:`, responseText);
      
      let result;
      try {
        result = responseText ? JSON.parse(responseText) : {};
        console.log(`🔬 JTAG CLI: Parsed JSON result:`, result);
      } catch (jsonError) {
        console.log(`⚠️ JTAG CLI: Response is not valid JSON, treating as plain text`);
        result = { data: responseText };
      }
      
      // Check if command returned help/definition format - make it user-friendly
      if (this.isHelpResponse(result, command)) {
        await this.formatHelpResponse(result, command, rawArgs);
      } else if (this.formatterRegistry.format(result, command)) {
        // Formatted by registered formatter
      } else if (result.data && typeof result.data === 'string') {
        console.log(result.data);
      } else {
        console.log(JSON.stringify(result, null, 2));
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      // If command failed (likely missing parameters), auto-show help
      if (errorMessage.includes('Parameter validation failed') || errorMessage.includes('needs parameters')) {
        console.log(`💡 Command '${command}' needs parameters. Showing help:\n`);
        
        try {
          // Auto-call help command via HTTP
          console.log(`🔬 JTAG CLI: Calling help command via HTTP`);
          const helpResponse = await fetch(`http://localhost:9000/api/commands/help`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ args: [command] })
          });
          
          if (helpResponse.ok) {
            const helpResult = await helpResponse.json();
            console.log(JSON.stringify(helpResult, null, 2));
          } else {
            console.error(`❌ JTAG CLI: Help request failed: ${helpResponse.status}`);
          }
          return;
        } catch (helpError) {
          console.error(`❌ JTAG CLI: Failed to show help: ${helpError}`);
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
      // Try to get definition via HTTP
      console.log(`🔬 JTAG CLI: Getting command definition via HTTP for: ${commandName}`);
      const response = await fetch(`http://localhost:9000/api/commands/${commandName}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ args: ['--definition'] })
      });

      if (response.ok) {
        const defResult = await response.json();
        
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
   * Check if daemon is running via HTTP - ultra-thin client
   */
  async isDaemonRunning(): Promise<boolean> {
    try {
      console.log(`🔬 JTAG CLI: Checking daemon health via HTTP`);
      const response = await fetch('http://localhost:9000/api/commands/health', {
        method: 'GET',
        headers: {
          'Content-Type': 'application/json',
        }
      });
      
      if (response.ok) {
        const result = await response.json();
        console.log(`🔬 JTAG CLI: Health check result:`, JSON.stringify(result, null, 2));
        return result.success || result.status === 'healthy' || result.server?.overall === 'healthy';
      }
      
      return false;
    } catch (error) {
      console.log(`🔬 JTAG CLI: Health check failed: ${error}`);
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
    
    // Only check daemon health for daemon management commands
    if (['start', 'stop', 'restart'].includes(command)) {
      const isRunning = await this.isDaemonRunning();
      if (!isRunning && command !== 'start') {
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