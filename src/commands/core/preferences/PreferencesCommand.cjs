/**
 * PreferencesCommand - Access and modify user preferences
 */

const BaseCommand = require('../../BaseCommand.cjs');

class PreferencesCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'preferences',
      description: 'Access and modify user preferences',
      icon: '⚙️',
      parameters: {
        input: {
          type: 'object',
          required: false,
          description: 'User object from previous command in pipeline'
        },
        key: {
          type: 'string',
          required: false,
          description: 'Specific preference key to retrieve'
        },
        set: {
          type: 'object',
          required: false,
          description: 'Preferences to set'
        }
      },
      examples: [
        'preferences --key mediaInput',
        'preferences --set {"theme": "dark"}'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    try {
      // Get user from input (pipeline) or default to current user
      let user = options.input;
      
      if (!user) {
        // Default to current user context (in real system, from auth)
        user = {
          id: 'current',
          name: 'current_user',
          preferences: {
            mediaInput: 'terminal',
            notifications: true,
            theme: 'dark'
          }
        };
      }

      if (!user.preferences) {
        return this.createErrorResult('User has no preferences', user);
      }

      // If setting preferences
      if (options.set) {
        user.preferences = { ...user.preferences, ...options.set };
        console.log(`⚙️ Updated preferences for ${user.name || user.id}`);
        return this.createSuccessResult(user, 'Preferences updated');
      }

      // If getting specific key
      if (options.key) {
        const value = user.preferences[options.key];
        if (value === undefined) {
          return this.createErrorResult(`Preference key '${options.key}' not found`);
        }
        console.log(`⚙️ Retrieved preference: ${options.key} = ${value}`);
        return this.createSuccessResult(value, `Retrieved ${options.key}`);
      }

      // Return all preferences
      console.log(`⚙️ Retrieved all preferences for ${user.name || user.id}`);
      return this.createSuccessResult(user.preferences, 'Retrieved all preferences');

    } catch (error) {
      return this.createErrorResult('Preferences access failed', error.message);
    }
  }
}

module.exports = PreferencesCommand;