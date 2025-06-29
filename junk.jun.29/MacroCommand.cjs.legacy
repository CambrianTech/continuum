/**
 * MacroCommand - Composable command chaining for elegant workflows
 * Enables fluent API like: continuum.screenshot().share(continuum.findUser(...))
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class MacroCommand extends BaseCommand {
  constructor(initialResult = null) {
    super();
    this.result = initialResult;
    this.pipeline = [];
  }

  static getDefinition() {
    return {
      name: 'macro',
      description: 'Composable command chaining for elegant workflows',
      icon: '🔗',
      parameters: {
        commands: {
          type: 'array',
          required: true,
          description: 'Array of commands to chain together'
        }
      },
      examples: [
        'macro --commands ["screenshot", "share"]',
        'screenshot().share(findUser("joel"))'
      ]
    };
  }

  // Fluent API methods
  screenshot(params = {}) {
    return this.chain('screenshot', params);
  }

  share(target, params = {}) {
    return this.chain('share', { target, ...params });
  }

  findUser(query) {
    return this.chain('findUser', query);
  }

  preferences() {
    return this.chain('preferences', {});
  }

  mediaInput() {
    return this.chain('mediaInput', {});
  }

  // Core chaining method
  chain(commandName, params = {}) {
    this.pipeline.push({ command: commandName, params });
    return this; // Return this for chaining
  }

  // Execute the entire pipeline
  async execute(params = {}, continuum = null) {
    try {
      // If params is a string, try to parse as JSON
      const options = typeof params === 'string' ? this.parseParams(params) : params;
      
      // If we have explicit commands in options, use those
      if (options.commands) {
        this.pipeline = options.commands.map(cmd => 
          typeof cmd === 'string' ? { command: cmd, params: {} } : cmd
        );
      }

      let currentResult = this.result;
      
      console.log(`🔗 Executing macro pipeline: ${this.pipeline.map(p => p.command).join(' → ')}`);

      // Execute each command in the pipeline
      for (const step of this.pipeline) {
        const { command, params: stepParams } = step;
        
        try {
          // Get the command class
          const CommandClass = this.getCommandClass(command);
          if (!CommandClass) {
            throw new Error(`Unknown command: ${command}`);
          }

          // Merge previous result into params if it exists
          const mergedParams = currentResult ? 
            { ...stepParams, input: currentResult } : stepParams;

          // Execute the command
          console.log(`  🔹 ${command}(${JSON.stringify(mergedParams).substring(0, 50)}...)`);
          currentResult = await CommandClass.execute(JSON.stringify(mergedParams), continuum);
          
          if (!currentResult.success) {
            throw new Error(`Command '${command}' failed: ${currentResult.message}`);
          }
          
          // Extract data for next command
          currentResult = currentResult.data || currentResult;
          
        } catch (error) {
          console.error(`❌ Pipeline failed at '${command}':`, error.message);
          return this.createErrorResult(`Pipeline failed at '${command}'`, error.message);
        }
      }

      console.log(`✅ Macro pipeline completed successfully`);
      return this.createSuccessResult(currentResult, 'Macro pipeline executed');

    } catch (error) {
      console.error('❌ Macro execution failed:', error);
      return this.createErrorResult('Macro execution failed', error.message);
    }
  }

  // Helper to get command classes
  getCommandClass(commandName) {
    const commandMap = {
      'screenshot': () => require('./ScreenshotCommand.cjs'),
      'share': () => require('./ShareCommand.cjs'),
      'findUser': () => require('./FindUserCommand.cjs'),
      'preferences': () => require('./PreferencesCommand.cjs'),
      'mediaInput': () => require('./MediaInputCommand.cjs'),
      'diagnostics': () => require('./DiagnosticsCommand.cjs'),
      'help': () => require('./HelpCommand.cjs'),
      'agents': () => require('./AgentsCommand.cjs'),
      'restart': () => require('./RestartCommand.cjs')
    };

    const factory = commandMap[commandName];
    return factory ? factory() : null;
  }

  // Static factory methods for fluent API
  static screenshot(params = {}) {
    return new MacroCommand().screenshot(params);
  }

  static findUser(query) {
    return new MacroCommand().findUser(query);
  }

  static share(target, params = {}) {
    return new MacroCommand().share(target, params);
  }
}

// Add fluent API to the global continuum object
MacroCommand.createFluentAPI = function(continuum) {
  return {
    screenshot: (params) => MacroCommand.screenshot(params),
    findUser: (query) => MacroCommand.findUser(query),
    share: (target, params) => MacroCommand.share(target, params),
    
    // Macro compositions
    shareScreenshot: (user) => 
      MacroCommand.screenshot()
        .share(MacroCommand.findUser(user).preferences().mediaInput()),
    
    shareScreenshotToJoel: () =>
      MacroCommand.screenshot()
        .share(MacroCommand.findUser({name: "joel"}).preferences().mediaInput()),
    
    quickDiagnostics: () =>
      MacroCommand.screenshot()
        .chain('diagnostics', { type: 'screenshot' })
        .share(MacroCommand.findUser({name: "joel"}).preferences().mediaInput())
  };
};

module.exports = MacroCommand;