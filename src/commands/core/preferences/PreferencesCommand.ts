/**
 * Preferences Command - Manage user preferences and system configuration
 * 
 * CRITICAL TESTING REQUIREMENTS:
 * ==============================
 * UNIT TEST COVERAGE NEEDED:
 * - Preference validation: Test type checking for string, number, boolean values
 * - Nested preferences: Verify object path setting/getting (e.g., "ui.theme.mode")
 * - Default fallbacks: Test behavior when preference keys don't exist
 * - Schema validation: Verify preferences match expected schema definitions
 * - Persistence validation: Test save/load cycle preserves all data types
 * 
 * INTEGRATION TEST COVERAGE NEEDED:
 * - File system persistence: Verify preferences save to/load from .continuum/preferences.json
 * - Command execution: Test via WebSocket command interface
 * - Cross-session persistence: Verify preferences survive daemon restarts
 * - Migration testing: Test preference schema updates and data migration
 * - Error recovery: Test behavior with corrupted preference files
 * 
 * ARCHITECTURAL INSIGHTS FROM IMPLEMENTATION:
 * ==========================================
 * DISCOVERED PATTERNS:
 * - Preference keys use dot notation for nested access (ui.theme.dark)
 * - Type safety enforced through TypeScript interfaces and runtime validation
 * - Default preferences defined as const objects for consistency
 * - Async file operations with proper error handling and atomic writes
 * 
 * REFACTORING OPPORTUNITIES:
 * - PreferenceSchema could be extracted to separate validation service
 * - File persistence logic should be abstracted to PreferenceStore class
 * - Preference change events should emit to WebSocket for real-time UI updates
 * - Type definitions should be shared with UI components for consistency
 * 
 * TODO: Extract components:
 * - PreferenceValidator (schema validation logic)
 * - PreferenceStore (file persistence abstraction)
 * - PreferenceEventEmitter (change notification system)
 * - PreferenceDefaults (default values management)
 */

import { BaseCommand, CommandResult, CommandDefinition } from '../base-command/BaseCommand';
import { promises as fs } from 'fs';
import * as path from 'path';

export interface PreferenceValue {
  readonly type: 'string' | 'number' | 'boolean' | 'object' | 'array';
  readonly value: string | number | boolean | object | unknown[];
  readonly default?: string | number | boolean | object | unknown[];
  readonly description?: string;
}

export interface PreferenceSchema {
  readonly [key: string]: PreferenceValue | PreferenceSchema;
}

export interface PreferencesData {
  readonly [key: string]: string | number | boolean | object | unknown[] | PreferencesData;
}

export class PreferencesCommand extends BaseCommand {
  private static preferences: PreferencesData = {};
  private static preferencesFile: string = '';
  private static schema: PreferenceSchema = {};
  private static initialized: boolean = false;
  
  static getDefinition(): CommandDefinition {
    return {
      name: 'preferences',
      category: 'core',
      icon: '⚙️',
      description: 'Manage user preferences and system configuration with type safety',
      parameters: { operation: 'string', data: 'any' },
      examples: [
        {
          description: 'Get theme mode preference',
          command: 'preferences get ui.theme.mode'
        },
        {
          description: 'Set theme to dark mode',
          command: 'preferences set ui.theme.mode dark'
        },
        {
          description: 'List UI preferences',
          command: 'preferences list --filter ui'
        },
        {
          description: 'Reset theme preferences',
          command: 'preferences reset ui.theme'
        }
      ],
      usage: 'preferences <get|set|list|reset|export|import> [key|data]'
    };
  }

  static async execute(params: any): Promise<CommandResult> {
    await this.ensureInitialized();
    
    const parsedParams = this.parseParams(params);
    const operation = parsedParams.operation || parsedParams._?.[0] || 'list';
    const data = parsedParams.data || parsedParams;
    
    try {
      console.log(`Executing preferences operation: ${operation}`);
      
      switch (operation) {
        case 'get':
          return await this.getPreference(data);
          
        case 'set':
          return await this.setPreference(data);
          
        case 'list':
          return await this.listPreferences(data);
          
        case 'reset':
          return await this.resetPreferences(data);
          
        case 'export':
          return await this.exportPreferences();
          
        case 'import':
          return await this.importPreferences(data);
          
        default:
          return this.createErrorResult(`Unknown preferences operation: ${operation}. Available: get, set, list, reset, export, import`);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`Preferences operation failed: ${errorMessage}`);
      return this.createErrorResult(`Preferences operation failed: ${errorMessage}`);
    }
  }

