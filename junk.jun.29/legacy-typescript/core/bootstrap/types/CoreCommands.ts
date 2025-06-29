/**
 * Core Commands Enum
 * Defines all bootstrap/core commands that are fundamental to system operation
 */

export enum CoreCommand {
  // IMMEDIATE - Available before any discovery
  INFO = 'info',
  STATUS = 'status',
  
  // POST-DISCOVERY - Need module discovery complete  
  LIST = 'list',
  HELP = 'help',
  
  // ALWAYS-AVAILABLE - Core filesystem operations
  FILESAVE = 'filesave',
  FILEREAD = 'fileread',
  
  // SYSTEM CONTROL - Core system operations
  RESTART = 'restart',
  RELOAD = 'reload',
  SHUTDOWN = 'shutdown'
}

export enum CommandCategory {
  IMMEDIATE = 'immediate',
  POST_DISCOVERY = 'post-discovery', 
  ALWAYS_AVAILABLE = 'always-available',
  SYSTEM_CONTROL = 'system-control'
}

export interface CoreCommandDefinition {
  command: CoreCommand;
  category: CommandCategory;
  description: string;
  requiresInitialization: boolean;
  priority: 'critical' | 'high' | 'medium' | 'low';
}

/**
 * Core command definitions with metadata
 */
export const CORE_COMMAND_DEFINITIONS: CoreCommandDefinition[] = [
  {
    command: CoreCommand.INFO,
    category: CommandCategory.IMMEDIATE,
    description: 'System information (available immediately)',
    requiresInitialization: false,
    priority: 'high'
  },
  {
    command: CoreCommand.STATUS,
    category: CommandCategory.IMMEDIATE,
    description: 'System status (available immediately)', 
    requiresInitialization: false,
    priority: 'high'
  },
  {
    command: CoreCommand.LIST,
    category: CommandCategory.POST_DISCOVERY,
    description: 'List all commands (requires module discovery)',
    requiresInitialization: true,
    priority: 'critical'
  },
  {
    command: CoreCommand.HELP,
    category: CommandCategory.POST_DISCOVERY,
    description: 'Command help (requires module discovery)',
    requiresInitialization: true,
    priority: 'high'
  },
  {
    command: CoreCommand.FILESAVE,
    category: CommandCategory.ALWAYS_AVAILABLE,
    description: 'Save file (always available)',
    requiresInitialization: false,
    priority: 'medium'
  },
  {
    command: CoreCommand.FILEREAD,
    category: CommandCategory.ALWAYS_AVAILABLE,
    description: 'Read file (always available)',
    requiresInitialization: false,
    priority: 'medium'
  },
  {
    command: CoreCommand.RESTART,
    category: CommandCategory.SYSTEM_CONTROL,
    description: 'Restart system',
    requiresInitialization: false,
    priority: 'critical'
  },
  {
    command: CoreCommand.RELOAD,
    category: CommandCategory.SYSTEM_CONTROL,
    description: 'Reload system configuration',
    requiresInitialization: false,
    priority: 'high'
  },
  {
    command: CoreCommand.SHUTDOWN,
    category: CommandCategory.SYSTEM_CONTROL,
    description: 'Shutdown system gracefully',
    requiresInitialization: false,
    priority: 'critical'
  }
];

/**
 * Helper functions for working with core commands
 */
export class CoreCommandUtils {
  static isCoreCommand(command: string): boolean {
    return Object.values(CoreCommand).includes(command as CoreCommand);
  }
  
  static getCoreCommandDefinition(command: CoreCommand): CoreCommandDefinition | undefined {
    return CORE_COMMAND_DEFINITIONS.find(def => def.command === command);
  }
  
  static getCommandsByCategory(category: CommandCategory): CoreCommand[] {
    return CORE_COMMAND_DEFINITIONS
      .filter(def => def.category === category)
      .map(def => def.command);
  }
  
  static getImmediateCommands(): CoreCommand[] {
    return this.getCommandsByCategory(CommandCategory.IMMEDIATE);
  }
  
  static getPostDiscoveryCommands(): CoreCommand[] {
    return this.getCommandsByCategory(CommandCategory.POST_DISCOVERY);
  }
  
  static getAlwaysAvailableCommands(): CoreCommand[] {
    return this.getCommandsByCategory(CommandCategory.ALWAYS_AVAILABLE);
  }
  
  static getSystemControlCommands(): CoreCommand[] {
    return this.getCommandsByCategory(CommandCategory.SYSTEM_CONTROL);
  }
  
  static requiresInitialization(command: CoreCommand): boolean {
    const definition = this.getCoreCommandDefinition(command);
    return definition?.requiresInitialization ?? false;
  }
  
  static getAllCoreCommands(): CoreCommand[] {
    return Object.values(CoreCommand);
  }
  
  static validateCoreCommand(command: string): CoreCommand {
    if (!this.isCoreCommand(command)) {
      throw new Error(`Not a core command: ${command}`);
    }
    return command as CoreCommand;
  }
}