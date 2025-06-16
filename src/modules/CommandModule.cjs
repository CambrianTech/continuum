/**
 * CommandModule - Base class for modular command systems
 * Each module can provide commands, macros, and fluent APIs
 */

class CommandModule {
  constructor(name, version = '1.0.0') {
    this.name = name;
    this.version = version;
    this.commands = new Map();
    this.macros = new Map();
  }

  // Register a command with this module
  registerCommand(name, CommandClass) {
    this.commands.set(name, CommandClass);
    return this;
  }

  // Register a macro with this module
  registerMacro(name, macroFunction) {
    this.macros.set(name, macroFunction);
    return this;
  }

  // Get command by name
  getCommand(name) {
    return this.commands.get(name);
  }

  // Get macro by name
  getMacro(name) {
    return this.macros.get(name);
  }

  // Get all commands provided by this module
  getCommands() {
    return Array.from(this.commands.entries());
  }

  // Get all macros provided by this module
  getMacros() {
    return Array.from(this.macros.entries());
  }

  // Initialize the module (override in subclasses)
  async initialize(continuum) {
    console.log(`📦 Initializing module: ${this.name} v${this.version}`);
    return true;
  }

  // Cleanup the module (override in subclasses)
  async cleanup() {
    console.log(`📦 Cleaning up module: ${this.name}`);
    return true;
  }

  // Module metadata
  getInfo() {
    return {
      name: this.name,
      version: this.version,
      commands: this.commands.size,
      macros: this.macros.size
    };
  }
}

module.exports = CommandModule;