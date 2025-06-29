/**
 * FluentAPI Module - Elegant command composition
 * Enables: continuum.screenshot().share(continuum.findUser({name:"joel"}))
 */

class FluentAPI {
  constructor(commandRegistry) {
    this.registry = commandRegistry;
    this.pipeline = [];
    
    // Dynamic method creation - auto-discover commands
    return this.createDynamicMethods();
  }

  createDynamicMethods() {
    try {
      // Validate registry first
      if (!this.registry || !this.registry.definitions) {
        throw new Error('Invalid command registry - missing definitions map');
      }
      
      if (this.registry.definitions.entries === undefined) {
        throw new Error('Command registry definitions is not a Map - cannot iterate entries');
      }
      
      try { console.log(`🔗 FluentAPI: Creating methods for ${this.registry.definitions.size} commands`); } catch(e) {}
      
      // Create fluent methods for all registered commands
      for (const [commandName, definition] of this.registry.definitions.entries()) {
        const methodName = commandName.toLowerCase();
        
        // Don't override existing methods
        if (this[methodName]) continue;
        
        // Create dynamic fluent method with validation
        this[methodName] = (params = {}) => {
          try {
            // Validate command exists and has proper definition
            if (!definition || !definition.name) {
              throw new Error(`Invalid command definition for ${commandName}`);
            }
            
            return this.chain(methodName, params);
          } catch (error) {
            console.error(`❌ FluentAPI method '${methodName}' failed:`, error.message);
            throw new Error(`FluentAPI: ${methodName}() failed - ${error.message}`);
          }
        };
      }

      // Return a proxy to catch unknown methods and suggest alternatives
      return new Proxy(this, {
        get(target, prop) {
          try {
            if (prop in target) {
              return target[prop];
            }
            
            // If method doesn't exist, suggest available commands
            if (typeof prop === 'string') {
              const availableCommands = target.registry?.definitions ? 
                Array.from(target.registry.definitions.keys()).map(cmd => cmd.toLowerCase()) : 
                ['No commands available - registry not initialized'];
              
              const similar = availableCommands.filter(cmd => 
                cmd.includes(prop.toLowerCase()) || prop.toLowerCase().includes(cmd)
              );
              
              const errorMsg = `Unknown fluent method: ${prop}. ` +
                (similar.length > 0 
                  ? `Did you mean: ${similar.join(', ')}?` 
                  : `Available commands: ${availableCommands.slice(0, 10).join(', ')}...`);
              
              console.error(`❌ FluentAPI: ${errorMsg}`);
              throw new Error(errorMsg);
            }
            
            return target[prop];
          } catch (error) {
            console.error(`❌ FluentAPI proxy error:`, error.message);
            throw error;
          }
        }
      });
    } catch (error) {
      console.error(`❌ FluentAPI initialization failed:`, error.message);
      throw new Error(`FluentAPI initialization failed: ${error.message}`);
    }
  }

  // Dynamic fluent methods - auto-discover from CommandRegistry
  // No god object! Commands are self-contained and auto-discovered

  // Chaining core
  chain(commandName, params = {}) {
    const newAPI = new FluentAPI(this.registry);
    newAPI.pipeline = [...this.pipeline, { command: commandName, params }];
    return newAPI;
  }