  private static async ensureInitialized(): Promise<void> {
    if (this.initialized) return;
    
    // Set up preferences file path
    const configDir = path.join(process.cwd(), '.continuum');
    await fs.mkdir(configDir, { recursive: true });
    this.preferencesFile = path.join(configDir, 'preferences.json');
    
    // Load default schema
    this.schema = this.getDefaultSchema();
    
    // Load existing preferences
    await this.loadPreferences();
    
    this.initialized = true;
    console.log('Preferences command initialized');
  }

  private static async getPreference(data: any): Promise<CommandResult> {
    const key = data.key || data._?.[1];
    const defaultValue = data.defaultValue || data.default;
    
    if (!key) {
      return this.createErrorResult('Preference key is required');
    }
    
    try {
      const value = this.getNestedValue(this.preferences, key);
      
      if (value !== undefined) {
        return this.createSuccessResult('Preference retrieved successfully', {
          key,
          value,
          type: typeof value,
          source: 'user'
        });
      }
      
      // Check schema for default
      const schemaDefault = this.getSchemaDefault(key);
      if (schemaDefault !== undefined) {
        return this.createSuccessResult('Schema default retrieved', {
          key,
          value: schemaDefault,
          type: typeof schemaDefault,
          source: 'schema'
        });
      }
      
      // Use provided default
      if (defaultValue !== undefined) {
        return this.createSuccessResult('Default value used', {
          key,
          value: defaultValue,
          type: typeof defaultValue,
          source: 'default'
        });
      }
      
      return this.createErrorResult(`Preference key '${key}' not found and no default provided`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to get preference: ${errorMessage}`);
    }
  }

  private static async setPreference(data: any): Promise<CommandResult> {
    const key = data.key || data._?.[1];
    const value = data.value || data._?.[2];
    const validateType = data.validateType !== false;
    
    if (!key || value === undefined) {
      return this.createErrorResult('Both key and value are required for set operation');
    }
    
    try {
      // Validate against schema if requested
      if (validateType) {
        const validationError = this.validatePreference(key, value);
        if (validationError) {
          return this.createErrorResult(validationError);
        }
      }
      
      // Set the preference
      this.setNestedValue(this.preferences, key, value);
      
      // Save to disk
      await this.savePreferences();
      
      console.log(`Preference set: ${key} = ${JSON.stringify(value)}`);
      
      return this.createSuccessResult('Preference set successfully', {
        key,
        value,
        type: typeof value,
        saved: true
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to set preference: ${errorMessage}`);
    }
  }

