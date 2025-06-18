/**
 * BaseCommand - Elegant command interface pattern
 * Provides consistent structure for all continuum commands
 */

class BaseCommand {
  static getDefinition() {
    // Override in subclasses
    throw new Error('getDefinition must be implemented by subclass');
  }
  
  static async execute(params, continuum) {
    // Override in subclasses
    throw new Error('execute must be implemented by subclass');
  }
  
  // Elegant helper methods for all commands
  static parseParams(params) {
    try {
      if (!params) return {};
      
      // If already an object, return as-is
      if (typeof params === 'object' && !Array.isArray(params)) {
        return params;
      }
      
      // If string, parse as JSON
      if (typeof params === 'string') {
        // Handle empty string
        if (params.trim() === '') {
          return {};
        }
        return JSON.parse(params);
      }
      
      console.warn(`⚠️ Invalid parameter type: ${typeof params}. Expected object or JSON string.`);
      return {};
    } catch (error) {
      console.warn(`⚠️ Parameter parsing failed: ${error.message}. Returning empty object.`);
      return {};
    }
  }

  // Validate parameters against command definition
  static validateParams(params, definition) {
    try {
      if (!definition || !definition.parameters) {
        return { valid: true, errors: [] };
      }

      const errors = [];
      const paramDefinition = definition.parameters;

      // Check required parameters
      for (const [paramName, paramConfig] of Object.entries(paramDefinition)) {
        if (paramConfig.required && !(paramName in params)) {
          errors.push(`Missing required parameter: ${paramName}`);
        }
        
        // Type validation
        if (paramName in params && paramConfig.type) {
          const value = params[paramName];
          const expectedType = paramConfig.type;
          
          if (!this.isValidType(value, expectedType)) {
            errors.push(`Parameter '${paramName}' must be ${expectedType}, got ${typeof value}`);
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
        errors: [`Validation error: ${error.message}`]
      };
    }
  }

  // Type checking helper
  static isValidType(value, expectedType) {
    switch (expectedType) {
      case 'string':
        return typeof value === 'string';
      case 'number':
        return typeof value === 'number' && !isNaN(value);
      case 'boolean':
        return typeof value === 'boolean';
      case 'array':
        return Array.isArray(value);
      case 'object':
        return typeof value === 'object' && !Array.isArray(value) && value !== null;
      default:
        return true; // Unknown types pass validation
    }
  }
  
  static createSuccessResult(data, message = 'Success') {
    return {
      success: true,
      message,
      data,
      timestamp: new Date().toISOString()
    };
  }
  
  static createErrorResult(message, error = null) {
    return {
      success: false,
      message,
      error,
      timestamp: new Date().toISOString()
    };
  }
}

module.exports = BaseCommand;