/**
 * FindUserCommand - Find users in the system by various criteria
 */

const BaseCommand = require('../../BaseCommand.cjs');

class FindUserCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'findUser',
      description: 'Find users in the system by name, preferences, or other criteria',
      icon: '🔍',
      parameters: {
        name: {
          type: 'string',
          required: false,
          description: 'User name to search for'
        },
        role: {
          type: 'string', 
          required: false,
          description: 'User role filter'
        },
        active: {
          type: 'boolean',
          required: false,
          description: 'Filter by active status'
        }
      },
      examples: [
        'findUser --name joel',
        'findUser --role admin --active true'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    try {
      // In a real system, this would query a user database
      // For now, return mock users
      const mockUsers = [
        {
          id: 'joel',
          name: 'joel',
          role: 'admin',
          active: true,
          preferences: {
            mediaInput: 'slack',
            notifications: true,
            theme: 'dark'
          }
        },
        {
          id: 'claude',
          name: 'claude', 
          role: 'agent',
          active: true,
          preferences: {
            mediaInput: 'terminal',
            notifications: false,
            theme: 'cyberpunk'
          }
        }
      ];

      let filteredUsers = mockUsers;

      // Apply filters
      if (options.name) {
        filteredUsers = filteredUsers.filter(user => 
          user.name.toLowerCase().includes(options.name.toLowerCase())
        );
      }

      if (options.role) {
        filteredUsers = filteredUsers.filter(user => user.role === options.role);
      }

      if (options.active !== undefined) {
        filteredUsers = filteredUsers.filter(user => user.active === options.active);
      }

      if (filteredUsers.length === 0) {
        return this.createErrorResult('No users found matching criteria', options);
      }

      // Return single user if exactly one match, otherwise return array
      const result = filteredUsers.length === 1 ? filteredUsers[0] : filteredUsers;

      console.log(`🔍 Found ${filteredUsers.length} user(s) matching criteria`);
      
      return this.createSuccessResult(result, `Found ${filteredUsers.length} user(s)`);

    } catch (error) {
      return this.createErrorResult('User search failed', error.message);
    }
  }
}

module.exports = FindUserCommand;