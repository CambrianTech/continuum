/**
 * TypeScript Command Processor - Modern command execution system
 * Replaces legacy CommandRegistry.cjs with full TypeScript architecture
 * Handles case-insensitive command lookup with proper parameter handling
 */

import { promises as fs } from 'fs';
import path from 'path';
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../commands/core/BaseCommand';
import { ProcessorConfig, CommandModule, CommandStats, ExecutionContext, LoadResult } from './types';

// Remove duplicate interfaces - now imported from types.ts

export class TypeScriptCommandProcessor {
  private commands = new Map<string, typeof BaseCommand>();
  private definitions = new Map<string, CommandDefinition>();
  private initialized = false;
  private config: ProcessorConfig;

  constructor(config: Partial<ProcessorConfig> = {}) {
    this.config = {
      commandDirs: [
        'src/commands/core',
        'src/commands/ui', 
        'src/commands/browser',
        'src/commands/file',
        'src/commands/docs',
        'src/commands/planning',
        'src/commands/development',
        'src/commands/monitoring',
        'src/commands/communication'
      ],
      enableCaseInsensitive: true, // Always case-insensitive - this is the right place for it
      enableTypeScriptOnly: false,
      logLevel: 'info',
      ...config
    };
  }

  /**
   * Initialize the command processor by scanning for TypeScript commands
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    this.log('info', '🚀 Initializing TypeScript Command Processor');
    
    let totalLoaded = 0;
    let totalErrors = 0;

    for (const commandDir of this.config.commandDirs) {
      const { loaded, errors } = await this.loadCommandsFromDirectory(commandDir);
      totalLoaded += loaded;
      totalErrors += errors;
    }

    this.initialized = true;
    this.log('info', `✅ TypeScript Command Processor initialized: ${totalLoaded} commands loaded, ${totalErrors} errors`);
    
    if (this.config.logLevel === 'debug') {
      this.logCommandSummary();
    }
  }

  /**
   * Execute a command with proper context and error handling
   */
  async executeCommand<T = any, R = any>(
    commandName: string, 
    params: T, 
    context?: CommandContext
  ): Promise<CommandResult<R>> {
    await this.initialize();

    // Case-insensitive command lookup
    const normalizedName = this.config.enableCaseInsensitive 
      ? commandName.toLowerCase() 
      : commandName;
    
    const CommandClass = this.commands.get(normalizedName);
    
    if (!CommandClass) {
      return {
        success: false,
        message: `Command '${commandName}' not found`,
        error: `Available commands: ${Array.from(this.commands.keys()).join(', ')}`,
        timestamp: new Date().toISOString()
      };
    }

    try {
      this.log('debug', `⚡ Executing TypeScript command: ${commandName}`);
      
      // Create enriched context
      const executionContext: CommandContext = {
        ...context,
        processor: 'typescript',
        executionId: this.generateExecutionId(),
        timestamp: new Date()
      };

      const result = await CommandClass.execute(params, executionContext);
      
      this.log('debug', `✅ Command ${commandName} completed: ${result.success ? 'SUCCESS' : 'FAILED'}`);
      return result;

    } catch (error) {
      this.log('error', `❌ Command ${commandName} failed: ${error.message}`);
      return {
        success: false,
        message: `Command execution failed: ${error.message}`,
        error: error.stack,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get command definition
   */
  getDefinition(commandName: string): CommandDefinition | undefined {
    const normalizedName = this.config.enableCaseInsensitive 
      ? commandName.toLowerCase() 
      : commandName;
    
    return this.definitions.get(normalizedName);
  }

  /**
   * Get all available commands
   */
  getAllCommands(): string[] {
    return Array.from(this.commands.keys());
  }

  /**
   * Get all command definitions
   */
  getAllDefinitions(): CommandDefinition[] {
    return Array.from(this.definitions.values());
  }

  /**
   * Get commands by category
   */
  getCommandsByCategory(category: string): CommandDefinition[] {
    return this.getAllDefinitions().filter(def => def.category === category);
  }

  /**
   * Get all categories
   */
  getCategories(): string[] {
    return [...new Set(this.getAllDefinitions().map(def => def.category))];
  }

  /**
   * Load commands from a directory recursively
   */
  private async loadCommandsFromDirectory(dir: string): Promise<{ loaded: number; errors: number }> {
    let loaded = 0;
    let errors = 0;

    try {
      const fullPath = path.resolve(dir);
      const exists = await fs.access(fullPath).then(() => true).catch(() => false);
      
      if (!exists) {
        this.log('warn', `Command directory not found: ${dir}`);
        return { loaded, errors };
      }

      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      
      for (const entry of entries) {
        const entryPath = path.join(fullPath, entry.name);
        
        if (entry.isDirectory()) {
          // Recursively load subdirectories
          const subResult = await this.loadCommandsFromDirectory(entryPath);
          loaded += subResult.loaded;
          errors += subResult.errors;
        } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) {
          // Load TypeScript command files
          try {
            await this.loadTypeScriptCommand(entryPath);
            loaded++;
          } catch (error) {
            this.log('error', `Failed to load TypeScript command ${entryPath}: ${error.message}`);
            errors++;
          }
        }
      }

    } catch (error) {
      this.log('error', `Failed to scan directory ${dir}: ${error.message}`);
      errors++;
    }

    return { loaded, errors };
  }

  /**
   * Load a single TypeScript command file
   */
  private async loadTypeScriptCommand(filePath: string): Promise<void> {
    try {
      this.log('debug', `🔷 Loading TypeScript command: ${path.basename(filePath)}`);
      
      // Dynamic import with file:// URL for TypeScript files
      const moduleUrl = `file://${filePath}`;
      const module: CommandModule = await import(moduleUrl);
      
      // Extract command class
      const CommandClass = module.default || 
                          Object.values(module).find(exp => 
                            exp && 
                            typeof exp === 'function' && 
                            typeof (exp as any).getDefinition === 'function' && 
                            typeof (exp as any).execute === 'function'
                          ) as typeof BaseCommand;

      if (!CommandClass) {
        throw new Error(`No valid command class found in ${filePath}`);
      }

      // Get command definition
      const definition = CommandClass.getDefinition();
      if (!definition || !definition.name) {
        throw new Error(`Invalid command definition in ${filePath}`);
      }

      // Register command with case handling
      const commandName = this.config.enableCaseInsensitive 
        ? definition.name.toLowerCase() 
        : definition.name;

      this.commands.set(commandName, CommandClass);
      this.definitions.set(commandName, definition);

      this.log('debug', `✅ Registered TypeScript command: ${commandName} (${definition.category})`);

    } catch (error) {
      throw new Error(`Failed to load TypeScript command from ${filePath}: ${error.message}`);
    }
  }

  /**
   * Generate unique execution ID
   */
  private generateExecutionId(): string {
    return `ts_exec_${Date.now().toString(36)}_${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Log command summary for debugging
   */
  private logCommandSummary(): void {
    this.log('debug', '📋 Loaded TypeScript Commands:');
    
    const categories = this.getCategories();
    for (const category of categories) {
      const commands = this.getCommandsByCategory(category);
      this.log('debug', `  📁 ${category}: ${commands.map(c => c.name).join(', ')}`);
    }
  }

  /**
   * Internal logging with levels
   */
  private log(level: ProcessorConfig['logLevel'], message: string): void {
    const levels = { debug: 0, info: 1, warn: 2, error: 3 };
    const currentLevel = levels[this.config.logLevel];
    const messageLevel = levels[level];

    if (messageLevel >= currentLevel) {
      const prefix = {
        debug: '🔍',
        info: 'ℹ️',
        warn: '⚠️',
        error: '❌'
      }[level];
      
      console.log(`${prefix} [TypeScriptCommandProcessor] ${message}`);
    }
  }
}

// ============================================================================
// UNIT TESTS - Embedded in module for immediate testing
// ============================================================================

export namespace TypeScriptCommandProcessorTests {
  
  export async function runUnitTests(): Promise<void> {
    console.log('🧪 Running TypeScript Command Processor Unit Tests...\n');
    
    let passed = 0;
    let failed = 0;
    
    const test = async (name: string, testFn: () => Promise<void> | void) => {
      try {
        console.log(`🔬 ${name}`);
        await testFn();
        console.log('  ✅ PASSED\n');
        passed++;
      } catch (error) {
        console.log(`  ❌ FAILED: ${error.message}\n`);
        failed++;
      }
    };

    // Test 1: Processor initialization
    await test('Processor initializes correctly', async () => {
      const processor = new TypeScriptCommandProcessor({
        commandDirs: ['src/commands/core'],
        logLevel: 'error' // Quiet for testing
      });
      
      if (processor.getAllCommands().length > 0) {
        throw new Error('Commands should be empty before initialization');
      }
      
      await processor.initialize();
      
      if (!processor.getAllCommands().length) {
        console.log('  ⚠️  No commands loaded - this is expected if no TypeScript commands exist yet');
      }
    });

    // Test 2: Case insensitive command lookup
    await test('Case insensitive command handling', async () => {
      const processor = new TypeScriptCommandProcessor({
        commandDirs: [],
        logLevel: 'error'
      });
      
      // Mock a command for testing
      const mockDefinition = {
        name: 'test-command',
        category: 'test',
        description: 'Test command',
        icon: '🧪',
        params: '{}',
        usage: 'Test usage',
        examples: []
      };
      
      // Direct injection for testing
      (processor as any).commands.set('test-command', {
        getDefinition: () => mockDefinition,
        execute: async () => ({ success: true, message: 'Test passed' })
      });
      (processor as any).definitions.set('test-command', mockDefinition);
      (processor as any).initialized = true;
      
      // Test case variations
      const variations = ['test-command', 'TEST-COMMAND', 'Test-Command', 'tEsT-CoMmAnD'];
      
      for (const variation of variations) {
        const result = await processor.executeCommand(variation, {});
        if (!result.success) {
          throw new Error(`Case insensitive lookup failed for: ${variation}`);
        }
      }
    });

    // Test 3: Command execution with context
    await test('Command execution with proper context', async () => {
      const processor = new TypeScriptCommandProcessor({
        commandDirs: [],
        logLevel: 'error'
      });
      
      let receivedContext: any = null;
      
      const mockCommand = {
        getDefinition: () => ({
          name: 'context-test',
          category: 'test',
          description: 'Context test',
          icon: '🔧',
          params: '{}',
          usage: 'Test',
          examples: []
        }),
        execute: async (params: any, context: any) => {
          receivedContext = context;
          return { success: true, message: 'Context received' };
        }
      };
      
      (processor as any).commands.set('context-test', mockCommand);
      (processor as any).initialized = true;
      
      const testContext = { test: 'value', continuum: { version: '1.0' } };
      await processor.executeCommand('context-test', {}, testContext);
      
      if (!receivedContext) {
        throw new Error('Context was not passed to command');
      }
      
      if (receivedContext.processor !== 'typescript') {
        throw new Error('Processor type not set in context');
      }
      
      if (!receivedContext.executionId) {
        throw new Error('Execution ID not generated');
      }
      
      if (receivedContext.test !== 'value') {
        throw new Error('Original context not preserved');
      }
    });

    // Test 4: Error handling
    await test('Error handling and reporting', async () => {
      const processor = new TypeScriptCommandProcessor({
        commandDirs: [],
        logLevel: 'error'
      });
      
      const errorCommand = {
        getDefinition: () => ({
          name: 'error-test',
          category: 'test',
          description: 'Error test',
          icon: '💥',
          params: '{}',
          usage: 'Test',
          examples: []
        }),
        execute: async () => {
          throw new Error('Intentional test error');
        }
      };
      
      (processor as any).commands.set('error-test', errorCommand);
      (processor as any).initialized = true;
      
      const result = await processor.executeCommand('error-test', {});
      
      if (result.success) {
        throw new Error('Error command should have failed');
      }
      
      if (!result.message.includes('Intentional test error')) {
        throw new Error('Error message not properly captured');
      }
    });

    // Test 5: Command not found handling
    await test('Command not found handling', async () => {
      const processor = new TypeScriptCommandProcessor({
        commandDirs: [],
        logLevel: 'error'
      });
      
      await processor.initialize();
      
      const result = await processor.executeCommand('nonexistent-command', {});
      
      if (result.success) {
        throw new Error('Nonexistent command should fail');
      }
      
      if (!result.message.includes('not found')) {
        throw new Error('Not found message not proper');
      }
    });

    console.log(`\n📊 Unit Test Results: ${passed} passed, ${failed} failed`);
    
    if (failed > 0) {
      throw new Error(`${failed} unit tests failed`);
    }
  }

  export async function runIntegrationTests(): Promise<void> {
    console.log('🔧 Running TypeScript Command Processor Integration Tests...\n');
    
    let passed = 0;
    let failed = 0;
    
    const test = async (name: string, testFn: () => Promise<void> | void) => {
      try {
        console.log(`🔗 ${name}`);
        await testFn();
        console.log('  ✅ PASSED\n');
        passed++;
      } catch (error) {
        console.log(`  ❌ FAILED: ${error.message}\n`);
        failed++;
      }
    };

    // Integration Test 1: Real command directory scanning
    await test('Real command directory scanning', async () => {
      const processor = new TypeScriptCommandProcessor({
        logLevel: 'error'
      });
      
      await processor.initialize();
      
      const commands = processor.getAllCommands();
      const definitions = processor.getAllDefinitions();
      
      console.log(`  📋 Found ${commands.length} TypeScript commands`);
      console.log(`  📋 Categories: ${processor.getCategories().join(', ')}`);
      
      if (commands.length !== definitions.length) {
        throw new Error('Command count mismatch between commands and definitions');
      }
      
      // Check that each command has a valid definition
      for (const commandName of commands) {
        const definition = processor.getDefinition(commandName);
        if (!definition) {
          throw new Error(`No definition found for command: ${commandName}`);
        }
        
        if (!definition.name || !definition.category) {
          throw new Error(`Invalid definition for command: ${commandName}`);
        }
      }
    });

    // Integration Test 2: Command execution with real TypeScript commands
    await test('Real TypeScript command execution', async () => {
      const processor = new TypeScriptCommandProcessor({
        logLevel: 'error'
      });
      
      await processor.initialize();
      
      const commands = processor.getAllCommands();
      
      if (commands.length === 0) {
        console.log('  ⚠️  No TypeScript commands to test - skipping real execution test');
        return;
      }
      
      // Try to execute the first available command with empty params
      const testCommand = commands[0];
      console.log(`  🎯 Testing command: ${testCommand}`);
      
      const result = await processor.executeCommand(testCommand, {});
      
      // We expect either success or a proper error (not a crash)
      if (typeof result.success !== 'boolean') {
        throw new Error('Command result should have success property');
      }
      
      if (!result.message) {
        throw new Error('Command result should have message');
      }
      
      if (!result.timestamp) {
        throw new Error('Command result should have timestamp');
      }
      
      console.log(`  📊 Result: ${result.success ? 'SUCCESS' : 'FAILED'} - ${result.message}`);
    });

    console.log(`\n📊 Integration Test Results: ${passed} passed, ${failed} failed`);
    
    if (failed > 0) {
      throw new Error(`${failed} integration tests failed`);
    }
  }

  export async function runAllTests(): Promise<void> {
    try {
      await runUnitTests();
      await runIntegrationTests();
      console.log('🎉 All TypeScript Command Processor tests passed!');
    } catch (error) {
      console.error('❌ Test suite failed:', error.message);
      throw error;
    }
  }
}

// ============================================================================
// MODULE EXPORTS AND SINGLETON
// ============================================================================

// Export singleton instance with case-insensitive defaults
export const typeScriptCommandProcessor = new TypeScriptCommandProcessor({
  logLevel: 'info',
  enableCaseInsensitive: true, // Always case-insensitive
  enableTypeScriptOnly: false // Will be true once migration is complete
});

export default TypeScriptCommandProcessor;

// Enable running tests when module is executed directly
if (typeof process !== 'undefined' && process.argv?.[1]?.includes('TypeScriptCommandProcessor')) {
  TypeScriptCommandProcessorTests.runAllTests().catch(error => {
    console.error('❌ Test execution failed:', error);
    process.exit(1);
  });
}