  // Execute the entire pipeline
  async execute(continuum = null) {
    let currentResult = null;
    
    try {
      if (!this.pipeline || this.pipeline.length === 0) {
        throw new Error('No commands in pipeline to execute');
      }

      console.log(`🔗 Executing: ${this.pipeline.map(p => p.command).join(' → ')}`);

      for (const [index, step] of this.pipeline.entries()) {
        const { command, params: stepParams } = step;
        
        try {
          // Validate step
          if (!command) {
            throw new Error(`Invalid command at step ${index + 1}: missing command name`);
          }

          const CommandClass = this.registry.getCommand(command);
          if (!CommandClass) {
            const availableCommands = Array.from(this.registry.definitions.keys()).slice(0, 5);
            throw new Error(`Command not found: ${command}. Available: ${availableCommands.join(', ')}...`);
          }

          // Validate and merge parameters
          let mergedParams;
          try {
            mergedParams = currentResult ? 
              { ...stepParams, input: currentResult } : stepParams;
            
            // Validate parameters against command definition
            const definition = this.registry.definitions.get(command.toUpperCase());
            if (definition && definition.parameters) {
              const validationResult = this.validateCommandParams(mergedParams, definition.parameters);
              if (!validationResult.valid) {
                throw new Error(`Invalid parameters: ${validationResult.errors.join(', ')}`);
              }
            }
          } catch (error) {
            throw new Error(`Parameter validation failed: ${error.message}`);
          }

          // Execute command
          console.log(`🔗 Step ${index + 1}: ${command}(${Object.keys(mergedParams).join(', ')})`);
          
          const result = await CommandClass.execute(JSON.stringify(mergedParams), continuum);
          
          if (!result || typeof result !== 'object') {
            throw new Error(`Invalid result format from ${command}`);
          }
          
          if (!result.success) {
            throw new Error(`${command} failed: ${result.message || 'Unknown error'}`);
          }
          
          currentResult = result.data || result;
          console.log(`✅ Step ${index + 1}: ${command} completed`);

        } catch (stepError) {
          console.error(`❌ Pipeline step ${index + 1} (${command}) failed:`, stepError.message);
          throw new Error(`Step ${index + 1} (${command}): ${stepError.message}`);
        }
      }

      console.log(`✅ Pipeline completed: ${this.pipeline.length} steps executed`);
      return currentResult;

    } catch (error) {
      console.error(`❌ FluentAPI pipeline execution failed:`, error.message);
      throw new Error(`Pipeline execution failed: ${error.message}`);
    }
  }

  // Validate command parameters against definition
  validateCommandParams(params, paramDefinition) {
    const errors = [];
    
    try {
      // Check required parameters
      for (const [paramName, paramConfig] of Object.entries(paramDefinition)) {
        if (paramConfig.required && !(paramName in params)) {
          errors.push(`Missing required parameter: ${paramName}`);
        }
        
        // Type validation
        if (paramName in params && paramConfig.type) {
          const value = params[paramName];
          const expectedType = paramConfig.type;
          
          if (expectedType === 'string' && typeof value !== 'string') {
            errors.push(`Parameter ${paramName} must be a string, got ${typeof value}`);
          } else if (expectedType === 'number' && typeof value !== 'number') {
            errors.push(`Parameter ${paramName} must be a number, got ${typeof value}`);
          } else if (expectedType === 'boolean' && typeof value !== 'boolean') {
            errors.push(`Parameter ${paramName} must be a boolean, got ${typeof value}`);
          } else if (expectedType === 'array' && !Array.isArray(value)) {
            errors.push(`Parameter ${paramName} must be an array, got ${typeof value}`);
          } else if (expectedType === 'object' && (typeof value !== 'object' || Array.isArray(value))) {
            errors.push(`Parameter ${paramName} must be an object, got ${typeof value}`);
          }
        }
      }
      
      return {
        valid: errors.length === 0,
        errors
      };
    } catch (error) {
      return {
        valid: false,
        errors: [`Parameter validation error: ${error.message}`]
      };
    }
  }

  // Predefined macros
  static createMacros(commandRegistry) {
    const api = new FluentAPI(commandRegistry);
    
    return {
      // Basic fluent API
      screenshot: (params) => api.screenshot(params),
      findUser: (query) => api.findUser(query),
      share: (target, params) => api.share(target, params),
      diagnostics: (type) => api.diagnostics(type),
      
      // Universal command composition - no repo-specific macros
      // Users create their own patterns:
      // continuum.screenshot().share(continuum.findUser({name:"joel"}))
      // This is the brilliant architecture - composable, not hardcoded
    };
  }
}

module.exports = FluentAPI;