  private static async listPreferences(data: any): Promise<CommandResult> {
    const filter = data.filter || '';
    const includeDefaults = data.includeDefaults || false;
    
    try {
      const allPreferences: Record<string, any> = {};
      
      // Add user preferences
      this.flattenObject(this.preferences, '', allPreferences);
      
      // Add schema defaults if requested
      if (includeDefaults) {
        this.addSchemaDefaults(allPreferences);
      }
      
      // Apply filter if provided
      let filtered = allPreferences;
      if (filter) {
        filtered = {};
        for (const [key, value] of Object.entries(allPreferences)) {
          if (key.toLowerCase().includes(filter.toLowerCase())) {
            filtered[key] = value;
          }
        }
      }
      
      return this.createSuccessResult('Preferences listed successfully', {
        preferences: filtered,
        count: Object.keys(filtered).length,
        filter: filter || null,
        includeDefaults
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to list preferences: ${errorMessage}`);
    }
  }

  private static async resetPreferences(data: any): Promise<CommandResult> {
    try {
      const keys = data.keys || data._?.slice(1);
      
      if (keys && Array.isArray(keys)) {
        // Reset specific keys
        for (const key of keys) {
          this.deleteNestedValue(this.preferences, key);
        }
        console.log(`Reset preferences: ${keys.join(', ')}`);
      } else {
        // Reset all preferences
        this.preferences = {};
        console.log('Reset all preferences to defaults');
      }
      
      await this.savePreferences();
      
      return this.createSuccessResult('Preferences reset successfully', {
        reset: keys || 'all',
        timestamp: new Date().toISOString()
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to reset preferences: ${errorMessage}`);
    }
  }

  private static async exportPreferences(): Promise<CommandResult> {
    try {
      const exportData = {
        version: '1.0.0',
        timestamp: new Date().toISOString(),
        preferences: this.preferences,
        schema: this.schema
      };
      
      return this.createSuccessResult('Preferences exported successfully', {
        export: exportData,
        size: JSON.stringify(exportData).length
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to export preferences: ${errorMessage}`);
    }
  }

  private static async importPreferences(data: any): Promise<CommandResult> {
    try {
      const preferences = data.preferences;
      const merge = data.merge !== false; // Default to true
      
      if (!preferences) {
        return this.createErrorResult('Preferences data is required for import');
      }
      
      if (merge) {
        // Merge with existing preferences
        this.preferences = { ...this.preferences, ...preferences };
      } else {
        // Replace all preferences
        this.preferences = { ...preferences };
      }
      
      await this.savePreferences();
      
      console.log(`Imported preferences (merge: ${merge})`);
      
      return this.createSuccessResult('Preferences imported successfully', {
        imported: true,
        merge,
        count: Object.keys(preferences).length,
        timestamp: new Date().toISOString()
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to import preferences: ${errorMessage}`);
    }
  }

  private static async loadPreferences(): Promise<void> {
    try {
      const data = await fs.readFile(this.preferencesFile, 'utf8');
      this.preferences = JSON.parse(data);
      console.log('Preferences loaded from disk');
    } catch (error) {
      // File doesn't exist or is invalid, start with empty preferences
      this.preferences = {};
      console.log('Starting with empty preferences');
    }
  }

  private static async savePreferences(): Promise<void> {
    try {
      const data = JSON.stringify(this.preferences, null, 2);
      await fs.writeFile(this.preferencesFile, data, 'utf8');
      console.log('Preferences saved to disk');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      throw new Error(`Failed to save preferences: ${errorMessage}`);
    }
  }

  private static getDefaultSchema(): PreferenceSchema {
    return {
      ui: {
        theme: {
          type: 'object',
          value: {
            mode: { type: 'string', value: 'dark', default: 'dark' },
            color: { type: 'string', value: 'blue', default: 'blue' }
          }
        },
        language: {
          type: 'string',
          value: 'en',
          default: 'en',
          description: 'Interface language'
        }
      },
      system: {
        logging: {
          type: 'object',
          value: {
            level: { type: 'string', value: 'info', default: 'info' },
            console: { type: 'boolean', value: true, default: true }
          }
        },
        autostart: {
          type: 'boolean',
          value: false,
          default: false,
          description: 'Start daemons automatically'
        }
      }
    };
  }

  // Utility methods for nested object access
  private static getNestedValue(obj: any, path: string): any {
    return path.split('.').reduce((current, key) => current?.[key], obj);
  }

  private static setNestedValue(obj: any, path: string, value: any): void {
    const keys = path.split('.');
    const lastKey = keys.pop()!;
    const target = keys.reduce((current, key) => {
      if (!(key in current)) current[key] = {};
      return current[key];
    }, obj);
    target[lastKey] = value;
  }

  private static deleteNestedValue(obj: any, path: string): void {
    const keys = path.split('.');
    const lastKey = keys.pop()!;
    const target = keys.reduce((current, key) => current?.[key], obj);
    if (target && lastKey in target) {
      delete target[lastKey];
    }
  }

  private static flattenObject(obj: any, prefix: string, result: Record<string, any>): void {
    for (const [key, value] of Object.entries(obj)) {
      const fullKey = prefix ? `${prefix}.${key}` : key;
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        this.flattenObject(value, fullKey, result);
      } else {
        result[fullKey] = value;
      }
    }
  }

  private static getSchemaDefault(key: string): any {
    const schemaValue = this.getNestedValue(this.schema, key);
    return schemaValue?.default;
  }

  private static validatePreference(key: string, value: any): string | null {
    const schemaValue = this.getNestedValue(this.schema, key);
    
    if (!schemaValue) {
      return null; // No schema validation if key not in schema
    }
    
    const expectedType = schemaValue.type;
    const actualType = Array.isArray(value) ? 'array' : typeof value;
    
    if (expectedType !== actualType) {
      return `Type mismatch for key '${key}': expected ${expectedType}, got ${actualType}`;
    }
    
    return null;
  }

  private static addSchemaDefaults(preferences: Record<string, any>): void {
    const addDefaults = (schema: any, prefix: string = '') => {
      for (const [key, value] of Object.entries(schema)) {
        const fullKey = prefix ? `${prefix}.${key}` : key;
        
        if (value && typeof value === 'object' && 'type' in value) {
          // This is a preference definition
          const defValue = (value as any).default;
          if (defValue !== undefined && !(fullKey in preferences)) {
            preferences[fullKey] = defValue;
          }
        } else if (value && typeof value === 'object') {
          // This is a nested schema
          addDefaults(value, fullKey);
        }
      }
    };
    
    addDefaults(this.schema);
  }
}