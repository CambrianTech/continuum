/**
 * PersonaToolRegistry.ts
 *
 * Registry for discovering and managing available tools for PersonaUsers.
 * Handles tool lookup, permission checks, and capability discovery.
 *
 * Part of Phase 3A: Tool Calling Foundation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { ToolDefinition } from './PersonaToolDefinitions';
import {
  getAllToolDefinitions,
  getAllToolDefinitionsAsync,
  getToolDefinition,
  formatToolForAI,
  formatAllToolsForAI
} from './PersonaToolDefinitions';

/**
 * Permission set for a PersonaUser
 */
export interface PersonaPermissions {
  personaId: UUID;
  permissions: Set<string>;
  role: 'assistant' | 'admin' | 'restricted';
}

/**
 * Registry for tool discovery and permission management
 */
export class PersonaToolRegistry {
  private permissionsByPersona: Map<UUID, PersonaPermissions> = new Map();

  constructor() {
    // Initialize with default permissions
  }

  /**
   * List all available tools
   */
  listTools(): ToolDefinition[] {
    return getAllToolDefinitions();
  }

  /**
   * List tools available to a specific persona based on permissions
   */
  listToolsForPersona(personaId: UUID): ToolDefinition[] {
    const permissions = this.getPermissionsForPersona(personaId);
    return this.listTools().filter(tool =>
      this.hasRequiredPermissions(permissions.permissions, tool.permissions)
    );
  }

  /**
   * List tools with guaranteed cache initialization (async)
   * Use this in critical paths where tools must be available
   */
  async listToolsAsync(): Promise<ToolDefinition[]> {
    return getAllToolDefinitionsAsync();
  }

  /**
   * List tools for persona with guaranteed cache initialization (async)
   */
  async listToolsForPersonaAsync(personaId: UUID): Promise<ToolDefinition[]> {
    const permissions = this.getPermissionsForPersona(personaId);
    const allTools = await getAllToolDefinitionsAsync();
    return allTools.filter(tool =>
      this.hasRequiredPermissions(permissions.permissions, tool.permissions)
    );
  }

  /**
   * Get tool definition by name
   */
  getTool(name: string): ToolDefinition | null {
    return getToolDefinition(name);
  }

  /**
   * Check if persona can use a specific tool
   */
  canUse(toolName: string, personaId: UUID): boolean {
    const tool = this.getTool(toolName);
    if (!tool) {
      return false;
    }

    const permissions = this.getPermissionsForPersona(personaId);
    return this.hasRequiredPermissions(permissions.permissions, tool.permissions);
  }

  /**
   * Register persona with permissions
   */
  registerPersona(
    personaId: UUID,
    role: 'assistant' | 'admin' | 'restricted' = 'assistant'
  ): void {
    const permissions = this.getDefaultPermissionsForRole(role);
    this.permissionsByPersona.set(personaId, {
      personaId,
      permissions: new Set(permissions),
      role
    });
  }

  /**
   * Grant permission to persona
   */
  grantPermission(personaId: UUID, permission: string): void {
    const perms = this.getPermissionsForPersona(personaId);
    perms.permissions.add(permission);
  }

  /**
   * Revoke permission from persona
   */
  revokePermission(personaId: UUID, permission: string): void {
    const perms = this.getPermissionsForPersona(personaId);
    perms.permissions.delete(permission);
  }

  /**
   * Get all permissions for a persona
   */
  getPermissionsForPersona(personaId: UUID): PersonaPermissions {
    let perms = this.permissionsByPersona.get(personaId);
    if (!perms) {
      // Auto-register with assistant role if not found
      this.registerPersona(personaId, 'assistant');
      perms = this.permissionsByPersona.get(personaId)!;
    }
    return perms;
  }

  /**
   * Format tools for AI system prompt
   */
  formatToolsForAI(personaId?: UUID): string {
    if (personaId) {
      const availableTools = this.listToolsForPersona(personaId);
      if (availableTools.length === 0) {
        return 'No tools available for this persona.';
      }

      let output = 'Available Tools:\n\n';
      for (const tool of availableTools) {
        output += formatToolForAI(tool) + '\n---\n\n';
      }

      output += `
Usage Format (Anthropic Claude XML style):
<tool_use>
  <tool_name>read</tool_name>
  <parameters>
    <filepath>/path/to/file.ts</filepath>
  </parameters>
</tool_use>

When you need information, use tools instead of making assumptions.
`;

      return output;
    }

    return formatAllToolsForAI();
  }

  /**
   * Get default permissions for a role
   */
  private getDefaultPermissionsForRole(role: string): string[] {
    switch (role) {
      case 'admin':
        // Admin: unrestricted access to all tool categories
        return [
          'file:execute',
          'code:execute',
          'data:execute',
          'media:execute',
          'system:execute',
        ];

      case 'assistant':
        // Assistant: full tool access (tools are the persona's hands)
        // Permission scoping happens at the command level (PRIVILEGED_COMMANDS, ADMIN_COMMANDS)
        // not at the category level — personas need all categories to function.
        return [
          'file:execute',
          'code:execute',
          'data:execute',
          'media:execute',
          'system:execute',
        ];

      case 'restricted':
        // Restricted: read-only, no code execution
        return [
          'file:execute',
          'data:execute',
          'system:execute',
        ];

      default:
        return ['file:read'];
    }
  }

  /**
   * Check if permission set includes all required permissions
   */
  private hasRequiredPermissions(
    userPermissions: Set<string>,
    requiredPermissions: string[]
  ): boolean {
    return requiredPermissions.every(perm => userPermissions.has(perm));
  }

  /**
   * Get tools by category
   */
  getToolsByCategory(category: string): ToolDefinition[] {
    return this.listTools().filter(tool => tool.category === category);
  }

  /**
   * Search tools by keyword
   */
  searchTools(keyword: string): ToolDefinition[] {
    const lowerKeyword = keyword.toLowerCase();
    return this.listTools().filter(tool =>
      tool.name.toLowerCase().includes(lowerKeyword) ||
      tool.description.toLowerCase().includes(lowerKeyword)
    );
  }

  /**
   * Get statistics about registered personas and permissions
   */
  getStats(): {
    totalPersonas: number;
    totalTools: number;
    permissionsByRole: Record<string, number>;
  } {
    const permissionsByRole: Record<string, number> = {
      admin: 0,
      assistant: 0,
      restricted: 0
    };

    for (const perms of this.permissionsByPersona.values()) {
      permissionsByRole[perms.role]++;
    }

    return {
      totalPersonas: this.permissionsByPersona.size,
      totalTools: this.listTools().length,
      permissionsByRole
    };
  }

  /**
   * Shared instance (singleton pattern)
   */
  private static _sharedInstance: PersonaToolRegistry | null = null;

  static sharedInstance(): PersonaToolRegistry {
    if (!PersonaToolRegistry._sharedInstance) {
      PersonaToolRegistry._sharedInstance = new PersonaToolRegistry();
    }
    return PersonaToolRegistry._sharedInstance;
  }

  /**
   * Reset shared instance (for testing)
   */
  static resetSharedInstance(): void {
    PersonaToolRegistry._sharedInstance = null;
  }
}
